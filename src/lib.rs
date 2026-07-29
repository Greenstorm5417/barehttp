//! Blocking HTTP client for `no_std` + `alloc`. Cleartext HTTP only unless your socket does TLS (`Config::assume_tls_socket`).
//!
//! ```no_run
//! let response = barehttp::get("http://httpbin.org/get")?;
//! println!("{}", response.text()?);
//! # Ok::<(), barehttp::Error>(())
//! ```
//!
//! [`HttpClient`] takes custom headers, follows redirects, and accepts [`config::Config`].
//! `text` and `is_success` are methods on [`Response`].

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

pub use dns::adapter::DnsResolver;
pub use dns::resolver::OsDnsResolver;
pub use error::{DnsError, ParseError, SocketError};
pub use socket::adapter::BlockingSocket;
pub use socket::{OsBlockingSocket, SocketAddr};
pub use util::IpAddr;

pub use headers::Headers;
pub use method::Method;
pub use parser::Response;
pub use parser::version::Version;

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
pub fn post(
  url: &str,
  body: impl AsRef<[u8]>,
) -> Result<Response, Error> {
  HttpClient::new().post(url).send(body)
}

/// Client configuration.
pub mod config;
/// Request builder.
pub mod request_builder;

mod client;
mod dns;
mod error;
mod headers;
mod method;
pub(crate) mod parser;
pub(crate) mod socket;
mod transport;
pub(crate) mod util;
