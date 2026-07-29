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
  missing_debug_implementations,
  clippy::pedantic,
  clippy::nursery,
  clippy::missing_errors_doc,
  clippy::missing_panics_doc,
  clippy::needless_pass_by_value,
  clippy::new_without_default,
  clippy::large_enum_variant,
  clippy::result_large_err,
  clippy::undocumented_unsafe_blocks
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
  clippy::double_must_use,
)]

extern crate alloc;

#[cfg(feature = "cookie-jar")]
/// RFC 6265 cookie store ([`CookieStore`](cookie_jar::CookieStore); alias [`CookieJar`](cookie_jar::CookieJar)).
pub mod cookie_jar;

pub use client::HttpClient;
pub use error::Error;

pub use dns::{DnsResolver, OsDnsResolver};
pub use error::{DecompressError, DnsError, IntoStringError, InvalidRequest, ParseError, SocketError};
pub use socket::{BlockingSocket, BlockingSocketFactory};
pub use socket::{OsBlockingSocket, SocketAddr};
pub use util::IpAddr;

pub use headers::{
  Headers, IntoIter as HeaderIntoIter, Iter as HeaderIter, WellKnownHeader, well_known_header, well_known_header_bytes,
};
pub use method::{ExtensionMethod, Method, ParseMethodError};
pub use parser::Response;
pub use parser::uri::{Authority, Host, Uri};
pub use parser::version::Version;
pub use request_builder::ClientRequestBuilder;

/// [`HttpClient`] with OS adapters (`HttpClient<OsBlockingSocket, OsDnsResolver>`).
///
/// ureq-style synonym. Stable (not deprecated); prefer [`HttpClient`] in new code.
pub type Agent = HttpClient<OsBlockingSocket, OsDnsResolver>;

/// [`ClientRequestBuilder`] with OS adapters.
///
/// ureq-style synonym. Stable (not deprecated); prefer [`ClientRequestBuilder`] in new code.
pub type RequestBuilder = ClientRequestBuilder<OsBlockingSocket, OsDnsResolver>;

/// Default-OS [`HttpClient`] (type alias [`Agent`]).
///
/// # Examples
///
/// ```no_run
/// let client = barehttp::agent();
/// let response = client.get("http://example.com").call()?;
/// assert!(response.status_code() > 0);
/// # Ok::<(), barehttp::Error>(())
/// ```
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
#[must_use]
pub fn get(url: impl AsRef<str>) -> RequestBuilder {
  HttpClient::new().get(url)
}

/// POST using a fresh default OS client (body via [`.send()`](ClientRequestBuilder::send)).
///
/// # Examples
///
/// ```no_run
/// let response = barehttp::post("http://example.com/api").send(b"{\"a\":1}")?;
/// assert!(response.is_success() || response.status_code() >= 400);
/// # Ok::<(), barehttp::Error>(())
/// ```
#[must_use]
pub fn post(url: impl AsRef<str>) -> RequestBuilder {
  HttpClient::new().post(url)
}

/// PUT using a fresh default OS client (body via [`.send()`](ClientRequestBuilder::send)).
///
/// # Examples
///
/// ```no_run
/// let response = barehttp::put("http://example.com/item/1").send(b"updated")?;
/// assert!(response.status_code() > 0);
/// # Ok::<(), barehttp::Error>(())
/// ```
#[must_use]
pub fn put(url: impl AsRef<str>) -> RequestBuilder {
  HttpClient::new().put(url)
}

/// DELETE using a fresh default OS client.
///
/// # Examples
///
/// ```no_run
/// let response = barehttp::delete("http://example.com/item/1").call()?;
/// assert!(response.status_code() > 0);
/// # Ok::<(), barehttp::Error>(())
/// ```
#[must_use]
pub fn delete(url: impl AsRef<str>) -> RequestBuilder {
  HttpClient::new().delete(url)
}

/// HEAD using a fresh default OS client.
///
/// # Examples
///
/// ```no_run
/// let response = barehttp::head("http://example.com").call()?;
/// assert!(response.status_code() > 0);
/// # Ok::<(), barehttp::Error>(())
/// ```
#[must_use]
pub fn head(url: impl AsRef<str>) -> RequestBuilder {
  HttpClient::new().head(url)
}

/// PATCH using a fresh default OS client (body via [`.send()`](ClientRequestBuilder::send)).
///
/// # Examples
///
/// ```no_run
/// let response = barehttp::patch("http://example.com/item/1").send(b"{}")?;
/// assert!(response.status_code() > 0);
/// # Ok::<(), barehttp::Error>(())
/// ```
#[must_use]
pub fn patch(url: impl AsRef<str>) -> RequestBuilder {
  HttpClient::new().patch(url)
}

/// Client configuration ([`config::Config`], [`config::ConfigBuilder`]).
///
/// Kept as a module (not flattened to the crate root); see README “Module layout”.
pub mod config;
/// Request builder ([`ClientRequestBuilder`]).
///
/// Module path is stable; the type is also re-exported at the crate root.
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

// C-SEND-SYNC: clients, errors, config, and response are threaded / `Arc`-shared.
const _: fn() = || {
  const fn assert_send_sync<T: Send + Sync>() {}
  assert_send_sync::<Agent>();
  assert_send_sync::<HttpClient<OsBlockingSocket, OsDnsResolver>>();
  assert_send_sync::<Error>();
  assert_send_sync::<ParseError>();
  assert_send_sync::<DecompressError>();
  assert_send_sync::<DnsError>();
  assert_send_sync::<SocketError>();
  assert_send_sync::<InvalidRequest>();
  assert_send_sync::<IntoStringError>();
  assert_send_sync::<ParseMethodError>();
  assert_send_sync::<ExtensionMethod>();
  assert_send_sync::<Method>();
  assert_send_sync::<config::Config>();
  assert_send_sync::<config::ConfigBuilder>();
  assert_send_sync::<Response>();
  assert_send_sync::<RequestBuilder>();
  assert_send_sync::<ClientRequestBuilder<OsBlockingSocket, OsDnsResolver>>();
  assert_send_sync::<Headers>();
  assert_send_sync::<Uri<'static>>();
  assert_send_sync::<Version>();
};

#[cfg(feature = "cookie-jar")]
const _: fn() = || {
  const fn assert_send_sync<T: Send + Sync>() {}
  assert_send_sync::<cookie_jar::CookieStore>();
  assert_send_sync::<cookie_jar::CookieJar>();
  assert_send_sync::<cookie_jar::StoredCookie>();
};
