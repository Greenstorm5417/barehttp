//! Blocking HTTP/1.1 client for `no_std` + `alloc`. Cleartext HTTP on the wire.
//! For `https://`, supply a TLS-terminating [`BlockingSocket`] and set
//! [`config::Config::assume_tls_socket`]. Pairing that flag with [`OsBlockingSocket`]
//! returns [`Error::TlsNotConfigured`].
//!
//! ```no_run
//! let response = barehttp::get("http://httpbin.org/get").call()?;
//! println!("{}", response.text()?);
//! # Ok::<(), barehttp::Error>(())
//! ```
//!
//! Configure via [`config::Config`]. [`HttpClient`] follows redirects.
//! See [`Response::text`] and [`Response::is_success`].

#![no_std]
#![forbid(unsafe_op_in_unsafe_fn)]
#![deny(
  clippy::unwrap_used,
  clippy::expect_used,
  clippy::panic,
  clippy::panic_in_result_fn,
  clippy::indexing_slicing,
  clippy::integer_division,
  clippy::cast_lossless,
  clippy::cast_possible_truncation,
  clippy::cast_possible_wrap,
  clippy::cast_precision_loss,
  clippy::shadow_unrelated,
  clippy::shadow_reuse,
  clippy::shadow_same,
  clippy::wildcard_imports,
  dead_code
)]
#![warn(
  missing_docs,
  clippy::pedantic,
  clippy::nursery,
  clippy::missing_errors_doc,
  clippy::missing_panics_doc
)]
#![allow(
  clippy::inline_always,
  clippy::similar_names,
  clippy::too_many_lines,
  clippy::too_many_arguments,
  clippy::type_complexity,
  clippy::ptr_as_ptr
)]

extern crate alloc;

#[cfg(feature = "cookie-jar")]
/// RFC 6265 cookie store.
pub mod cookie_jar;

pub use client::HttpClient;
pub use error::Error;

pub use dns::{DnsResolver, OsDnsResolver};
pub use error::{DnsError, ParseError, SocketError};
pub use socket::BlockingSocket;
pub use socket::{OsBlockingSocket, SocketAddr};
pub use util::IpAddr;

pub use headers::Headers;
pub use method::Method;
pub use parser::Response;
pub use parser::uri::Uri;
pub use parser::version::Version;
pub use request_builder::ClientRequestBuilder;

/// Alias for [`HttpClient`] with OS adapters (ureq naming).
pub type Agent = HttpClient<OsBlockingSocket, OsDnsResolver>;

/// Alias for [`ClientRequestBuilder`] with OS adapters (ureq naming).
pub type RequestBuilder = ClientRequestBuilder<OsBlockingSocket, OsDnsResolver>;

/// [`Agent`] / [`HttpClient`] with default OS adapters.
#[must_use]
pub fn agent() -> Agent {
  HttpClient::new()
}

/// GET using a fresh default OS client.
#[must_use]
pub fn get(url: &str) -> RequestBuilder {
  HttpClient::new().get(url)
}

/// POST using a fresh default OS client (body via [`.send()`](ClientRequestBuilder::send)).
#[must_use]
pub fn post(url: &str) -> RequestBuilder {
  HttpClient::new().post(url)
}

/// PUT using a fresh default OS client (body via [`.send()`](ClientRequestBuilder::send)).
#[must_use]
pub fn put(url: &str) -> RequestBuilder {
  HttpClient::new().put(url)
}

/// DELETE using a fresh default OS client.
#[must_use]
pub fn delete(url: &str) -> RequestBuilder {
  HttpClient::new().delete(url)
}

/// HEAD using a fresh default OS client.
#[must_use]
pub fn head(url: &str) -> RequestBuilder {
  HttpClient::new().head(url)
}

/// PATCH using a fresh default OS client (body via [`.send()`](ClientRequestBuilder::send)).
#[must_use]
pub fn patch(url: &str) -> RequestBuilder {
  HttpClient::new().patch(url)
}

/// Client configuration.
pub mod config;
/// Request builder.
pub mod request_builder;

mod client;
mod dns;
mod error;
/// Gzip / zlib / raw DEFLATE decompression (RFC 1950–1952). Feature-gated.
#[cfg(feature = "gzip-decompression")]
pub mod gzip;
mod headers;
mod method;
pub(crate) mod parser;
pub(crate) mod socket;
pub(crate) mod sync;
mod transport;
pub(crate) mod util;
