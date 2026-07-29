//! Raw DEFLATE inflate (RFC 1951).

use super::DecompressError;
use super::bit::BitReader;
use super::huffman::HuffmanDecoder;
use alloc::vec;
use alloc::vec::Vec;

/// Sliding window size (RFC 1951 §2).
const WINDOW: usize = 32_768;

/// Length base / extra bits for codes 257..=285 (RFC 1951 §3.2.5).
#[allow(clippy::integer_division)] // table derived from RFC ranges
fn length_base_extra(code: u16) -> Result<(u16, u8), DecompressError> {
  // code 257..285
  const BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
  ];
  const EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
  ];
  let idx = usize::from(code.saturating_sub(257));
  let base = *BASE.get(idx).ok_or(DecompressError::InvalidInput)?;
  let extra = *EXTRA.get(idx).ok_or(DecompressError::InvalidInput)?;
  Ok((base, extra))
}

/// Distance base / extra bits for codes 0..=29 (RFC 1951 §3.2.5).
fn dist_base_extra(code: u16) -> Result<(u16, u8), DecompressError> {
  const BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049,
    3073, 4097, 6145, 8193, 12289, 16385, 24577,
  ];
  const EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13,
  ];
  let idx = usize::from(code);
  let base = *BASE.get(idx).ok_or(DecompressError::InvalidInput)?;
  let extra = *EXTRA.get(idx).ok_or(DecompressError::InvalidInput)?;
  Ok((base, extra))
}

/// Code-length alphabet order (RFC 1951 §3.2.7).
const CL_ORDER: [u8; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

/// Inflate a raw DEFLATE stream. Returns `(output, bytes_consumed)`.
pub(super) fn inflate(
  data: &[u8],
  max_out: usize,
) -> Result<(Vec<u8>, usize), DecompressError> {
  let mut bits = BitReader::new(data);
  let mut out = Vec::new();
  loop {
    let bfinal = bits.get_bits(1)?;
    let btype = bits.get_bits(2)?;
    match btype {
      0 => inflate_stored(&mut bits, &mut out, max_out)?,
      1 => {
        let lit = fixed_lit_decoder()?;
        let dist = fixed_dist_decoder()?;
        inflate_compressed(&mut bits, &mut out, max_out, &lit, &dist)?;
      },
      2 => {
        let (lit, dist) = read_dynamic_trees(&mut bits)?;
        inflate_compressed(&mut bits, &mut out, max_out, &lit, &dist)?;
      },
      _ => return Err(DecompressError::InvalidInput),
    }
    if bfinal != 0 {
      break;
    }
  }
  bits.align_to_byte();
  Ok((out, bits.byte_pos()))
}

fn push_byte(
  out: &mut Vec<u8>,
  max_out: usize,
  byte: u8,
) -> Result<(), DecompressError> {
  if out.len() >= max_out {
    return Err(DecompressError::LimitExceeded);
  }
  out.push(byte);
  Ok(())
}

fn inflate_stored(
  bits: &mut BitReader<'_>,
  out: &mut Vec<u8>,
  max_out: usize,
) -> Result<(), DecompressError> {
  bits.align_to_byte();
  let len_lo = u16::from(bits.get_aligned_byte()?);
  let len_hi = u16::from(bits.get_aligned_byte()?);
  let nlen_lo = u16::from(bits.get_aligned_byte()?);
  let nlen_hi = u16::from(bits.get_aligned_byte()?);
  let len = len_lo | (len_hi << 8);
  let nlen = nlen_lo | (nlen_hi << 8);
  if nlen != (len ^ 0xffff) {
    return Err(DecompressError::InvalidInput);
  }
  let mut i = 0u16;
  while i < len {
    let b = bits.get_aligned_byte()?;
    push_byte(out, max_out, b)?;
    i = i.saturating_add(1);
  }
  Ok(())
}

fn fixed_lit_decoder() -> Result<HuffmanDecoder, DecompressError> {
  // RFC 1951 §3.2.6
  let mut lengths = vec![0u8; 288];
  let mut i = 0usize;
  while i <= 143 {
    if let Some(slot) = lengths.get_mut(i) {
      *slot = 8;
    }
    i = i.saturating_add(1);
  }
  while i <= 255 {
    if let Some(slot) = lengths.get_mut(i) {
      *slot = 9;
    }
    i = i.saturating_add(1);
  }
  while i <= 279 {
    if let Some(slot) = lengths.get_mut(i) {
      *slot = 7;
    }
    i = i.saturating_add(1);
  }
  while i <= 287 {
    if let Some(slot) = lengths.get_mut(i) {
      *slot = 8;
    }
    i = i.saturating_add(1);
  }
  HuffmanDecoder::from_lengths(&lengths)
}

fn fixed_dist_decoder() -> Result<HuffmanDecoder, DecompressError> {
  let lengths = vec![5u8; 32];
  HuffmanDecoder::from_lengths(&lengths)
}

fn read_dynamic_trees(
  bits: &mut BitReader<'_>,
) -> Result<(HuffmanDecoder, HuffmanDecoder), DecompressError> {
  // RFC 1951 §3.2.7
  let hlit = bits.get_bits(5)?.saturating_add(257);
  let hdist = bits.get_bits(5)?.saturating_add(1);
  let hclen = bits.get_bits(4)?.saturating_add(4);

  let mut cl_lengths = [0u8; 19];
  let mut i = 0u32;
  while i < hclen {
    let len = u8::try_from(bits.get_bits(3)?).map_err(|_| DecompressError::InvalidInput)?;
    let ord = CL_ORDER
      .get(usize::try_from(i).map_err(|_| DecompressError::InvalidInput)?)
      .copied()
      .ok_or(DecompressError::InvalidInput)?;
    if let Some(slot) = cl_lengths.get_mut(usize::from(ord)) {
      *slot = len;
    }
    i = i.saturating_add(1);
  }
  let cl_dec = HuffmanDecoder::from_lengths(&cl_lengths)?;

  let total = usize::try_from(hlit.saturating_add(hdist)).map_err(|_| DecompressError::InvalidInput)?;
  let mut all_lens = vec![0u8; total];
  let mut n = 0usize;
  while n < total {
    let sym = cl_dec.decode(bits)?;
    match sym {
      0..=15 => {
        let val = u8::try_from(sym).map_err(|_| DecompressError::InvalidInput)?;
        if let Some(slot) = all_lens.get_mut(n) {
          *slot = val;
        }
        n = n.saturating_add(1);
      },
      16 => {
        let rep = usize::try_from(bits.get_bits(2)?.saturating_add(3)).map_err(|_| DecompressError::InvalidInput)?;
        let prev = if n == 0 {
          return Err(DecompressError::InvalidInput);
        } else {
          all_lens.get(n.saturating_sub(1)).copied().unwrap_or(0)
        };
        let mut r = 0usize;
        while r < rep {
          if n >= total {
            return Err(DecompressError::InvalidInput);
          }
          if let Some(slot) = all_lens.get_mut(n) {
            *slot = prev;
          }
          n = n.saturating_add(1);
          r = r.saturating_add(1);
        }
      },
      17 => {
        let rep = usize::try_from(bits.get_bits(3)?.saturating_add(3)).map_err(|_| DecompressError::InvalidInput)?;
        let mut r = 0usize;
        while r < rep {
          if n >= total {
            return Err(DecompressError::InvalidInput);
          }
          if let Some(slot) = all_lens.get_mut(n) {
            *slot = 0;
          }
          n = n.saturating_add(1);
          r = r.saturating_add(1);
        }
      },
      18 => {
        let rep = usize::try_from(bits.get_bits(7)?.saturating_add(11)).map_err(|_| DecompressError::InvalidInput)?;
        let mut r = 0usize;
        while r < rep {
          if n >= total {
            return Err(DecompressError::InvalidInput);
          }
          if let Some(slot) = all_lens.get_mut(n) {
            *slot = 0;
          }
          n = n.saturating_add(1);
          r = r.saturating_add(1);
        }
      },
      _ => return Err(DecompressError::InvalidInput),
    }
  }

  let lit_n = usize::try_from(hlit).map_err(|_| DecompressError::InvalidInput)?;
  let lit_lens = all_lens.get(..lit_n).ok_or(DecompressError::InvalidInput)?;
  let dist_lens = all_lens.get(lit_n..).ok_or(DecompressError::InvalidInput)?;
  let lit = HuffmanDecoder::from_lengths(lit_lens)?;
  let dist = HuffmanDecoder::from_lengths(dist_lens)?;
  Ok((lit, dist))
}

fn inflate_compressed(
  bits: &mut BitReader<'_>,
  out: &mut Vec<u8>,
  max_out: usize,
  lit: &HuffmanDecoder,
  dist: &HuffmanDecoder,
) -> Result<(), DecompressError> {
  loop {
    let sym = lit.decode(bits)?;
    if sym < 256 {
      let byte = u8::try_from(sym).map_err(|_| DecompressError::InvalidInput)?;
      push_byte(out, max_out, byte)?;
      continue;
    }
    if sym == 256 {
      return Ok(());
    }
    // Length code 257..=285
    if sym > 285 {
      return Err(DecompressError::InvalidInput);
    }
    let (base_len, extra_len) = length_base_extra(sym)?;
    let len = base_len.saturating_add(u16::try_from(bits.get_bits(extra_len)?).map_err(|_| DecompressError::InvalidInput)?);
    let dsym = dist.decode(bits)?;
    if dsym > 29 {
      return Err(DecompressError::InvalidInput);
    }
    let (base_dist, extra_dist) = dist_base_extra(dsym)?;
    let distance = base_dist.saturating_add(u16::try_from(bits.get_bits(extra_dist)?).map_err(|_| DecompressError::InvalidInput)?);
    copy_match(out, max_out, usize::from(distance), usize::from(len))?;
  }
}

fn copy_match(
  out: &mut Vec<u8>,
  max_out: usize,
  distance: usize,
  length: usize,
) -> Result<(), DecompressError> {
  if distance == 0 || distance > out.len() || distance > WINDOW {
    return Err(DecompressError::InvalidInput);
  }
  let mut n = 0usize;
  while n < length {
    let idx = out.len().saturating_sub(distance);
    let byte = out.get(idx).copied().ok_or(DecompressError::InvalidInput)?;
    push_byte(out, max_out, byte)?;
    n = n.saturating_add(1);
  }
  Ok(())
}
