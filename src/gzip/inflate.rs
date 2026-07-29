//! Raw DEFLATE inflate (RFC 1951).

#![allow(clippy::cast_possible_truncation, clippy::cast_lossless)] // DEFLATE bit widths / symbol ranges are RFC-bounded

use super::DecompressError;
use super::bit::BitReader;
use super::fixed_tables::{FIXED_DIST_MAX_BITS, FIXED_DIST_TABLE, FIXED_LIT_MAX_BITS, FIXED_LIT_TABLE};
use super::huffman::HuffmanDecoder;
use alloc::vec::Vec;

/// Sliding window size (RFC 1951 §2).
const WINDOW: usize = 32_768;

/// Length base for codes 257..=285 (RFC 1951 §3.2.5).
const LENGTH_BASE: [u16; 29] = [
  3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258,
];
/// Extra bits for length codes 257..=285.
const LENGTH_EXTRA: [u8; 29] = [
  0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];

/// Distance base for codes 0..=29 (RFC 1951 §3.2.5).
const DIST_BASE: [u16; 30] = [
  1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145,
  8193, 12289, 16385, 24577,
];
/// Extra bits for distance codes 0..=29.
const DIST_EXTRA: [u8; 30] = [
  0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13,
];

/// Code-length alphabet order (RFC 1951 §3.2.7).
const CL_ORDER: [u8; 19] = [
  16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

/// Inflate a raw DEFLATE stream. Returns `(output, bytes_consumed)`.
pub(super) fn inflate(
  data: &[u8],
  max_out: usize,
) -> Result<(Vec<u8>, usize), DecompressError> {
  let mut bits = BitReader::new(data);
  // Heuristic capacity: prefer avoiding realloc churn on typical HTTP bodies.
  let guess = max_out
    .min(data.len().saturating_mul(4).max(64))
    .min(WINDOW.saturating_mul(2));
  let mut out = Vec::with_capacity(guess);
  loop {
    let bfinal = bits.get_bits(1)?;
    let btype = bits.get_bits(2)?;
    match btype {
      0 => inflate_stored(&mut bits, &mut out, max_out)?,
      1 => inflate_fixed(&mut bits, &mut out, max_out)?,
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
  bits.copy_aligned_bytes(out, usize::from(len), max_out)
}

/// Fixed Huffman block: static tables, no enum dispatch on every symbol.
fn inflate_fixed(
  bits: &mut BitReader<'_>,
  out: &mut Vec<u8>,
  max_out: usize,
) -> Result<(), DecompressError> {
  loop {
    let sym = HuffmanDecoder::decode_static(&FIXED_LIT_TABLE, FIXED_LIT_MAX_BITS, bits)?;
    if sym < 256 {
      #[allow(clippy::cast_possible_truncation)] // sym < 256
      let byte = sym as u8;
      if out.len() >= max_out {
        return Err(DecompressError::LimitExceeded);
      }
      out.push(byte);
      continue;
    }
    if sym == 256 {
      return Ok(());
    }
    if sym > 285 {
      return Err(DecompressError::InvalidInput);
    }
    let (base_len, extra_len) = length_base_extra(sym)?;
    let len = base_len + bits.get_bits(extra_len)? as u16;
    let dsym = HuffmanDecoder::decode_static(&FIXED_DIST_TABLE, FIXED_DIST_MAX_BITS, bits)?;
    if dsym > 29 {
      return Err(DecompressError::InvalidInput);
    }
    let (base_dist, extra_dist) = dist_base_extra(dsym)?;
    let distance = base_dist + bits.get_bits(extra_dist)? as u16;
    copy_match(out, max_out, usize::from(distance), usize::from(len))?;
  }
}

fn read_dynamic_trees(bits: &mut BitReader<'_>) -> Result<(HuffmanDecoder, HuffmanDecoder), DecompressError> {
  // RFC 1951 §3.2.7
  let hlit = bits.get_bits(5)? + 257;
  let hdist = bits.get_bits(5)? + 1;
  let hclen = bits.get_bits(4)? + 4;

  let mut cl_lengths = [0u8; 19];
  let mut i = 0u32;
  while i < hclen {
    let len = bits.get_bits(3)? as u8;
    let ord = *CL_ORDER
      .get(i as usize)
      .ok_or(DecompressError::InvalidInput)?;
    if let Some(slot) = cl_lengths.get_mut(usize::from(ord)) {
      *slot = len;
    }
    i += 1;
  }
  let cl_dec = HuffmanDecoder::from_lengths(&cl_lengths)?;

  // Max lit+dist lengths: 286 + 32 = 318. Stay on the stack — no heap for tree build.
  let total = (hlit + hdist) as usize;
  if total > 318 {
    return Err(DecompressError::InvalidInput);
  }
  let mut all_lens = [0u8; 318];
  let mut n = 0usize;
  while n < total {
    let sym = cl_dec.decode(bits)?;
    match sym {
      0..=15 => {
        if let Some(slot) = all_lens.get_mut(n) {
          *slot = sym as u8;
        }
        n += 1;
      },
      16 => {
        let rep = (bits.get_bits(2)? + 3) as usize;
        if n == 0 {
          return Err(DecompressError::InvalidInput);
        }
        let prev = all_lens.get(n - 1).copied().unwrap_or(0);
        if n + rep > total {
          return Err(DecompressError::InvalidInput);
        }
        let end = n + rep;
        while n < end {
          if let Some(slot) = all_lens.get_mut(n) {
            *slot = prev;
          }
          n += 1;
        }
      },
      17 => {
        let rep = (bits.get_bits(3)? + 3) as usize;
        if n + rep > total {
          return Err(DecompressError::InvalidInput);
        }
        n += rep; // already zero-filled
      },
      18 => {
        let rep = (bits.get_bits(7)? + 11) as usize;
        if n + rep > total {
          return Err(DecompressError::InvalidInput);
        }
        n += rep;
      },
      _ => return Err(DecompressError::InvalidInput),
    }
  }

  let lit_n = hlit as usize;
  let lit_lens = all_lens.get(..lit_n).ok_or(DecompressError::InvalidInput)?;
  let dist_lens = all_lens
    .get(lit_n..total)
    .ok_or(DecompressError::InvalidInput)?;
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
      #[allow(clippy::cast_possible_truncation)] // sym < 256
      let byte = sym as u8;
      if out.len() >= max_out {
        return Err(DecompressError::LimitExceeded);
      }
      out.push(byte);
      continue;
    }
    if sym == 256 {
      return Ok(());
    }
    if sym > 285 {
      return Err(DecompressError::InvalidInput);
    }
    let (base_len, extra_len) = length_base_extra(sym)?;
    let len = base_len + bits.get_bits(extra_len)? as u16;
    let dsym = dist.decode(bits)?;
    if dsym > 29 {
      return Err(DecompressError::InvalidInput);
    }
    let (base_dist, extra_dist) = dist_base_extra(dsym)?;
    let distance = base_dist + bits.get_bits(extra_dist)? as u16;
    copy_match(out, max_out, usize::from(distance), usize::from(len))?;
  }
}

#[inline(always)]
#[allow(clippy::indexing_slicing)] // idx validated: code in 257..=285
fn length_base_extra(code: u16) -> Result<(u16, u8), DecompressError> {
  let idx = usize::from(code - 257);
  if idx >= 29 {
    return Err(DecompressError::InvalidInput);
  }
  Ok((LENGTH_BASE[idx], LENGTH_EXTRA[idx]))
}

#[inline(always)]
#[allow(clippy::indexing_slicing)] // idx validated: code ≤ 29
fn dist_base_extra(code: u16) -> Result<(u16, u8), DecompressError> {
  let idx = usize::from(code);
  if idx >= 30 {
    return Err(DecompressError::InvalidInput);
  }
  Ok((DIST_BASE[idx], DIST_EXTRA[idx]))
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
  if out.len().saturating_add(length) > max_out {
    return Err(DecompressError::LimitExceeded);
  }
  out.reserve(length);

  // Hot case: RLE-style match (distance == 1) — common in repetitive HTTP bodies.
  if distance == 1 {
    let b = *out.last().ok_or(DecompressError::InvalidInput)?;
    let new_len = out.len() + length;
    out.resize(new_len, b);
    return Ok(());
  }

  // Non-overlapping: one extend covers the whole match.
  if length <= distance {
    let src = out.len() - distance;
    out.extend_from_within(src..src + length);
    return Ok(());
  }

  // Overlapping general case: chunk by `distance` so RLE-style expansion stays correct.
  let mut left = length;
  while left > 0 {
    let src = out.len() - distance;
    let chunk = left.min(distance);
    out.extend_from_within(src..src + chunk);
    left -= chunk;
  }
  Ok(())
}
