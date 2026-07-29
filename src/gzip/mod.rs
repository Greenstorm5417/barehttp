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
pub enum DecompressError {
  /// Truncated, corrupt, or illegal input.
  InvalidInput,
  /// Uncompressed output would exceed the configured limit.
  LimitExceeded,
}

/// RFC 1952 gzip member → uncompressed bytes. Enforces `max_out`.
///
/// # Errors
/// [`DecompressError`] when the member is invalid or output would exceed `max_out`.
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
    Err(DecompressError::InvalidInput) => inflate_raw(data, max_out),
  }
}

/// Raw DEFLATE bitstream (RFC 1951) only.
///
/// # Errors
/// [`DecompressError`] when the stream is invalid or output would exceed `max_out`.
pub fn inflate_raw(
  data: &[u8],
  max_out: usize,
) -> Result<Vec<u8>, DecompressError> {
  let (out, _) = inflate::inflate(data, max_out)?;
  Ok(out)
}
