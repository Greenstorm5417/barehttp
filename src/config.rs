use core::time::Duration;

/// Default max response body size (~10 MiB), matching ureq.
pub const DEFAULT_MAX_RESPONSE_BODY_SIZE: usize = 10 * 1024 * 1024;

/// Defaults and knobs for [`crate::HttpClient`]: timeouts, redirects, headers,
/// pooling, and TLS / HTTPS policy.
///
/// Pooling is enabled when [`Self::max_idle_per_host`] is greater than zero.
/// Build with [`Config::builder`] or struct update on [`Config::default`].
///
/// # `assume_tls_socket`
///
/// Means your [`crate::BlockingSocket`] already terminates TLS. Do not combine
/// with cleartext [`crate::OsBlockingSocket`]; that pair is rejected as
/// `Error::TlsNotConfigured`.
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct Config {
  /// `User-Agent` value; empty omits the default header.
  pub user_agent: alloc::string::String,
  /// Redirect hops to follow (`0` = return the redirect response).
  pub max_redirects: u32,
  /// Map 4xx/5xx to [`crate::error::Error::HttpStatus`].
  pub http_status_as_error: bool,
  /// Max response header-section size in bytes.
  pub max_response_header_size: usize,
  /// Max response body size in bytes (default ~10 MiB).
  pub max_response_body_size: usize,
  /// Connect deadline.
  pub timeout_connect: Option<Duration>,
  /// Read deadline.
  pub timeout_read: Option<Duration>,
  /// Write deadline.
  pub timeout_write: Option<Duration>,
  /// `Accept` value; empty omits the default header.
  pub accept: alloc::string::String,
  /// Reject non-`https` schemes.
  pub https_only: bool,
  /// Allow `https://` when your [`crate::BlockingSocket`] already does TLS (default `false`).
  ///
  /// The OS socket is cleartext; without this flag, `https://` is rejected.
  /// Setting this with [`crate::OsBlockingSocket`] yields `TlsNotConfigured`.
  pub assume_tls_socket: bool,
  /// Idle sockets kept per host (`0` disables pooling).
  pub max_idle_per_host: usize,
  /// Drop pooled sockets older than this (default 15s).
  pub max_idle_age: Duration,
}

impl Default for Config {
  fn default() -> Self {
    Self {
      user_agent: alloc::string::String::from("barehttp/1.0"),
      max_redirects: 10,
      http_status_as_error: true,
      max_response_header_size: 64 * 1024,
      max_response_body_size: DEFAULT_MAX_RESPONSE_BODY_SIZE,
      timeout_connect: None,
      timeout_read: None,
      timeout_write: None,
      accept: alloc::string::String::from("*/*"),
      https_only: false,
      assume_tls_socket: false,
      max_idle_per_host: 3,
      max_idle_age: Duration::from_secs(15),
    }
  }
}

impl Config {
  /// Builder starting from [`Config::default`].
  #[must_use]
  pub fn builder() -> ConfigBuilder {
    ConfigBuilder { config: Self::default() }
  }
}

/// Builder for [`Config`].
#[derive(Debug, Clone)]
pub struct ConfigBuilder {
  config: Config,
}

impl ConfigBuilder {
  /// Return the finished [`Config`].
  #[must_use]
  pub fn build(self) -> Config {
    self.config
  }

  /// Set [`Config::user_agent`].
  #[must_use]
  pub fn user_agent(
    mut self,
    v: impl Into<alloc::string::String>,
  ) -> Self {
    self.config.user_agent = v.into();
    self
  }

  /// Set [`Config::max_redirects`] (`0` = do not follow redirects).
  #[must_use]
  pub const fn max_redirects(
    mut self,
    v: u32,
  ) -> Self {
    self.config.max_redirects = v;
    self
  }

  /// Set [`Config::http_status_as_error`].
  #[must_use]
  pub const fn http_status_as_error(
    mut self,
    v: bool,
  ) -> Self {
    self.config.http_status_as_error = v;
    self
  }

  /// Set [`Config::max_response_header_size`].
  #[must_use]
  pub const fn max_response_header_size(
    mut self,
    v: usize,
  ) -> Self {
    self.config.max_response_header_size = v;
    self
  }

  /// Set [`Config::max_response_body_size`].
  #[must_use]
  pub const fn max_response_body_size(
    mut self,
    v: usize,
  ) -> Self {
    self.config.max_response_body_size = v;
    self
  }

  /// Set [`Config::timeout_connect`].
  #[must_use]
  pub const fn timeout_connect(
    mut self,
    v: Option<Duration>,
  ) -> Self {
    self.config.timeout_connect = v;
    self
  }

  /// Set [`Config::timeout_read`].
  #[must_use]
  pub const fn timeout_read(
    mut self,
    v: Option<Duration>,
  ) -> Self {
    self.config.timeout_read = v;
    self
  }

  /// Set [`Config::timeout_write`].
  #[must_use]
  pub const fn timeout_write(
    mut self,
    v: Option<Duration>,
  ) -> Self {
    self.config.timeout_write = v;
    self
  }

  /// Set [`Config::accept`].
  #[must_use]
  pub fn accept(
    mut self,
    v: impl Into<alloc::string::String>,
  ) -> Self {
    self.config.accept = v.into();
    self
  }

  /// Set [`Config::https_only`].
  #[must_use]
  pub const fn https_only(
    mut self,
    v: bool,
  ) -> Self {
    self.config.https_only = v;
    self
  }

  /// Set [`Config::assume_tls_socket`].
  #[must_use]
  pub const fn assume_tls_socket(
    mut self,
    v: bool,
  ) -> Self {
    self.config.assume_tls_socket = v;
    self
  }

  /// Set [`Config::max_idle_per_host`].
  #[must_use]
  pub const fn max_idle_per_host(
    mut self,
    v: usize,
  ) -> Self {
    self.config.max_idle_per_host = v;
    self
  }

  /// Set [`Config::max_idle_age`].
  #[must_use]
  pub const fn max_idle_age(
    mut self,
    v: Duration,
  ) -> Self {
    self.config.max_idle_age = v;
    self
  }
}
