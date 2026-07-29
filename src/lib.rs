#![doc = include_str!("../README.md")]
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
  clippy::ptr_as_ptr,
  // Agent 1 privacy: private modules keep `pub(crate)` items for intent clarity.
  clippy::redundant_pub_crate,
  // Builder type is `#[must_use]`; free/method helpers also annotate.
  clippy::double_must_use
)]

extern crate alloc;

#[cfg(feature = "cookie-jar")]
/// RFC 6265 cookie store ([`CookieStore`](cookie_jar::CookieStore) / [`CookieJar`](cookie_jar::CookieJar)).
pub mod cookie_jar;

pub use client::HttpClient;
pub use error::Error;

pub use dns::{DnsResolver, OsDnsResolver};
pub use error::{DnsError, ParseError, SocketError};
pub use socket::{BlockingSocket, BlockingSocketFactory};
pub use socket::{OsBlockingSocket, SocketAddr};
pub use util::IpAddr;

pub use headers::{Headers, Iter as HeaderIter};
pub use method::Method;
pub use parser::Response;
pub use parser::uri::{Authority, Host, Uri};
pub use parser::version::Version;
pub use request_builder::ClientRequestBuilder;

/// [`HttpClient`] with OS adapters (ureq calls this `Agent`).
pub type Agent = HttpClient<OsBlockingSocket, OsDnsResolver>;

/// [`ClientRequestBuilder`] with OS adapters (ureq calls this `RequestBuilder`).
pub type RequestBuilder = ClientRequestBuilder<OsBlockingSocket, OsDnsResolver>;

/// Default-OS [`Agent`] / [`HttpClient`].
#[must_use]
pub fn agent() -> Agent {
  HttpClient::new()
}

/// GET using a fresh default OS client.
///
/// # Examples
///
/// ```no_run
/// let response = barehttp::get("http://example.com").call()?;
/// println!("{}", response.to_text()?);
/// # Ok::<(), barehttp::Error>(())
/// ```
pub fn get(url: &str) -> RequestBuilder {
  HttpClient::new().get(url)
}

/// POST using a fresh default OS client (body via [`.send()`](ClientRequestBuilder::send)).
///
/// # Examples
///
/// ```no_run
/// let response = barehttp::post("http://example.com/api").send(b"{\"a\":1}")?;
/// assert!(response.is_success() || response.status() >= 400);
/// # Ok::<(), barehttp::Error>(())
/// ```
pub fn post(url: &str) -> RequestBuilder {
  HttpClient::new().post(url)
}

/// PUT using a fresh default OS client (body via [`.send()`](ClientRequestBuilder::send)).
pub fn put(url: &str) -> RequestBuilder {
  HttpClient::new().put(url)
}

/// DELETE using a fresh default OS client.
pub fn delete(url: &str) -> RequestBuilder {
  HttpClient::new().delete(url)
}

/// HEAD using a fresh default OS client.
pub fn head(url: &str) -> RequestBuilder {
  HttpClient::new().head(url)
}

/// PATCH using a fresh default OS client (body via [`.send()`](ClientRequestBuilder::send)).
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
/// Gzip / zlib / raw DEFLATE decompression (RFC 1950–1952). Feature-gated (`gzip`).
#[cfg(feature = "gzip")]
pub mod gzip;
mod headers;
mod method;
pub(crate) mod parser;
pub(crate) mod socket;
pub(crate) mod sync;
mod transport;
pub(crate) mod util;
