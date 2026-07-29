//! Blocking HTTP client for `no_std` + `alloc`. Cleartext HTTP only unless your socket does TLS (`Config::assume_tls_socket`).
//!
//! ```no_run
//! let response = barehttp::get("http://httpbin.org/get")?;
//! println!("{}", response.text()?);
//! # Ok::<(), barehttp::Error>(())
//! ```
//!
//! Use [`HttpClient`] when you need headers, redirects, or a custom [`config::Config`].
//! Response helpers (`text`, `is_success`, …) are inherent methods on [`Response`].

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
/// RFC 6265 cookie jar for request/response cookie handling.
pub mod cookie_jar;

pub use client::HttpClient;
pub use error::Error;
pub use request_builder::IntoBody;

pub use dns::adapter::DnsResolver;
pub use dns::resolver::OsDnsResolver;
pub use error::{DnsError, SocketError};
pub use socket::adapter::{BlockingSocket, SocketAddr};
pub use socket::blocking::OsBlockingSocket;
pub use socket::flags::SocketFlags;
pub use util::IpAddr;

pub use body::Body;
pub use headers::Headers;
pub use method::Method;
pub use parser::status::{StatusClass, StatusCode};
pub use parser::version::Version;
pub use parser::Response;

/// GET with default OS adapters.
///
/// # Errors
/// URL parse, DNS, connect, or HTTP failure.
pub fn get(url: &str) -> Result<Response, Error> {
  HttpClient::new().get(url).call()
}

/// POST with default OS adapters.
///
/// # Errors
/// URL parse, DNS, connect, or HTTP failure.
pub fn post(url: &str, body: impl IntoBody) -> Result<Response, Error> {
  HttpClient::new().post(url).send(body)
}

/// PUT with default OS adapters.
///
/// # Errors
/// URL parse, DNS, connect, or HTTP failure.
pub fn put(url: &str, body: impl IntoBody) -> Result<Response, Error> {
  HttpClient::new().put(url).send(body)
}

/// DELETE with default OS adapters.
///
/// # Errors
/// URL parse, DNS, connect, or HTTP failure.
pub fn delete(url: &str) -> Result<Response, Error> {
  HttpClient::new().delete(url).call()
}

/// HEAD with default OS adapters.
///
/// # Errors
/// URL parse, DNS, connect, or HTTP failure.
pub fn head(url: &str) -> Result<Response, Error> {
  HttpClient::new().head(url).call()
}

/// PATCH with default OS adapters.
///
/// # Errors
/// URL parse, DNS, connect, or HTTP failure.
pub fn patch(url: &str, body: impl IntoBody) -> Result<Response, Error> {
  HttpClient::new().patch(url).send(body)
}

/// Client configuration.
pub mod config;
/// Typestate request builder.
pub mod request_builder;

mod body;
mod client;
mod dns;
mod error;
mod headers;
mod method;
pub(crate) mod parser;
pub(crate) mod socket;
mod transport;
pub(crate) mod util;
