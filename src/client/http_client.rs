use crate::body::Body;
use crate::client::policy::{PolicyDecision, RequestPolicy};
use crate::config::Config;
use crate::dns::DnsResolver;
use crate::error::Error;
use crate::parser::uri::Uri;
use crate::parser::{RequestBuilder as ParserRequestBuilder, Response};
use crate::request_builder::ClientRequestBuilder;
use crate::socket::BlockingSocket;
use crate::transport::{ConnectionPool, Connector, PoolKey, ResponseBodyExpectation};
use alloc::string::String;
use alloc::vec::Vec;

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
}

impl
  HttpClient<
    crate::socket::blocking::OsBlockingSocket,
    crate::dns::resolver::OsDnsResolver,
  >
{
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
    }
  }

  /// Create a new HTTP client with custom adapters and configuration
  ///
  /// # Parameters
  /// - `dns`: Custom DNS resolver for hostname resolution
  /// - `config`: Custom client configuration
  #[allow(clippy::missing_const_for_fn)]
  pub fn with_adapters_and_config(dns: D, config: Config) -> Self {
    Self {
      pool: ConnectionPool::new(config.max_idle_per_host, config.idle_timeout),
      dns,
      config,
    }
  }

  /// TODO: Per-request config should overlay, not mutate client state
  /// This is temporary until we implement proper config scoping
  pub(crate) fn apply_request_config(&mut self, config: Config) {
    self.config = config;
  }

  /// Start building a GET request
  ///
  /// Returns a request builder that enforces no request body at compile time.
  pub fn get(
    &mut self,
    url: impl Into<String>,
  ) -> ClientRequestBuilder<'_, S, D, crate::request_builder::WithoutBody> {
    ClientRequestBuilder::<'_, S, D, crate::request_builder::WithoutBody>::new(
      self,
      crate::method::Method::Get,
      url,
    )
  }

  /// Start building a POST request
  ///
  /// Returns a request builder that requires a request body.
  pub fn post(
    &mut self,
    url: impl Into<String>,
  ) -> ClientRequestBuilder<'_, S, D, crate::request_builder::WithBody> {
    ClientRequestBuilder::<'_, S, D, crate::request_builder::WithBody>::new(
      self,
      crate::method::Method::Post,
      url,
    )
  }

  /// Start building a PUT request
  ///
  /// Returns a request builder that requires a request body.
  pub fn put(
    &mut self,
    url: impl Into<String>,
  ) -> ClientRequestBuilder<'_, S, D, crate::request_builder::WithBody> {
    ClientRequestBuilder::<'_, S, D, crate::request_builder::WithBody>::new(
      self,
      crate::method::Method::Put,
      url,
    )
  }

  /// Start building a DELETE request
  ///
  /// Returns a request builder with no body by default (use `force_send_body()` if needed).
  pub fn delete(
    &mut self,
    url: impl Into<String>,
  ) -> ClientRequestBuilder<'_, S, D, crate::request_builder::WithoutBody> {
    ClientRequestBuilder::<'_, S, D, crate::request_builder::WithoutBody>::new(
      self,
      crate::method::Method::Delete,
      url,
    )
  }

  /// Start building a HEAD request
  ///
  /// Returns a request builder that enforces no request body.
  pub fn head(
    &mut self,
    url: impl Into<String>,
  ) -> ClientRequestBuilder<'_, S, D, crate::request_builder::WithoutBody> {
    ClientRequestBuilder::<'_, S, D, crate::request_builder::WithoutBody>::new(
      self,
      crate::method::Method::Head,
      url,
    )
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
    ClientRequestBuilder::<'_, S, D, crate::request_builder::WithBody>::new(
      self,
      crate::method::Method::Patch,
      url,
    )
  }

  /// Start building a TRACE request
  ///
  /// Returns a request builder that enforces no request body.
  pub fn trace(
    &mut self,
    url: impl Into<String>,
  ) -> ClientRequestBuilder<'_, S, D, crate::request_builder::WithoutBody> {
    ClientRequestBuilder::<'_, S, D, crate::request_builder::WithoutBody>::new(
      self,
      crate::method::Method::Trace,
      url,
    )
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

  /// Execute a `Request` object
  ///
  /// # Errors
  /// Returns an error if URL parsing, DNS resolution, socket connection, or HTTP communication fails.
  pub fn run(&mut self, request: crate::request::Request) -> Result<Response, Error> {
    let (method, url, headers, body) = request.into_parts();
    self.request(method, &url, &headers, body.map(Body::into_bytes))
  }

  /// Internal request execution with thin orchestration
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
      let uri = Uri::parse(&current_url).map_err(Error::Parse)?;
      policy.validate_protocol(&uri)?;

      // RFC 9112 Section 3.2: Host header handling
      let authority = uri.authority();
      let host_str = if let Some(auth) = authority {
        match auth.host() {
          crate::parser::uri::Host::RegName(name) => name,
          crate::parser::uri::Host::IpAddr(_) => {
            return Err(Error::IpAddressNotSupported);
          }
        }
      } else {
        // RFC 9112 Section 3.2: If authority missing, send empty Host
        ""
      };

      let port = authority
        .and_then(super::super::parser::uri::Authority::port)
        .unwrap_or_else(|| if uri.scheme() == "https" { 443 } else { 80 });

      let pool_key = PoolKey::new(String::from(host_str), port);

      // Get socket from pool or create new
      let mut socket = if self.config.connection_pooling {
        match self.pool.get(&pool_key) {
          Some(s) => s,
          None => S::new().map_err(Error::Socket)?,
        }
      } else {
        S::new().map_err(Error::Socket)?
      };

      let connector = Connector::new(&mut socket, &self.dns);
      let mut conn = connector.connect(&uri, &self.config)?;

      let mut builder =
        ParserRequestBuilder::new(current_method.as_str(), &uri.path_and_query())
          .header("Host", host_str);

      // RFC 9112 Section 9.3: Send Connection: close if pooling is disabled
      if !self.config.connection_pooling {
        builder = builder.header("Connection", "close");
      }

      if let Some(ref user_agent) = self.config.user_agent {
        builder = builder.header("User-Agent", user_agent.as_str());
      }

      if let Some(ref accept) = self.config.accept {
        builder = builder.header("Accept", accept.as_str());
      }

      for (name, value) in custom_headers {
        builder = builder.header(name.as_str(), value.as_str());
      }

      if let Some(ref body_data) = current_body {
        builder = builder.body(body_data.clone());
      }

      let request_bytes = builder.build()?;
      conn.send_request(&request_bytes)?;

      let expectation = if current_method == crate::method::Method::Head {
        ResponseBodyExpectation::NoBody
      } else {
        ResponseBodyExpectation::Normal
      };
      let raw = conn.read_raw_response(expectation)?;

      // Check if connection can be reused
      let is_reusable = conn.is_reusable();

      // Return socket to pool if pooling is enabled and connection is reusable
      if self.config.connection_pooling && is_reusable {
        self.pool.return_connection(pool_key, socket);
      }

      match policy.process_raw_response(
        raw,
        &uri,
        &current_url,
        current_method,
        current_body,
      )? {
        PolicyDecision::Return(response) => return Ok(response),
        PolicyDecision::Redirect {
          next_uri,
          next_method,
          next_body,
        } => {
          current_url = next_uri;
          current_method = next_method;
          current_body = next_body;
        }
      }
    }
  }
}
