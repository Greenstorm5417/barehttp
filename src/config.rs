use core::time::Duration;

/// Timeouts, redirects, headers, pooling, and protocol flags for [`crate::HttpClient`].
///
/// Connection pooling is on when [`Self::max_idle_per_host`] is greater than zero.
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct Config {
  /// User-Agent header value (empty = omit default)
  pub user_agent: alloc::string::String,
  /// Follow HTTP redirects
  pub follow_redirects: bool,
  /// Maximum number of redirects to follow
  pub max_redirects: u32,
  /// Treat 4xx/5xx as [`crate::error::Error::HttpStatus`]
  pub http_status_as_error: bool,
  /// Maximum size for response headers in bytes
  pub max_response_header_size: usize,
  /// Timeout for establishing connection
  pub timeout_connect: Option<Duration>,
  /// Timeout for reading response
  pub timeout_read: Option<Duration>,
  /// Timeout for writing request
  pub timeout_write: Option<Duration>,
  /// Accept header value (empty = omit default)
  pub accept: alloc::string::String,
  /// Reject non-HTTPS schemes
  pub https_only: bool,
  /// Allow `https://` when your [`crate::BlockingSocket`] already does TLS (default `false`).
  ///
  /// The OS socket is cleartext; without this flag, `https://` is rejected.
  pub assume_tls_socket: bool,
  /// Maximum idle connections to keep per host (`0` disables pooling)
  pub max_idle_per_host: usize,
}

impl Default for Config {
  fn default() -> Self {
    Self {
      user_agent: alloc::string::String::from("barehttp/1.0"),
      follow_redirects: true,
      max_redirects: 10,
      http_status_as_error: true,
      max_response_header_size: 64 * 1024,
      timeout_connect: None,
      timeout_read: None,
      timeout_write: None,
      accept: alloc::string::String::from("*/*"),
      https_only: false,
      assume_tls_socket: false,
      max_idle_per_host: 5,
    }
  }
}
