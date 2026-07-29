//! Gzip, zlib, and raw DEFLATE decompression (RFC 1950–1952).

mod bit;
mod crc32;
#[allow(clippy::module_inception)] // RFC 1952 member parser lives in `gzip.rs` by design
mod gzip;
mod huffman;
mod inflate;
mod zlib;

#[cfg(test)]
mod tests;

use alloc::vec::Vec;

/// Errors from gzip / deflate decompression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecompressError {
  /// Invalid or truncated input.
  InvalidInput,
  /// Uncompressed output would exceed the configured limit.
  LimitExceeded,
}

impl core::fmt::Display for DecompressError {
  fn fmt(
    &self,
    f: &mut core::fmt::Formatter<'_>,
  ) -> core::fmt::Result {
    match self {
      Self::InvalidInput => f.write_str("invalid gzip/deflate input"),
      Self::LimitExceeded => f.write_str("decompressed output exceeds size limit"),
    }
  }
}

impl core::error::Error for DecompressError {}

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
/// assert_eq!(decompress_gzip(gz, 64).unwrap(), b"hi");
/// ```
pub fn decompress_gzip(
  data: &[u8],
  max_out: usize,
) -> Result<Vec<u8>, DecompressError> {
  gzip::decompress_member(data, max_out)
}

/// HTTP `Content-Encoding: deflate`: try zlib (RFC 1950), then raw RFC 1951.
///
/// # Errors
/// [`DecompressError`] when both wrappers fail or output would exceed `max_out`.
pub fn decompress_http_deflate(
  data: &[u8],
  max_out: usize,
) -> Result<Vec<u8>, DecompressError> {
  match zlib::decompress(data, max_out) {
    Ok(out) => Ok(out),
    Err(DecompressError::LimitExceeded) => Err(DecompressError::LimitExceeded),
    Err(DecompressError::InvalidInput) => decompress_raw_deflate(data, max_out),
  }
}

/// Raw DEFLATE bitstream (RFC 1951) only.
///
/// # Errors
/// [`DecompressError`] when the stream is invalid or output would exceed `max_out`.
pub fn decompress_raw_deflate(
  data: &[u8],
  max_out: usize,
) -> Result<Vec<u8>, DecompressError> {
  let (out, _) = inflate::inflate(data, max_out)?;
  Ok(out)
}
