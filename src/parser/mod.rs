//! HTTP/1.1 parse + wire serialize (`pub(crate)`).
//!
//! Crate-root re-exports: [`Response`], [`status::StatusCode`], [`version::Version`].
//! Internal: [`WireRequest`], [`BodyReadStrategy`], [`uri::Uri`], [`framing`].

mod chunked;
mod headers;
mod status_line;
mod response;
mod wire_request;
pub mod framing;
pub mod status;
pub mod uri;
pub mod version;
#[cfg(feature = "cookie-jar")]
pub mod cookie;

#[cfg(test)]
pub mod tests;

pub use response::{BodyReadStrategy, Response};
pub use wire_request::WireRequest;
