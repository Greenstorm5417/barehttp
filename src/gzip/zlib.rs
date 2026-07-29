//! Zlib wrapper (RFC 1950) for HTTP `Content-Encoding: deflate`.

use super::DecompressError;
use super::inflate;
use alloc::vec::Vec;

const CM_DEFLATE: u8 = 8;
const FDICT: u8 = 0x20;

/// Adler-32 (RFC 1950) of `data`.
#[allow(clippy::integer_division)] // Adler-32 uses mod 65521
fn adler32(data: &[u8]) -> u32 {
  const MOD: u32 = 65_521;
  let mut a = 1u32;
  let mut b = 0u32;
  for &byte in data {
    a = a.saturating_add(u32::from(byte)) % MOD;
    b = b.saturating_add(a) % MOD;
  }
  (b << 16) | a
}

/// Decompress a zlib-wrapped DEFLATE stream.
pub(super) fn decompress(
  data: &[u8],
  max_out: usize,
) -> Result<Vec<u8>, DecompressError> {
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

  let deflate = data.get(2..).ok_or(DecompressError::InvalidInput)?;
  let (out, consumed) = inflate::inflate(deflate, max_out)?;
  let trailer_off = consumed.checked_add(2).ok_or(DecompressError::InvalidInput)?;
  let b0 = *data.get(trailer_off).ok_or(DecompressError::InvalidInput)?;
  let b1 = *data.get(trailer_off.saturating_add(1)).ok_or(DecompressError::InvalidInput)?;
  let b2 = *data.get(trailer_off.saturating_add(2)).ok_or(DecompressError::InvalidInput)?;
  let b3 = *data.get(trailer_off.saturating_add(3)).ok_or(DecompressError::InvalidInput)?;
  let got = u32::from_be_bytes([b0, b1, b2, b3]);
  if got != adler32(&out) {
    return Err(DecompressError::InvalidInput);
  }
  Ok(out)
}
