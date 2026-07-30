//! Zlib wrapper (RFC 1950) for HTTP `Content-Encoding: deflate`.

use super::DecompressError;
use super::inflate::{self, RunningChecksum};
use alloc::vec::Vec;

const CM_DEFLATE: u8 = 8;
const FDICT: u8 = 0x20;

fn split_deflate(data: &[u8]) -> Result<&[u8], DecompressError> {
  let cmf = *data.first().ok_or(DecompressError::InvalidInput)?;
  let flg = *data.get(1).ok_or(DecompressError::InvalidInput)?;
  if cmf & 0x0f != CM_DEFLATE {
    return Err(DecompressError::InvalidInput);
  }
  let check = (u16::from(cmf) << 8) | u16::from(flg);
  if check % 31 != 0 {
    return Err(DecompressError::InvalidInput);
  }
  if flg & FDICT != 0 {
    return Err(DecompressError::InvalidInput);
  }
  data.get(2..).ok_or(DecompressError::InvalidInput)
}

fn check_adler(
  data: &[u8],
  trailer_off: usize,
  adler: RunningChecksum,
) -> Result<(), DecompressError> {
  let trailer = data
    .get(trailer_off..trailer_off.saturating_add(4))
    .ok_or(DecompressError::InvalidInput)?;
  let b0 = *trailer.first().ok_or(DecompressError::InvalidInput)?;
  let b1 = *trailer.get(1).ok_or(DecompressError::InvalidInput)?;
  let b2 = *trailer.get(2).ok_or(DecompressError::InvalidInput)?;
  let b3 = *trailer.get(3).ok_or(DecompressError::InvalidInput)?;
  let got = u32::from_be_bytes([b0, b1, b2, b3]);
  if got != adler.adler_value() {
    return Err(DecompressError::InvalidInput);
  }
  Ok(())
}

/// Decompress a zlib-wrapped DEFLATE stream (allocating; public API path).
pub(super) fn decompress_owned(
  data: &[u8],
  max_out: usize,
) -> Result<Vec<u8>, DecompressError> {
  let deflate = split_deflate(data)?;
  let mut adler = RunningChecksum::adler();
  let (out, consumed) = inflate::inflate_owned(deflate, max_out, &mut adler)?;
  let trailer_off = consumed
    .checked_add(2)
    .ok_or(DecompressError::InvalidInput)?;
  check_adler(data, trailer_off, adler)?;
  Ok(out)
}

/// Decompress a zlib-wrapped DEFLATE stream into `out` (cleared when non-empty).
pub(super) fn decompress(
  data: &[u8],
  max_out: usize,
  out: &mut Vec<u8>,
) -> Result<(), DecompressError> {
  // Drop stale bytes before header checks; skip when `out` is a fresh empty Vec
  // so the inflate `with_capacity` fast path stays intact.
  if !out.is_empty() {
    out.clear();
  }
  let deflate = split_deflate(data)?;
  let mut adler = RunningChecksum::adler();
  let consumed = inflate::inflate(deflate, max_out, &mut adler, out)?;
  let trailer_off = consumed
    .checked_add(2)
    .ok_or(DecompressError::InvalidInput)?;
  check_adler(data, trailer_off, adler)
}
