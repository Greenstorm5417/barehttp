//! # barehttp
//!
//! **A minimal, explicit HTTP client for Rust**
//!
//! barehttp is a low-level, blocking HTTP client designed for developers who want
//! **predictable behavior, full control, and minimal dependencies**.
//!
//! It supports `no_std` (with `alloc`), avoids global state, and exposes all network
//! behavior explicitly. There is no async runtime, no hidden connection pooling,
//! and no built-in TLS—you bring your own via adapters.
//!
//! ## Key Features
//!
//! - **Minimal and explicit**: No global state, no implicit behavior
//! - **`no_std` compatible**: Core works with `alloc` only
//! - **Blocking I/O**: Simple, predictable execution model
//! - **Generic adapters**: Custom socket and DNS implementations
//! - **Compile-time safety**: Typestate enforces correct body usage
//!
//! ## Quick Start
//!
//! ```no_run
//! use barehttp::response::ResponseExt;
//!
//! let response = barehttp::get("http://httpbin.org/get")?;
//! let body = response.text()?;
//! println!("{}", body);
//! # Ok::<(), barehttp::Error>(())
//! ```
//!
//! ## Using `HttpClient`
//!
//! For repeated requests or more control, use [`HttpClient`]:
//!
//! ```no_run
//! use barehttp::HttpClient;
//! use barehttp::response::ResponseExt;
//!
//! let mut client = HttpClient::new()?;
//!
//! let response = client
//!     .get("http://httpbin.org/get")
//!     .header("User-Agent", "my-app/1.0")
//!     .call()?;
//!
//! println!("Status: {}", response.status_code);
//! println!("Body: {}", response.text()?);
//! # Ok::<(), barehttp::Error>(())
//! ```
//!
//! ## Configuration
//!
//! Client behavior is controlled via [`config::Config`] and [`config::ConfigBuilder`]:
//!
//! ```no_run
//! use barehttp::HttpClient;
//! use barehttp::config::ConfigBuilder;
//! use core::time::Duration;
//!
//! let config = ConfigBuilder::new()
//!     .timeout(Duration::from_secs(30))
//!     .max_redirects(5)
//!     .user_agent("my-app/2.0")
//!     .build();
//!
//! let mut client = HttpClient::with_config(config)?;
//!
//! let response = client.get("http://httpbin.org/get").call()?;
//! # Ok::<(), barehttp::Error>(())
//! ```
//!
//! ## Making Requests
//!
//! ### GET
//!
//! ```no_run
//! let mut client = barehttp::HttpClient::new()?;
//!
//! let response = client.get("http://httpbin.org/get").call()?;
//! # Ok::<(), barehttp::Error>(())
//! ```
//!
//! ### POST with body
//!
//! ```no_run
//! let mut client = barehttp::HttpClient::new()?;
//!
//! let response = client
//!     .post("http://httpbin.org/post")
//!     .header("Content-Type", "application/json")
//!     .send(br#"{"name":"test"}"#.to_vec())?;
//! # Ok::<(), barehttp::Error>(())
//! ```
//!
//! ## Type-Safe Request Bodies
//!
//! Request methods enforce body semantics at compile time:
//!
//! - GET, HEAD, DELETE, OPTIONS → methods without body
//! - POST, PUT, PATCH → methods with body
//!
//! ```compile_fail
//! let mut client = barehttp::HttpClient::new()?;
//! client.get("http://example.com").send(vec![])?;
//! ```
//!
//! ```no_run
//! let mut client = barehttp::HttpClient::new()?;
//! client.post("http://example.com").send(b"data".to_vec())?;
//! # Ok::<(), barehttp::Error>(())
//! ```
//!
//! ## Error Handling
//!
//! All operations return `Result<T, barehttp::Error>`.
//!
//! Errors include:
//! - DNS resolution failures
//! - Socket I/O errors
//! - Parse errors
//! - HTTP status errors (4xx/5xx by default)
//!
//! ```no_run
//! use barehttp::{Error, HttpClient};
//!
//! let mut client = HttpClient::new()?;
//!
//! match client.get("http://httpbin.org/status/404").call() {
//!     Ok(resp) => println!("Status: {}", resp.status_code),
//!     Err(Error::HttpStatus(code)) => println!("HTTP error: {}", code),
//!     Err(e) => println!("Other error: {:?}", e),
//! }
//! # Ok::<(), barehttp::Error>(())
//! ```
//!
//! Automatic HTTP status errors can be disabled via configuration.
//!
//! ## Response Helpers
//!
//! The [`response::ResponseExt`] trait provides helpers for common tasks:
//!
//! ```no_run
//! use barehttp::response::ResponseExt;
//!
//! let mut client = barehttp::HttpClient::new()?;
//! let response = client.get("http://httpbin.org/get").call()?;
//!
//! if response.is_success() {
//!     let text = response.text()?;
//!     println!("{}", text);
//! }
//! # Ok::<(), barehttp::Error>(())
//! ```
//!
//! ## Custom Socket and DNS Adapters
//!
//! barehttp’s networking is fully pluggable.
//!
//! Implement `BlockingSocket` and `DnsResolver` traits to provide:
//!
//! - TLS via external libraries
//! - Proxies or tunnels
//! - Embedded or WASM networking
//! - Test mocks
//!
//! ```no_run
//! use barehttp::{HttpClient, OsBlockingSocket, OsDnsResolver};
//!
//! let dns = OsDnsResolver::new();
//! let mut client: HttpClient<OsBlockingSocket, _> = HttpClient::new_with_adapters(dns);
//! let response = client.get("http://example.com").call()?;
//! # Ok::<(), barehttp::Error>(())
//! ```
//!
//! ## Design Notes
//!
//! - Blocking I/O keeps the API simple and dependency-free
//! - Each request is independent and explicit
//! - No shared state or background behavior
//!
//! barehttp is intended for environments where **clarity and control matter more than convenience**.

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
/// RFC 6265 compliant cookie storage and management
///
/// This module provides a `CookieStore` for automatic cookie handling
/// in HTTP requests and responses, including domain/path matching and expiration.
pub mod cookie_jar;

// Re-exports of core types
pub use client::HttpClient;
pub use error::Error;

// Re-exports of default OS adapters
pub use dns::resolver::OsDnsResolver;
pub use socket::blocking::OsBlockingSocket;

// Re-exports of request/response types
pub use body::Body;
pub use headers::{HeaderName, Headers};
pub use method::Method;
pub use parser::status::{StatusClass, StatusCode};
pub use parser::version::Version;
pub use request::Request;

// Convenience functions for quick HTTP requests

/// Convenience function for GET requests
///
/// Creates a new client with default OS adapters and executes a GET request.
///
/// # Errors
/// Returns an error if URL parsing, DNS resolution, socket connection, or HTTP communication fails.
pub fn get(url: &str) -> Result<parser::Response, crate::error::Error> {
  let mut client = HttpClient::new()?;
  client.get(url).call()
}

/// Convenience function for POST requests
///
/// Creates a new client with default OS adapters and executes a POST request.
///
/// # Errors
/// Returns an error if URL parsing, DNS resolution, socket connection, or HTTP communication fails.
pub fn post(
  url: &str,
  body: alloc::vec::Vec<u8>,
) -> Result<parser::Response, crate::error::Error> {
  let mut client = HttpClient::new()?;
  client.post(url).send(body)
}

/// Convenience function for PUT requests
///
/// Creates a new client with default OS adapters and executes a PUT request.
///
/// # Errors
/// Returns an error if URL parsing, DNS resolution, socket connection, or HTTP communication fails.
pub fn put(
  url: &str,
  body: alloc::vec::Vec<u8>,
) -> Result<parser::Response, crate::error::Error> {
  let mut client = HttpClient::new()?;
  client.put(url).send(body)
}

/// Convenience function for DELETE requests
///
/// Creates a new client with default OS adapters and executes a DELETE request.
///
/// # Errors
/// Returns an error if URL parsing, DNS resolution, socket connection, or HTTP communication fails.
pub fn delete(url: &str) -> Result<parser::Response, crate::error::Error> {
  let mut client = HttpClient::new()?;
  client.delete(url).call()
}

/// Convenience function for HEAD requests
///
/// Creates a new client with default OS adapters and executes a HEAD request.
///
/// # Errors
/// Returns an error if URL parsing, DNS resolution, socket connection, or HTTP communication fails.
pub fn head(url: &str) -> Result<parser::Response, crate::error::Error> {
  let mut client = HttpClient::new()?;
  client.head(url).call()
}

/// Convenience function for OPTIONS requests
///
/// Creates a new client with default OS adapters and executes an OPTIONS request.
///
/// # Errors
/// Returns an error if URL parsing, DNS resolution, socket connection, or HTTP communication fails.
pub fn options(url: &str) -> Result<parser::Response, crate::error::Error> {
  let mut client = HttpClient::new()?;
  client.options(url).call()
}

/// Convenience function for PATCH requests
///
/// Creates a new client with default OS adapters and executes a PATCH request.
///
/// # Errors
/// Returns an error if URL parsing, DNS resolution, socket connection, or HTTP communication fails.
pub fn patch(
  url: &str,
  body: alloc::vec::Vec<u8>,
) -> Result<parser::Response, crate::error::Error> {
  let mut client = HttpClient::new()?;
  client.patch(url).send(body)
}

/// Convenience function for TRACE requests
///
/// Creates a new client with default OS adapters and executes a TRACE request.
///
/// # Errors
/// Returns an error if URL parsing, DNS resolution, socket connection, or HTTP communication fails.
pub fn trace(url: &str) -> Result<parser::Response, crate::error::Error> {
  let mut client = HttpClient::new()?;
  client.trace(url).call()
}

/// Convenience function for CONNECT requests
///
/// Creates a new client with default OS adapters and executes a CONNECT request.
///
/// # Errors
/// Returns an error if URL parsing, DNS resolution, socket connection, or HTTP communication fails.
pub fn connect(url: &str) -> Result<parser::Response, crate::error::Error> {
  let mut client = HttpClient::new()?;
  client.connect(url).call()
}

// Public modules

/// Configuration for HTTP client behavior
pub mod config;
/// Typestate request builder for compile-time safety
pub mod request_builder;
/// Response extensions and helpers
pub mod response;

mod body;
mod client;
mod dns;
mod error;
mod headers;
mod method;
pub(crate) mod parser;
mod request;
pub(crate) mod socket;
mod transport;
pub(crate) mod util;
