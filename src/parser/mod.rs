//! HTTP/1.1 parse and wire serialize (`pub(crate)`).
//!
//! Re-exported at the crate root: [`Response`], [`version::Version`].
//! Crate-internal: [`serialize_request`], [`BodyReadStrategy`], [`uri::Uri`], [`framing`].

pub mod chunked;
#[cfg(feature = "cookie-jar")]
pub mod cookie;
pub mod framing;
mod headers;
mod response;
pub mod uri;
pub mod version;
mod wire_request;

#[cfg(test)]
pub mod tests;

pub use response::{BodyReadStrategy, Response};
pub use wire_request::serialize_request;
