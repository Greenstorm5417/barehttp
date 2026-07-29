//! HTTP/1.1 parsing and wire serialization (`pub(crate)`).
//!
//! Crate-root re-exports: [`Response`], [`version::Version`].
//! Internal: [`serialize_request`], [`BodyReadStrategy`], [`uri::Uri`], [`has_complete_headers`].

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

/// Buffer already contains a complete header section (`\r\n\r\n` or LF-only `\n\n`).
#[inline]
pub fn has_complete_headers(data: &[u8]) -> bool {
  data.windows(4).any(|w| w == b"\r\n\r\n") || data.windows(2).any(|w| w == b"\n\n")
}

pub use response::Response;
pub use response::BodyReadStrategy;
pub use wire_request::serialize_request;
