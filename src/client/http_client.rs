use crate::body::Body;
use crate::client::policy::{PolicyDecision, RequestPolicy};
use crate::client::request_executor::RequestExecutor;
use crate::config::Config;
use crate::dns::DnsResolver;
use crate::error::Error;
use crate::parser::Response;
use crate::parser::uri::Uri;
use crate::request_builder::ClientRequestBuilder;
use crate::socket::BlockingSocket;
use crate::transport::ConnectionPool;
use alloc::string::String;
use alloc::vec::Vec;

#[cfg(feature = "cookie-jar")]
use crate::cookie_jar::CookieStore;

/// Generic HTTP client with customizable socket and DNS adapters
///
/// This client supports `no_std` environments and allows complete control over
/// network operations through generic socket (`S`) and DNS (`D`) adapters.
///
/// # Type Parameters
/// - `S`: Socket implementation (must implement `BlockingSocket`)
/// - `D`: DNS resolver implementation (must implement `DnsResolver`)
///
/// # Examples
/// ```no_run
/// use barehttp::HttpClient;
///
/// let mut client = HttpClient::new()?;
///
/// let response = client.get("http://example.com").call()?;
/// # Ok::<(), barehttp::Error>(())
/// ```
pub struct HttpClient<S, D> {
  pool: ConnectionPool<S>,
  dns: D,
  config: Config,
  #[cfg(feature = "cookie-jar")]
  cookie_store: CookieStore,
}

impl HttpClient<crate::socket::blocking::OsBlockingSocket, crate::dns::resolver::OsDnsResolver> {
  /// Create a new HTTP client with OS adapters and default configuration
  ///
  /// Uses the operating system's default socket and DNS resolver.
  ///
  /// # Errors
  /// Returns an error if socket initialization fails.
  pub fn new() -> Result<Self, Error> {
    let config = Config::default();
    Ok(Self {
      pool: ConnectionPool::new(config.max_idle_per_host, config.idle_timeout),
      dns: crate::dns::resolver::OsDnsResolver::new(),
      config,
      #[cfg(feature = "cookie-jar")]
      cookie_store: CookieStore::new(),
    })
  }

  /// Create a new HTTP client with OS adapters and custom configuration
  ///
  /// Uses the operating system's default socket and DNS resolver.
  ///
  /// # Errors
  /// Returns an error if socket initialization fails.
  pub const fn with_config(config: Config) -> Result<Self, Error> {
    Ok(Self {
      pool: ConnectionPool::new(config.max_idle_per_host, config.idle_timeout),
      dns: crate::dns::resolver::OsDnsResolver::new(),
      config,
      #[cfg(feature = "cookie-jar")]
      cookie_store: CookieStore::new(),
    })
  }
}

impl<S, D> HttpClient<S, D>
where
  S: BlockingSocket,
  D: DnsResolver,
{
  /// Create a new HTTP client with custom socket and DNS adapters
  ///
  /// For most use cases, prefer `HttpClient::new()` which uses OS defaults.
  /// Use this when you need custom socket or DNS implementations.
  ///
  /// # Parameters
  /// - `dns`: Custom DNS resolver for hostname resolution
  pub fn new_with_adapters(dns: D) -> Self {
    let config = Config::default();
    Self {
      pool: ConnectionPool::new(config.max_idle_per_host, config.idle_timeout),
      dns,
      config,
      #[cfg(feature = "cookie-jar")]
      cookie_store: CookieStore::new(),
    }
  }

  /// Create a new HTTP client with custom adapters and configuration
  ///
  /// # Parameters
  /// - `dns`: Custom DNS resolver for hostname resolution
  /// - `config`: Custom client configuration
  #[allow(clippy::missing_const_for_fn)]
  pub fn with_adapters_and_config(
    dns: D,
    config: Config,
  ) -> Self {
    Self {
      pool: ConnectionPool::new(config.max_idle_per_host, config.idle_timeout),
      dns,
      config,
      #[cfg(feature = "cookie-jar")]
      cookie_store: CookieStore::new(),
    }
  }

  /// TODO: Per-request config should overlay, not mutate client state
  /// This is temporary until we implement proper config scoping
  pub(crate) fn apply_request_config(
    &mut self,
    config: Config,
  ) {
    self.config = config;
  }

  /// Start building a GET request
  ///
  /// Returns a request builder that enforces no request body at compile time.
  pub fn get(
    &mut self,
    url: impl Into<String>,
  ) -> ClientRequestBuilder<'_, S, D, crate::request_builder::WithoutBody> {
    ClientRequestBuilder::<'_, S, D, crate::request_builder::WithoutBody>::new(self, crate::method::Method::Get, url)
  }

  /// Start building a POST request
  ///
  /// Returns a request builder that requires a request body.
  pub fn post(
    &mut self,
    url: impl Into<String>,
  ) -> ClientRequestBuilder<'_, S, D, crate::request_builder::WithBody> {
    ClientRequestBuilder::<'_, S, D, crate::request_builder::WithBody>::new(self, crate::method::Method::Post, url)
  }

  /// Start building a PUT request
  ///
  /// Returns a request builder that requires a request body.
  pub fn put(
    &mut self,
    url: impl Into<String>,
  ) -> ClientRequestBuilder<'_, S, D, crate::request_builder::WithBody> {
    ClientRequestBuilder::<'_, S, D, crate::request_builder::WithBody>::new(self, crate::method::Method::Put, url)
  }

  /// Start building a DELETE request
  ///
  /// Returns a request builder with no body by default (use `force_send_body()` if needed).
  pub fn delete(
    &mut self,
    url: impl Into<String>,
  ) -> ClientRequestBuilder<'_, S, D, crate::request_builder::WithoutBody> {
    ClientRequestBuilder::<'_, S, D, crate::request_builder::WithoutBody>::new(self, crate::method::Method::Delete, url)
  }

  /// Start building a HEAD request
  ///
  /// Returns a request builder that enforces no request body.
  pub fn head(
    &mut self,
    url: impl Into<String>,
  ) -> ClientRequestBuilder<'_, S, D, crate::request_builder::WithoutBody> {
    ClientRequestBuilder::<'_, S, D, crate::request_builder::WithoutBody>::new(self, crate::method::Method::Head, url)
  }

  /// Start building an OPTIONS request
  ///
  /// Returns a request builder that enforces no request body.
  pub fn options(
    &mut self,
    url: impl Into<String>,
  ) -> ClientRequestBuilder<'_, S, D, crate::request_builder::WithoutBody> {
    ClientRequestBuilder::<'_, S, D, crate::request_builder::WithoutBody>::new(
      self,
      crate::method::Method::Options,
      url,
    )
  }

  /// Start building a PATCH request
  ///
  /// Returns a request builder that requires a request body.
  pub fn patch(
    &mut self,
    url: impl Into<String>,
  ) -> ClientRequestBuilder<'_, S, D, crate::request_builder::WithBody> {
    ClientRequestBuilder::<'_, S, D, crate::request_builder::WithBody>::new(self, crate::method::Method::Patch, url)
  }

  /// Start building a TRACE request
  ///
  /// Returns a request builder that enforces no request body.
  pub fn trace(
    &mut self,
    url: impl Into<String>,
  ) -> ClientRequestBuilder<'_, S, D, crate::request_builder::WithoutBody> {
    ClientRequestBuilder::<'_, S, D, crate::request_builder::WithoutBody>::new(self, crate::method::Method::Trace, url)
  }

  /// Start building a CONNECT request
  ///
  /// Returns a request builder that enforces no request body.
  pub fn connect(
    &mut self,
    url: impl Into<String>,
  ) -> ClientRequestBuilder<'_, S, D, crate::request_builder::WithoutBody> {
    ClientRequestBuilder::<'_, S, D, crate::request_builder::WithoutBody>::new(
      self,
      crate::method::Method::Connect,
      url,
    )
  }

  /// Get mutable reference to the cookie store (requires cookie-jar feature)
  ///
  /// Allows direct access to the cookie store for custom cookie management.
  #[cfg(feature = "cookie-jar")]
  pub const fn cookie_store_mut(&mut self) -> &mut CookieStore {
    &mut self.cookie_store
  }

  /// Get reference to the cookie store (requires cookie-jar feature)
  ///
  /// Allows read-only access to the cookie store.
  #[cfg(feature = "cookie-jar")]
  pub const fn cookie_store(&self) -> &CookieStore {
    &self.cookie_store
  }

  /// Execute a `Request` object
  ///
  /// # Errors
  /// Returns an error if URL parsing, DNS resolution, socket connection, or HTTP communication fails.
  pub fn run(
    &mut self,
    request: crate::request::Request,
  ) -> Result<Response, Error> {
    let (method, url, headers, body) = request.into_parts();
    self.request(method, &url, &headers, body.map(Body::into_bytes))
  }

  /// Internal request execution with clean orchestration
  ///
  /// This method orchestrates the high-level request flow:
  /// - Redirect loop handling
  /// - Policy validation and decisions
  /// - Delegation to `RequestExecutor` for actual HTTP execution
  ///
  /// # Errors
  /// Returns an error if URL parsing, DNS resolution, socket connection, or HTTP communication fails.
  pub(crate) fn request(
    &mut self,
    method: crate::method::Method,
    url: &str,
    custom_headers: &crate::headers::Headers,
    body: Option<Vec<u8>>,
  ) -> Result<Response, Error> {
    let mut current_url = String::from(url);
    let mut current_method = method;
    let mut current_body = body;

    let mut policy = RequestPolicy::new(&self.config);

    loop {
      // Parse and validate URL
      let uri = Uri::parse(&current_url).map_err(Error::Parse)?;
      policy.validate_protocol(&uri)?;

      // Add cookies to request headers if cookie-jar feature is enabled
      #[cfg(feature = "cookie-jar")]
      let mut headers_with_cookies = custom_headers.clone();
      #[cfg(feature = "cookie-jar")]
      {
        let is_secure = current_url.starts_with("https://");
        let cookie_header = self
          .cookie_store
          .get_request_cookies(&current_url, is_secure);
        if !cookie_header.is_empty() {
          headers_with_cookies.insert(crate::headers::HeaderName::COOKIE, &cookie_header);
        }
      }

      #[cfg(feature = "cookie-jar")]
      let headers_to_use = &headers_with_cookies;
      #[cfg(not(feature = "cookie-jar"))]
      let headers_to_use = custom_headers;

      // Execute single HTTP request
      let mut executor = RequestExecutor::new(&mut self.pool, &self.dns, &self.config);
      let body_slice = current_body.as_deref();
      let raw = executor.execute(&uri, current_method, headers_to_use, body_slice)?;

      // Store cookies from response if cookie-jar feature is enabled
      #[cfg(feature = "cookie-jar")]
      {
        let set_cookie_headers: Vec<String> = raw
          .headers
          .get_all(crate::headers::HeaderName::SET_COOKIE)
          .into_iter()
          .map(alloc::string::ToString::to_string)
          .collect();

        if !set_cookie_headers.is_empty() {
          self
            .cookie_store
            .store_response_cookies(&current_url, &set_cookie_headers);
        }
      }

      // Process response and make policy decision
      match policy.process_raw_response(raw, &uri, &current_url, current_method, current_body)? {
        PolicyDecision::Return(response) => return Ok(response),
        PolicyDecision::Redirect {
          next_uri,
          next_method,
          next_body,
        } => {
          current_url = next_uri;
          current_method = next_method;
          current_body = next_body;
        },
      }
    }
  }
}
