use core::time::Duration;

/// Policy for forwarding authorization headers during redirects
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedirectAuthHeaders {
  /// Never forward authorization headers on redirects
  Never,
  /// Forward authorization headers only when redirecting to the same host
  SameHost,
}

/// HTTP redirect following behavior
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedirectPolicy {
  /// Follow redirects and return the final response
  Follow,
  /// Follow redirects but return the last redirect response
  FollowReturnLast,
  /// Do not follow redirects
  NoFollow,
}

/// How to handle HTTP error status codes (4xx, 5xx)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpStatusHandling {
  /// Treat 4xx and 5xx status codes as errors
  AsError,
  /// Treat all status codes as successful responses
  AsResponse,
}

/// Protocol restrictions for requests
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolRestriction {
  /// Only allow HTTPS requests
  HttpsOnly,
  /// Allow both HTTP and HTTPS requests
  Any,
}

/// HTTP client configuration
///
/// Controls behavior for timeouts, redirects, headers, and protocol restrictions.
#[derive(Debug, Clone)]
pub struct Config {
  /// General timeout for the entire request
  pub timeout: Option<Duration>,
  /// User-Agent header value
  pub user_agent: Option<alloc::string::String>,
  /// How to handle HTTP redirects
  pub redirect_policy: RedirectPolicy,
  /// Maximum number of redirects to follow
  pub max_redirects: u32,
  /// How to handle 4xx/5xx status codes
  pub http_status_handling: HttpStatusHandling,
  /// Policy for forwarding auth headers on redirects
  pub redirect_auth_headers: RedirectAuthHeaders,
  /// Maximum size for response headers in bytes
  pub max_response_header_size: usize,
  /// Timeout for establishing connection
  pub timeout_connect: Option<Duration>,
  /// Timeout for reading response
  pub timeout_read: Option<Duration>,
  /// Accept header value
  pub accept: Option<alloc::string::String>,
  /// Protocol restrictions (HTTP/HTTPS)
  pub protocol_restriction: ProtocolRestriction,
}

impl Default for Config {
  fn default() -> Self {
    Self {
      timeout: None,
      user_agent: Some(alloc::string::String::from("barehttp/1.0")),
      redirect_policy: RedirectPolicy::Follow,
      max_redirects: 10,
      http_status_handling: HttpStatusHandling::AsError,
      redirect_auth_headers: RedirectAuthHeaders::Never,
      max_response_header_size: 64 * 1024,
      timeout_connect: None,
      timeout_read: None,
      accept: Some(alloc::string::String::from("*/*")),
      protocol_restriction: ProtocolRestriction::Any,
    }
  }
}

/// Builder for constructing HTTP client configuration
///
/// Provides a fluent interface for setting configuration options.
pub struct ConfigBuilder {
  config: Config,
}

impl ConfigBuilder {
  /// Create a new config builder with default values
  #[must_use]
  pub fn new() -> Self {
    Self {
      config: Config::default(),
    }
  }

  /// Set the general request timeout
  #[must_use]
  pub const fn timeout(mut self, duration: Duration) -> Self {
    self.config.timeout = Some(duration);
    self
  }

  /// Set the User-Agent header
  #[must_use]
  pub fn user_agent(mut self, agent: impl Into<alloc::string::String>) -> Self {
    self.config.user_agent = Some(agent.into());
    self
  }

  /// Set the redirect following policy
  #[must_use]
  pub const fn redirect_policy(mut self, policy: RedirectPolicy) -> Self {
    self.config.redirect_policy = policy;
    self
  }

  /// Set the maximum number of redirects to follow
  #[must_use]
  pub const fn max_redirects(mut self, max: u32) -> Self {
    self.config.max_redirects = max;
    self
  }

  /// Set how to handle HTTP error status codes
  #[must_use]
  pub const fn http_status_handling(mut self, handling: HttpStatusHandling) -> Self {
    self.config.http_status_handling = handling;
    self
  }

  /// Set the policy for forwarding authorization headers on redirects
  #[must_use]
  pub const fn redirect_auth_headers(mut self, policy: RedirectAuthHeaders) -> Self {
    self.config.redirect_auth_headers = policy;
    self
  }

  /// Set the maximum response header size in bytes
  #[must_use]
  pub const fn max_response_header_size(mut self, size: usize) -> Self {
    self.config.max_response_header_size = size;
    self
  }

  /// Set the connection timeout
  #[must_use]
  pub const fn timeout_connect(mut self, duration: Duration) -> Self {
    self.config.timeout_connect = Some(duration);
    self
  }

  /// Set the read timeout
  #[must_use]
  pub const fn timeout_read(mut self, duration: Duration) -> Self {
    self.config.timeout_read = Some(duration);
    self
  }

  #[must_use]
  /// Set the Accept header value
  pub fn accept(mut self, value: impl Into<alloc::string::String>) -> Self {
    self.config.accept = Some(value.into());
    self
  }

  #[must_use]
  /// Set protocol restrictions (HTTP/HTTPS only)
  pub const fn protocol_restriction(mut self, restriction: ProtocolRestriction) -> Self {
    self.config.protocol_restriction = restriction;
    self
  }

  #[must_use]
  /// Build the final configuration
  pub fn build(self) -> Config {
    self.config
  }
}

impl Default for ConfigBuilder {
  fn default() -> Self {
    Self::new()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn config_default_values() {
    let config = Config::default();
    
    assert!(config.timeout.is_none());
    assert_eq!(config.user_agent, Some(alloc::string::String::from("barehttp/1.0")));
    assert_eq!(config.redirect_policy, RedirectPolicy::Follow);
    assert_eq!(config.max_redirects, 10);
    assert_eq!(config.http_status_handling, HttpStatusHandling::AsError);
    assert_eq!(config.redirect_auth_headers, RedirectAuthHeaders::Never);
    assert_eq!(config.max_response_header_size, 64 * 1024);
    assert!(config.timeout_connect.is_none());
    assert!(config.timeout_read.is_none());
    assert_eq!(config.accept, Some(alloc::string::String::from("*/*")));
    assert_eq!(config.protocol_restriction, ProtocolRestriction::Any);
  }

  #[test]
  fn config_builder_timeout() {
    let config = ConfigBuilder::new()
      .timeout(Duration::from_secs(30))
      .build();
    
    assert_eq!(config.timeout, Some(Duration::from_secs(30)));
  }

  #[test]
  fn config_builder_user_agent() {
    let config = ConfigBuilder::new()
      .user_agent("MyClient/1.0")
      .build();
    
    assert_eq!(config.user_agent, Some(alloc::string::String::from("MyClient/1.0")));
  }

  #[test]
  fn config_builder_redirect_policy() {
    let config = ConfigBuilder::new()
      .redirect_policy(RedirectPolicy::NoFollow)
      .build();
    
    assert_eq!(config.redirect_policy, RedirectPolicy::NoFollow);
  }

  #[test]
  fn config_builder_max_redirects() {
    let config = ConfigBuilder::new()
      .max_redirects(5)
      .build();
    
    assert_eq!(config.max_redirects, 5);
  }

  #[test]
  fn config_builder_http_status_handling() {
    let config = ConfigBuilder::new()
      .http_status_handling(HttpStatusHandling::AsResponse)
      .build();
    
    assert_eq!(config.http_status_handling, HttpStatusHandling::AsResponse);
  }

  #[test]
  fn config_builder_chaining() {
    let config = ConfigBuilder::new()
      .timeout(Duration::from_secs(10))
      .user_agent("Test/1.0")
      .max_redirects(3)
      .http_status_handling(HttpStatusHandling::AsResponse)
      .build();
    
    assert_eq!(config.timeout, Some(Duration::from_secs(10)));
    assert_eq!(config.max_redirects, 3);
    assert_eq!(config.http_status_handling, HttpStatusHandling::AsResponse);
  }

  #[test]
  fn config_builder_protocol_restriction() {
    let config = ConfigBuilder::new()
      .protocol_restriction(ProtocolRestriction::HttpsOnly)
      .build();
    
    assert_eq!(config.protocol_restriction, ProtocolRestriction::HttpsOnly);
  }

  #[test]
  fn config_builder_timeouts() {
    let config = ConfigBuilder::new()
      .timeout_connect(Duration::from_secs(5))
      .timeout_read(Duration::from_secs(30))
      .build();
    
    assert_eq!(config.timeout_connect, Some(Duration::from_secs(5)));
    assert_eq!(config.timeout_read, Some(Duration::from_secs(30)));
  }

  #[test]
  fn config_builder_accept_header() {
    let config = ConfigBuilder::new()
      .accept("application/json")
      .build();
    
    assert_eq!(config.accept, Some(alloc::string::String::from("application/json")));
  }
}
