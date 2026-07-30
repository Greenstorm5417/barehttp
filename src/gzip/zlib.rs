//! Zlib wrapper (RFC 1950) for HTTP `Content-Encoding: deflate`.

use super::DecompressError;
use super::inflate::{self, RunningChecksum};
use alloc::vec::Vec;

const CM_DEFLATE: u8 = 8;
const FDICT: u8 = 0x20;

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
  let mut adler = RunningChecksum::adler();
  let (out, consumed) = inflate::inflate(deflate, max_out, &mut adler)?;
  let trailer_off = consumed
    .checked_add(2)
    .ok_or(DecompressError::InvalidInput)?;
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
  Ok(out)
}
