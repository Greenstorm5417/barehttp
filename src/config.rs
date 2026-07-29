use core::time::Duration;

/// Default max response body size (~10 MiB), matching ureq.
pub const DEFAULT_MAX_RESPONSE_BODY_SIZE: usize = 10 * 1024 * 1024;

/// Timeouts, redirects, default headers, pooling, and HTTPS policy for
/// [`crate::HttpClient`].
///
/// Pooling is on when [`Self::max_idle_per_host`] is greater than zero.
/// Build with [`Config::builder`] only; fields are private.
///
/// # `assume_tls_socket`
///
/// The [`crate::BlockingSocket`] already terminates TLS. Combining this with
/// cleartext [`crate::OsBlockingSocket`] returns `Error::TlsNotConfigured`.
///
/// # Examples
///
/// ```
/// use barehttp::config::Config;
/// use core::time::Duration;
///
/// let config = Config::builder()
///     .timeout_read(Some(Duration::from_secs(30)))
///     .max_redirects(5)
///     .user_agent("my-app/1.0")
///     .build();
/// assert_eq!(config.max_redirects(), 5);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Config {
  user_agent: alloc::string::String,
  max_redirects: u32,
  http_status_as_error: bool,
  max_response_header_size: usize,
  max_response_body_size: usize,
  timeout_connect: Option<Duration>,
  timeout_read: Option<Duration>,
  timeout_write: Option<Duration>,
  accept: alloc::string::String,
  https_only: bool,
  assume_tls_socket: bool,
  max_idle_per_host: usize,
  max_idle_age: Duration,
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

  /// `User-Agent` value; empty omits the default header.
  #[must_use]
  pub fn user_agent(&self) -> &str {
    &self.user_agent
  }

  /// Redirect hops to follow (`0` = return the redirect response).
  #[must_use]
  pub const fn max_redirects(&self) -> u32 {
    self.max_redirects
  }

  /// Map 4xx/5xx to [`crate::error::Error::HttpStatus`].
  #[must_use]
  pub const fn http_status_as_error(&self) -> bool {
    self.http_status_as_error
  }

  /// Max response header-section size in bytes.
  #[must_use]
  pub const fn max_response_header_size(&self) -> usize {
    self.max_response_header_size
  }

  /// Max response body size in bytes (default ~10 MiB).
  #[must_use]
  pub const fn max_response_body_size(&self) -> usize {
    self.max_response_body_size
  }

  /// Connect deadline.
  #[must_use]
  pub const fn timeout_connect(&self) -> Option<Duration> {
    self.timeout_connect
  }

  /// Read deadline.
  #[must_use]
  pub const fn timeout_read(&self) -> Option<Duration> {
    self.timeout_read
  }

  /// Write deadline.
  #[must_use]
  pub const fn timeout_write(&self) -> Option<Duration> {
    self.timeout_write
  }

  /// `Accept` value; empty omits the default header.
  #[must_use]
  pub fn accept(&self) -> &str {
    &self.accept
  }

  /// Reject non-`https` schemes.
  #[must_use]
  pub const fn https_only(&self) -> bool {
    self.https_only
  }

  /// Allow `https://` when the [`crate::BlockingSocket`] already does TLS.
  #[must_use]
  pub const fn assume_tls_socket(&self) -> bool {
    self.assume_tls_socket
  }

  /// Idle sockets kept per host (`0` disables pooling).
  #[must_use]
  pub const fn max_idle_per_host(&self) -> usize {
    self.max_idle_per_host
  }

  /// Drop pooled sockets older than this (default 15s).
  #[must_use]
  pub const fn max_idle_age(&self) -> Duration {
    self.max_idle_age
  }

  pub(crate) const fn set_timeout_connect(
    &mut self,
    v: Option<Duration>,
  ) {
    self.timeout_connect = v;
  }

  pub(crate) const fn set_timeout_read(
    &mut self,
    v: Option<Duration>,
  ) {
    self.timeout_read = v;
  }

  pub(crate) const fn set_timeout_write(
    &mut self,
    v: Option<Duration>,
  ) {
    self.timeout_write = v;
  }

  pub(crate) const fn set_max_response_body_size(
    &mut self,
    v: usize,
  ) {
    self.max_response_body_size = v;
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
