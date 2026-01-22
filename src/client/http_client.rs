use crate::body::Body;
use crate::client::policy::{PolicyDecision, RequestPolicy};
use crate::config::Config;
use crate::dns::DnsResolver;
use crate::error::Error;
use crate::parser::uri::Uri;
use crate::parser::{RequestBuilder as ParserRequestBuilder, Response};
use crate::request_builder::ClientRequestBuilder;
use crate::socket::BlockingSocket;
use crate::transport::{Connector, ResponseBodyExpectation};
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
  socket: S,
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
    Ok(Self {
      socket: crate::socket::blocking::OsBlockingSocket::new()?,
      dns: crate::dns::resolver::OsDnsResolver::new(),
      config: Config::default(),
    })
  }

  /// Create a new HTTP client with OS adapters and custom configuration
  ///
  /// Uses the operating system's default socket and DNS resolver.
  ///
  /// # Errors
  /// Returns an error if socket initialization fails.
  pub fn with_config(config: Config) -> Result<Self, Error> {
    Ok(Self {
      socket: crate::socket::blocking::OsBlockingSocket::new()?,
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
  /// - `socket`: Custom socket adapter for network I/O
  /// - `dns`: Custom DNS resolver for hostname resolution
  pub fn new_with_adapters(socket: S, dns: D) -> Self {
    Self {
      socket,
      dns,
      config: Config::default(),
    }
  }

  /// Create a new HTTP client with custom adapters and configuration
  ///
  /// # Parameters
  /// - `socket`: Custom socket adapter for network I/O
  /// - `dns`: Custom DNS resolver for hostname resolution
  /// - `config`: Custom client configuration
  #[allow(clippy::missing_const_for_fn)]
  pub fn with_adapters_and_config(socket: S, dns: D, config: Config) -> Self {
    Self {
      socket,
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
      self, "GET", url,
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
      self, "POST", url,
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
      self, "PUT", url,
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
      self, "DELETE", url,
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
      self, "HEAD", url,
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
      self, "OPTIONS", url,
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
      self, "PATCH", url,
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
      self, "TRACE", url,
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
      self, "CONNECT", url,
    )
  }

  /// Execute a `Request` object
  ///
  /// # Errors
  /// Returns an error if URL parsing, DNS resolution, socket connection, or HTTP communication fails.
  pub fn run(&mut self, request: crate::request::Request) -> Result<Response, Error> {
    let (method, url, headers, body) = request.into_parts();
    self.request(method.as_str(), &url, &headers, body.map(Body::into_bytes))
  }

  /// Internal request execution with thin orchestration
  ///
  /// # Errors
  /// Returns an error if URL parsing, DNS resolution, socket connection, or HTTP communication fails.
  pub(crate) fn request(
    &mut self,
    method: &str,
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

      let connector = Connector::new(&mut self.socket, &self.dns);
      let mut conn = connector.connect(&uri, &self.config)?;

      let authority = uri.authority().ok_or(Error::InvalidUrl)?;
      let host_str = match authority.host() {
        crate::parser::uri::Host::RegName(name) => name,
        crate::parser::uri::Host::IpAddr(_) => return Err(Error::IpAddressNotSupported),
      };

      let mut builder = ParserRequestBuilder::new(current_method, &uri.path_and_query())
        .header("Host", host_str);

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

      let request_bytes = builder.build();
      conn.send_request(&request_bytes)?;

      let expectation = if current_method == "HEAD" {
        ResponseBodyExpectation::NoBody
      } else {
        ResponseBodyExpectation::Normal
      };
      let raw = conn.read_raw_response(expectation)?;

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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]
mod tests {
  use super::*;
  use crate::error::SocketError;
  use crate::util::IpAddr;
  use alloc::string::ToString;
  use alloc::vec;

  struct MockSocket {
    read_data: Vec<u8>,
    read_pos: usize,
    written: Vec<u8>,
  }

  impl MockSocket {
    fn new(response: &str) -> Self {
      Self {
        read_data: response.as_bytes().to_vec(),
        read_pos: 0,
        written: Vec::new(),
      }
    }

    fn get_written(&self) -> String {
      String::from_utf8_lossy(&self.written).to_string()
    }
  }

  impl crate::socket::BlockingSocket for MockSocket {
    fn connect(
      &mut self,
      _addr: &crate::socket::SocketAddr<'_>,
    ) -> Result<(), SocketError> {
      Ok(())
    }

    fn read(&mut self, buf: &mut [u8]) -> Result<usize, SocketError> {
      if self.read_pos >= self.read_data.len() {
        return Ok(0);
      }
      let remaining = &self.read_data[self.read_pos..];
      let to_read = remaining.len().min(buf.len());
      buf[..to_read].copy_from_slice(&remaining[..to_read]);
      self.read_pos += to_read;
      Ok(to_read)
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize, SocketError> {
      self.written.extend_from_slice(buf);
      Ok(buf.len())
    }

    fn shutdown(&mut self) -> Result<(), SocketError> {
      Ok(())
    }

    fn set_flags(
      &mut self,
      _flags: crate::socket::SocketFlags,
    ) -> Result<(), SocketError> {
      Ok(())
    }

    fn set_read_timeout(&mut self, _timeout_ms: u32) -> Result<(), SocketError> {
      Ok(())
    }

    fn set_write_timeout(&mut self, _timeout_ms: u32) -> Result<(), SocketError> {
      Ok(())
    }
  }

  struct MockDns;

  impl crate::dns::DnsResolver for MockDns {
    fn resolve(&self, _hostname: &str) -> Result<Vec<IpAddr>, crate::error::DnsError> {
      Ok(vec![IpAddr::V4([127, 0, 0, 1])])
    }
  }

  #[test]
  fn head_request_passes_no_body_expectation() {
    let socket = MockSocket::new("HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\n");
    let dns = MockDns;
    let mut client = HttpClient::with_adapters_and_config(socket, dns, Config::default());

    let result = client.head("http://example.com").call();

    assert!(result.is_ok());
    let resp = result.unwrap();
    assert_eq!(resp.status_code, 200);
    assert!(
      resp.body.as_bytes().is_empty(),
      "HEAD response body should be empty"
    );
  }

  #[test]
  fn custom_headers_are_forwarded() {
    let socket = MockSocket::new("HTTP/1.1 200 OK\r\n\r\n");
    let dns = MockDns;
    let mut client = HttpClient::with_adapters_and_config(socket, dns, Config::default());

    let _result = client
      .get("http://example.com")
      .header("X-Test", "123")
      .header("X-Custom", "value")
      .call();

    let written = client.socket.get_written();
    assert!(
      written.contains("X-Test: 123"),
      "Custom header X-Test not found in request"
    );
    assert!(
      written.contains("X-Custom: value"),
      "Custom header X-Custom not found in request"
    );
  }

  #[test]
  fn user_agent_from_config_is_applied() {
    let socket = MockSocket::new("HTTP/1.1 200 OK\r\n\r\n");
    let dns = MockDns;
    let config = Config {
      user_agent: Some(String::from("TestAgent/1.0")),
      ..Default::default()
    };
    let mut client = HttpClient::with_adapters_and_config(socket, dns, config);

    let _result = client.get("http://example.com").call();

    let written = client.socket.get_written();
    assert!(
      written.contains("User-Agent: TestAgent/1.0"),
      "User-Agent header not applied from config"
    );
  }

  #[test]
  fn accept_header_from_config_is_applied() {
    let socket = MockSocket::new("HTTP/1.1 200 OK\r\n\r\n");
    let dns = MockDns;
    let config = Config {
      accept: Some(String::from("application/json")),
      ..Default::default()
    };
    let mut client = HttpClient::with_adapters_and_config(socket, dns, config);

    let _result = client.get("http://example.com").call();

    let written = client.socket.get_written();
    assert!(
      written.contains("Accept: application/json"),
      "Accept header not applied from config"
    );
  }

  #[test]
  fn ip_address_url_is_rejected() {
    let socket = MockSocket::new("HTTP/1.1 200 OK\r\n\r\n");
    let dns = MockDns;
    let mut client = HttpClient::with_adapters_and_config(socket, dns, Config::default());

    let err = client.get("http://127.0.0.1").call().unwrap_err();
    assert!(
      matches!(err, Error::IpAddressNotSupported),
      "Should reject IP address URLs"
    );
  }

  #[test]
  fn client_respects_no_follow_policy() {
    let socket = MockSocket::new("HTTP/1.1 302 Found\r\nLocation: /next\r\n\r\n");
    let dns = MockDns;
    let config = Config {
      redirect_policy: crate::config::RedirectPolicy::NoFollow,
      ..Default::default()
    };
    let mut client = HttpClient::with_adapters_and_config(socket, dns, config);

    let result = client.get("http://example.com").call();

    assert!(result.is_ok());
    let resp = result.unwrap();
    assert_eq!(
      resp.status_code, 302,
      "Should return redirect response without following"
    );
  }

  #[test]
  fn host_header_is_added() {
    let socket = MockSocket::new("HTTP/1.1 200 OK\r\n\r\n");
    let dns = MockDns;
    let mut client = HttpClient::with_adapters_and_config(socket, dns, Config::default());

    let _result = client.get("http://example.com").call();

    let written = client.socket.get_written();
    assert!(
      written.contains("Host: example.com"),
      "Host header should be added automatically"
    );
  }

  #[test]
  fn request_method_is_correct() {
    let socket = MockSocket::new("HTTP/1.1 200 OK\r\n\r\n");
    let dns = MockDns;
    let mut client = HttpClient::with_adapters_and_config(socket, dns, Config::default());

    let _result = client.get("http://example.com/path").call();

    let written = client.socket.get_written();
    assert!(
      written.starts_with("GET /path"),
      "Request should start with GET method and path"
    );
  }

  #[test]
  fn https_only_enforcement() {
    let socket = MockSocket::new("HTTP/1.1 200 OK\r\n\r\n");
    let dns = MockDns;
    let config = Config {
      protocol_restriction: crate::config::ProtocolRestriction::HttpsOnly,
      ..Default::default()
    };
    let mut client = HttpClient::with_adapters_and_config(socket, dns, config);

    let err = client.get("http://example.com").call().unwrap_err();
    assert!(
      matches!(err, Error::HttpsRequired),
      "Should enforce HTTPS-only when configured"
    );
  }
}
