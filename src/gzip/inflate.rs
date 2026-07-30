//! Raw DEFLATE inflate (RFC 1951).

#![allow(clippy::cast_possible_truncation, clippy::cast_lossless)] // DEFLATE bit widths / symbol ranges are RFC-bounded

use super::DecompressError;
use super::bit::BitReader;
use super::crc32::update_crc;
use super::fixed_tables::{FIXED_DIST_MAX_BITS, FIXED_DIST_TABLE, FIXED_LIT_MAX_BITS, FIXED_LIT_TABLE};
use super::huffman::{HuffmanDecoder, HuffmanPool};
use alloc::vec::Vec;

/// Sliding window size (RFC 1951 §2).
const WINDOW: usize = 32_768;

/// Stack buffer for consecutive Huffman literals before one `extend`.
const LIT_BATCH: usize = 64;

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

/// Running output checksum updated as inflate emits bytes (no full post-pass).
pub(super) enum RunningChecksum {
  /// Raw DEFLATE — no trailer check.
  None,
  /// Gzip member CRC-32 (init `0`; `update_crc` pre/post-conditions).
  Crc(u32),
  /// Zlib Adler-32 (RFC 1950). `pending` = bytes since last `% 65521`.
  Adler { a: u32, b: u32, pending: usize },
}

impl RunningChecksum {
  pub(super) const fn crc() -> Self {
    Self::Crc(0)
  }

  pub(super) const fn adler() -> Self {
    Self::Adler { a: 1, b: 0, pending: 0 }
  }

  #[inline]
  fn update(
    &mut self,
    data: &[u8],
  ) {
    match self {
      Self::None => {},
      Self::Crc(c) => *c = update_crc(*c, data),
      Self::Adler { a, b, pending } => update_adler(a, b, pending, data),
    }
  }

  /// Finished CRC-32 value (gzip trailer).
  pub(super) const fn crc_value(self) -> u32 {
    match self {
      Self::Crc(c) => c,
      _ => 0,
    }
  }

  /// Finished Adler-32 value (zlib trailer).
  pub(super) const fn adler_value(self) -> u32 {
    match self {
      Self::Adler { a, b, .. } => {
        const MOD: u32 = 65_521;
        ((b % MOD) << 16) | (a % MOD)
      },
      _ => 1,
    }
  }
}

/// Adler-32 inner update (RFC 1950 / zlib `NMAX` deferral of `% 65521`).
#[allow(clippy::integer_division)] // Adler-32 uses mod 65521
#[inline]
fn update_adler(
  a: &mut u32,
  b: &mut u32,
  pending: &mut usize,
  data: &[u8],
) {
  const MOD: u32 = 65_521;
  const NMAX: usize = 5552;
  for &byte in data {
    *a += u32::from(byte);
    *b += *a;
    *pending += 1;
    if *pending >= NMAX {
      *a %= MOD;
      *b %= MOD;
      *pending = 0;
    }
  }
}

/// Inflate a raw DEFLATE stream. Returns `(output, bytes_consumed)`.
///
/// Allocating entry used by the public `decompress_*` APIs (Callgrind-sensitive).
pub(super) fn inflate_owned(
  data: &[u8],
  max_out: usize,
  checksum: &mut RunningChecksum,
) -> Result<(Vec<u8>, usize), DecompressError> {
  let mut bits = BitReader::new(data);
  // Heuristic capacity: prefer avoiding realloc churn on typical HTTP bodies.
  let guess = max_out
    .min(data.len().saturating_mul(4).max(64))
    .min(WINDOW.saturating_mul(2));
  let mut out = Vec::with_capacity(guess);
  // Reuse capped Huffman table buffers across dynamic blocks (peak ≈ 2× 2 KiB).
  let mut huff_pool = HuffmanPool::new();
  loop {
    let bfinal = bits.get_bits(1)?;
    let btype = bits.get_bits(2)?;
    match btype {
      0 => inflate_stored(&mut bits, &mut out, max_out, checksum)?,
      1 => inflate_fixed(&mut bits, &mut out, max_out, checksum)?,
      2 => {
        let (lit, dist) = read_dynamic_trees(&mut bits, &mut huff_pool)?;
        inflate_compressed(&mut bits, &mut out, max_out, checksum, &lit, &dist)?;
        huff_pool.recycle_pair(lit, dist);
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

/// Inflate a raw DEFLATE stream into `out` (cleared first when reusing capacity).
///
/// Returns bytes of `data` consumed. On error, `out` may hold a partial payload —
/// callers that reuse the buffer should `clear` before the next attempt if they
/// do not already call this again (which clears / reallocates).
pub(super) fn inflate(
  data: &[u8],
  max_out: usize,
  checksum: &mut RunningChecksum,
  out: &mut Vec<u8>,
) -> Result<usize, DecompressError> {
  let mut bits = BitReader::new(data);
  // Heuristic capacity: prefer avoiding realloc churn on typical HTTP bodies.
  let guess = max_out
    .min(data.len().saturating_mul(4).max(64))
    .min(WINDOW.saturating_mul(2));
  // Fresh buffer: `with_capacity` matches [`inflate_owned`].
  // Reused buffer: clear + reserve keeps pooled capacity.
  if out.capacity() == 0 {
    *out = Vec::with_capacity(guess);
  } else {
    out.clear();
    out.reserve(guess);
  }
  // Reuse capped Huffman table buffers across dynamic blocks (peak ≈ 2× 2 KiB).
  let mut huff_pool = HuffmanPool::new();
  loop {
    let bfinal = bits.get_bits(1)?;
    let btype = bits.get_bits(2)?;
    match btype {
      0 => inflate_stored(&mut bits, out, max_out, checksum)?,
      1 => inflate_fixed(&mut bits, out, max_out, checksum)?,
      2 => {
        let (lit, dist) = read_dynamic_trees(&mut bits, &mut huff_pool)?;
        inflate_compressed(&mut bits, out, max_out, checksum, &lit, &dist)?;
        huff_pool.recycle_pair(lit, dist);
      },
      _ => return Err(DecompressError::InvalidInput),
    }
    if bfinal != 0 {
      break;
    }
  }
  bits.align_to_byte();
  Ok(bits.byte_pos())
}

fn inflate_stored(
  bits: &mut BitReader<'_>,
  out: &mut Vec<u8>,
  max_out: usize,
  checksum: &mut RunningChecksum,
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
  let start = out.len();
  bits.copy_aligned_bytes(out, usize::from(len), max_out)?;
  checksum.update(out.get(start..).unwrap_or(&[]));
  Ok(())
}

/// Fixed Huffman block: static tables, no enum dispatch on every symbol.
fn inflate_fixed(
  bits: &mut BitReader<'_>,
  out: &mut Vec<u8>,
  max_out: usize,
  checksum: &mut RunningChecksum,
) -> Result<(), DecompressError> {
  let mut lit_buf = [0u8; LIT_BATCH];
  let mut lit_n = 0usize;
  loop {
    let sym = HuffmanDecoder::decode_static(&FIXED_LIT_TABLE, FIXED_LIT_MAX_BITS, bits)?;
    if sym < 256 {
      #[allow(clippy::cast_possible_truncation)] // sym < 256
      let byte = sym as u8;
      if lit_n == LIT_BATCH {
        flush_literals(out, max_out, checksum, &lit_buf, &mut lit_n)?;
      }
      if let Some(slot) = lit_buf.get_mut(lit_n) {
        *slot = byte;
        lit_n += 1;
      }
      continue;
    }
    flush_literals(out, max_out, checksum, &lit_buf, &mut lit_n)?;
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
    copy_match(out, max_out, checksum, usize::from(distance), usize::from(len))?;
  }
}

fn read_dynamic_trees(
  bits: &mut BitReader<'_>,
  pool: &mut HuffmanPool,
) -> Result<(HuffmanDecoder, HuffmanDecoder), DecompressError> {
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
  let cl_dec = pool.take_decoder0(&cl_lengths)?;

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
  // Reuse cl table capacity for lit; dist takes the second pool slot.
  pool.recycle0(cl_dec);
  let lit = pool.take_decoder0(lit_lens)?;
  let dist = pool.take_decoder1(dist_lens)?;
  Ok((lit, dist))
}

fn inflate_compressed(
  bits: &mut BitReader<'_>,
  out: &mut Vec<u8>,
  max_out: usize,
  checksum: &mut RunningChecksum,
  lit: &HuffmanDecoder,
  dist: &HuffmanDecoder,
) -> Result<(), DecompressError> {
  let mut lit_buf = [0u8; LIT_BATCH];
  let mut lit_n = 0usize;
  loop {
    let sym = lit.decode(bits)?;
    if sym < 256 {
      #[allow(clippy::cast_possible_truncation)] // sym < 256
      let byte = sym as u8;
      if lit_n == LIT_BATCH {
        flush_literals(out, max_out, checksum, &lit_buf, &mut lit_n)?;
      }
      if let Some(slot) = lit_buf.get_mut(lit_n) {
        *slot = byte;
        lit_n += 1;
      }
      continue;
    }
    flush_literals(out, max_out, checksum, &lit_buf, &mut lit_n)?;
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
    copy_match(out, max_out, checksum, usize::from(distance), usize::from(len))?;
  }
}

#[inline]
fn flush_literals(
  out: &mut Vec<u8>,
  max_out: usize,
  checksum: &mut RunningChecksum,
  lit_buf: &[u8; LIT_BATCH],
  lit_n: &mut usize,
) -> Result<(), DecompressError> {
  let n = *lit_n;
  if n == 0 {
    return Ok(());
  }
  let bytes = lit_buf.get(..n).unwrap_or(&[]);
  emit_bytes(out, max_out, checksum, bytes)?;
  *lit_n = 0;
  Ok(())
}

#[inline]
fn emit_bytes(
  out: &mut Vec<u8>,
  max_out: usize,
  checksum: &mut RunningChecksum,
  bytes: &[u8],
) -> Result<(), DecompressError> {
  if out.len().saturating_add(bytes.len()) > max_out {
    return Err(DecompressError::LimitExceeded);
  }
  out.extend_from_slice(bytes);
  checksum.update(bytes);
  Ok(())
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
  checksum: &mut RunningChecksum,
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
  let start = out.len();

  // Hot case: RLE-style match (distance == 1) — common in repetitive HTTP bodies.
  if distance == 1 {
    let b = *out.last().ok_or(DecompressError::InvalidInput)?;
    let new_len = out.len() + length;
    out.resize(new_len, b);
  } else if length <= distance {
    // Non-overlapping: one extend covers the whole match.
    let src = out.len() - distance;
    out.extend_from_within(src..src + length);
  } else {
    // Overlapping general case: chunk by `distance` so RLE-style expansion stays correct.
    let mut left = length;
    while left > 0 {
      let src = out.len() - distance;
      let chunk = left.min(distance);
      out.extend_from_within(src..src + chunk);
      left -= chunk;
    }
  }
  checksum.update(out.get(start..).unwrap_or(&[]));
  Ok(())
}
