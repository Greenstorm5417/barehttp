//! Gzip, zlib, and raw DEFLATE decompression (RFC 1950–1952).

mod bit;
mod crc32;
mod fixed_tables;
#[allow(clippy::module_inception)] // RFC 1952 member parser lives in `gzip.rs` by design
mod gzip;
mod huffman;
mod inflate;
mod zlib;

#[cfg(test)]
mod tests;

use alloc::vec::Vec;

/// Same type as [`crate::DecompressError`] (always at the crate root; this module is
/// feature-gated behind `gzip`).
pub use crate::error::DecompressError;

/// RFC 1952 gzip member → uncompressed bytes. Enforces `max_out`.
///
/// # Errors
/// [`DecompressError`] when the member is invalid or output would exceed `max_out`.
///
/// # Examples
///
/// ```
/// use barehttp::gzip::decompress_gzip;
///
/// // gzip member for payload "hi"
/// let gz: &[u8] = &[
///   0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0xff, 0xcb, 0xc8, 0x04, 0x00, 0xac, 0x2a, 0x93,
///   0xd8, 0x02, 0x00, 0x00, 0x00,
/// ];
/// assert_eq!(decompress_gzip(gz, 64)?, b"hi");
/// # Ok::<(), barehttp::gzip::DecompressError>(())
/// ```
pub fn decompress_gzip(
  data: &[u8],
  max_out: usize,
) -> Result<Vec<u8>, DecompressError> {
  gzip::decompress_member_owned(data, max_out)
}

/// Like [`decompress_gzip`], writing into `out` (cleared first; capacity reused).
///
/// Crate-internal: response parse / pooled buffers can adopt without a public API change.
pub(crate) fn decompress_gzip_into(
  data: &[u8],
  max_out: usize,
  out: &mut Vec<u8>,
) -> Result<(), DecompressError> {
  gzip::decompress_member(data, max_out, out)
}

/// HTTP `Content-Encoding: deflate`: try zlib (RFC 1950), then raw RFC 1951.
///
/// # Errors
/// [`DecompressError`] when both wrappers fail or output would exceed `max_out`.
///
/// # Examples
///
/// ```
/// use barehttp::gzip::decompress_http_deflate;
///
/// // zlib-wrapped DEFLATE for payload "hi" (CMF/FLG + deflate + Adler-32)
/// let z: &[u8] = &[0x78, 0xda, 0xcb, 0xc8, 0x04, 0x00, 0x01, 0x3b, 0x00, 0xd2];
/// assert_eq!(decompress_http_deflate(z, 64)?, b"hi");
/// # Ok::<(), barehttp::gzip::DecompressError>(())
/// ```
pub fn decompress_http_deflate(
  data: &[u8],
  max_out: usize,
) -> Result<Vec<u8>, DecompressError> {
  match zlib::decompress_owned(data, max_out) {
    Ok(out) => Ok(out),
    Err(DecompressError::LimitExceeded) => Err(DecompressError::LimitExceeded),
    Err(DecompressError::InvalidInput) => decompress_raw_deflate(data, max_out),
  }
}

/// Like [`decompress_http_deflate`], writing into `out` (cleared; capacity reused).
pub(crate) fn decompress_http_deflate_into(
  data: &[u8],
  max_out: usize,
  out: &mut Vec<u8>,
) -> Result<(), DecompressError> {
  match zlib::decompress(data, max_out, out) {
    Ok(()) => Ok(()),
    Err(DecompressError::LimitExceeded) => Err(DecompressError::LimitExceeded),
    // zlib may have filled `out` before rejecting the trailer / stream; clear for raw retry.
    Err(DecompressError::InvalidInput) => {
      out.clear();
      decompress_raw_deflate_into(data, max_out, out)
    },
  }
}

/// Raw DEFLATE bitstream (RFC 1951) only.
///
/// # Errors
/// [`DecompressError`] when the stream is invalid or output would exceed `max_out`.
///
/// # Examples
///
/// ```
/// use barehttp::gzip::decompress_raw_deflate;
///
/// // raw DEFLATE for payload "hi" (no zlib wrapper)
/// let raw: &[u8] = &[0xcb, 0xc8, 0x04, 0x00];
/// assert_eq!(decompress_raw_deflate(raw, 64)?, b"hi");
/// # Ok::<(), barehttp::gzip::DecompressError>(())
/// ```
pub fn decompress_raw_deflate(
  data: &[u8],
  max_out: usize,
) -> Result<Vec<u8>, DecompressError> {
  let mut none = inflate::RunningChecksum::None;
  let (out, _) = inflate::inflate_owned(data, max_out, &mut none)?;
  Ok(out)
}

/// Like [`decompress_raw_deflate`], writing into `out` (cleared; capacity reused).
pub(crate) fn decompress_raw_deflate_into(
  data: &[u8],
  max_out: usize,
  out: &mut Vec<u8>,
) -> Result<(), DecompressError> {
  let mut none = inflate::RunningChecksum::None;
  let _ = inflate::inflate(data, max_out, &mut none, out)?;
  Ok(())
}
