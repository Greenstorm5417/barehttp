//! HTTP/1.1 parsing and wire serialization (`pub(crate)`).
//!
//! Crate-root re-exports: [`Response`], [`version::Version`].
//! Internal: [`serialize_request`], [`SerializedRequest`], [`BodyReadStrategy`], [`uri::Uri`], [`has_complete_headers`].

pub mod chunked;
#[cfg(feature = "cookie-jar")]
pub mod cookie;
mod headers;
mod response;
pub mod uri;
pub mod version;
mod wire_request;

#[cfg(test)]
pub mod tests;

#[cfg(kani)]
mod kani_proofs;

/// Buffer already contains a complete header section (`\r\n\r\n` or LF-only `\n\n`).
#[inline]
pub fn has_complete_headers(data: &[u8]) -> bool {
  header_section_end(data).is_some()
}

/// Length of the header section including the terminating blank line, if complete.
#[inline]
pub fn header_section_end(data: &[u8]) -> Option<usize> {
  let mut off = 0usize;
  while let Some(rel) = find_byte(data.get(off..).unwrap_or(&[]), b'\n') {
    let i = off.saturating_add(rel);
    if data.get(i.saturating_add(1)).copied() == Some(b'\n') {
      return Some(i.saturating_add(2));
    }
    if i > 0
      && data.get(i.saturating_sub(1)).copied() == Some(b'\r')
      && data.get(i.saturating_add(1)).copied() == Some(b'\r')
      && data.get(i.saturating_add(2)).copied() == Some(b'\n')
    {
      return Some(i.saturating_add(3));
    }
    off = i.saturating_add(1);
  }
  None
}

/// SWAR search for `needle` (used by header scanning and terminator search).
#[inline]
pub(crate) fn find_byte(
  haystack: &[u8],
  needle: u8,
) -> Option<usize> {
  const ONES: u64 = 0x0101_0101_0101_0101;
  const HIGHS: u64 = 0x8080_8080_8080_8080;
  let splat = u64::from(needle).wrapping_mul(ONES);
  let mut off = 0usize;
  while let Some(chunk) = haystack.get(off..off.saturating_add(8))
    && let Ok(bytes) = <[u8; 8]>::try_from(chunk)
  {
    let word = u64::from_le_bytes(bytes);
    let xor = word ^ splat;
    let mask = xor.wrapping_sub(ONES) & !xor & HIGHS;
    if mask != 0 {
      let bit = mask.trailing_zeros();
      let idx = u8::try_from(bit >> 3).unwrap_or(0);
      return Some(off.saturating_add(usize::from(idx)));
    }
    off = off.saturating_add(8);
  }
  haystack.get(off..).and_then(|tail| {
    tail
      .iter()
      .position(|&b| b == needle)
      .map(|i| off.saturating_add(i))
  })
}

/// First CR or LF in `haystack`.
#[inline]
pub(crate) fn find_cr_or_lf(haystack: &[u8]) -> Option<usize> {
  const ONES: u64 = 0x0101_0101_0101_0101;
  const HIGHS: u64 = 0x8080_8080_8080_8080;
  let splat_n = u64::from(b'\n').wrapping_mul(ONES);
  let splat_r = u64::from(b'\r').wrapping_mul(ONES);
  let mut off = 0usize;
  while let Some(chunk) = haystack.get(off..off.saturating_add(8))
    && let Ok(bytes) = <[u8; 8]>::try_from(chunk)
  {
    let word = u64::from_le_bytes(bytes);
    let xor_n = word ^ splat_n;
    let xor_r = word ^ splat_r;
    let mask = (xor_n.wrapping_sub(ONES) & !xor_n & HIGHS) | (xor_r.wrapping_sub(ONES) & !xor_r & HIGHS);
    if mask != 0 {
      let bit = mask.trailing_zeros();
      let idx = u8::try_from(bit >> 3).unwrap_or(0);
      return Some(off.saturating_add(usize::from(idx)));
    }
    off = off.saturating_add(8);
  }
  haystack.get(off..).and_then(|tail| {
    tail
      .iter()
      .position(|&b| b == b'\n' || b == b'\r')
      .map(|i| off.saturating_add(i))
  })
}

pub use response::BodyReadStrategy;
pub use response::Response;
pub use wire_request::{SerializedRequest, serialize_request};
