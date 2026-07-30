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
  // Single forward scan (avoids `windows().any` iterator overhead on every recv).
  let mut i = 0usize;
  while i < data.len() {
    match data.get(i).copied() {
      Some(b'\r')
        if data.get(i.saturating_add(1)).copied() == Some(b'\n')
          && data.get(i.saturating_add(2)).copied() == Some(b'\r')
          && data.get(i.saturating_add(3)).copied() == Some(b'\n') =>
      {
        return true;
      },
      Some(b'\n') if data.get(i.saturating_add(1)).copied() == Some(b'\n') => {
        return true;
      },
      _ => {},
    }
    i = i.saturating_add(1);
  }
  false
}

pub use response::BodyReadStrategy;
pub use response::Response;
pub use wire_request::{SerializedRequest, serialize_request};
