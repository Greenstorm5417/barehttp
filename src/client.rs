use crate::body::Body;
use crate::config::{Config, HttpStatusHandling, ProtocolRestriction, RedirectPolicy};
use crate::dns::DnsResolver;
use crate::error::Error;
use crate::parser::framing::FramingDetector;
use crate::parser::uri::{Host, Uri};
use crate::parser::{BodyReadStrategy, RequestBuilder as ParserRequestBuilder, Response};
use crate::request_builder::ClientRequestBuilder;
use crate::socket::{BlockingSocket, SocketAddr};
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

  /// # Errors
  /// Returns an error if URL parsing, DNS resolution, socket connection, or HTTP communication fails.
  pub(crate) fn request(
    &mut self,
    method: &str,
    url: &str,
    custom_headers: &crate::headers::Headers,
    body: Option<Vec<u8>>,
  ) -> Result<Response, Error> {
    let mut redirect_count = 0u32;
    let mut current_url = alloc::string::String::from(url);
    let mut current_method = method;
    let mut current_body = body;
    let mut visited_urls = Vec::new();

    loop {
      let uri = Uri::parse(&current_url).map_err(Error::Parse)?;

      if self.config.protocol_restriction == ProtocolRestriction::HttpsOnly
        && uri.scheme() != "https"
      {
        return Err(Error::HttpsRequired);
      }

      let authority = uri.authority().ok_or(Error::InvalidUrl)?;
      let host_str = match authority.host() {
        Host::RegName(name) => name,
        Host::IpAddr(_) => return Err(Error::IpAddressNotSupported),
      };
      let port = authority
        .port()
        .unwrap_or_else(|| if uri.scheme() == "https" { 443 } else { 80 });

      let addresses = self.dns.resolve(host_str).map_err(Error::Dns)?;
      let addr = addresses.first().ok_or(Error::NoAddresses)?;

      let socket_addr = SocketAddr::Ip { addr: *addr, port };

      if let Some(timeout_connect) = self.config.timeout_connect {
        let timeout_ms = timeout_connect.as_millis();
        if timeout_ms <= u128::from(u32::MAX) {
          #[allow(clippy::cast_possible_truncation)]
          let timeout_u32 = timeout_ms as u32;
          self
            .socket
            .set_write_timeout(timeout_u32)
            .map_err(Error::Socket)?;
        }
      }

      self.socket.connect(&socket_addr).map_err(Error::Socket)?;

      if let Some(timeout_read) = self.config.timeout_read {
        let timeout_ms = timeout_read.as_millis();
        if timeout_ms <= u128::from(u32::MAX) {
          #[allow(clippy::cast_possible_truncation)]
          let timeout_u32 = timeout_ms as u32;
          self
            .socket
            .set_read_timeout(timeout_u32)
            .map_err(Error::Socket)?;
        }
      } else if let Some(timeout) = self.config.timeout {
        let timeout_ms = timeout.as_millis();
        if timeout_ms <= u128::from(u32::MAX) {
          #[allow(clippy::cast_possible_truncation)]
          let timeout_u32 = timeout_ms as u32;
          self
            .socket
            .set_read_timeout(timeout_u32)
            .map_err(Error::Socket)?;
          self
            .socket
            .set_write_timeout(timeout_u32)
            .map_err(Error::Socket)?;
        }
      }

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

      if let Some(body_data) = current_body.take() {
        builder = builder.body(body_data);
      }

      let request_bytes = builder.build();
      self.socket.write(&request_bytes).map_err(Error::Socket)?;

      // Phase 1: Read response headers using framing detection
      let max_header_size = self.config.max_response_header_size;
      let mut buffer = alloc::vec![0u8; max_header_size.min(8192)];
      let mut total_read = 0usize;
      let mut header_buffer = Vec::new();

      loop {
        let n = self.socket.read(&mut buffer).map_err(Error::Socket)?;
        if n == 0 {
          break;
        }

        if let Some(slice) = buffer.get(..n) {
          header_buffer.extend_from_slice(slice);
        }
        total_read += n;

        if total_read > max_header_size {
          return Err(Error::ResponseHeaderTooLarge);
        }

        if FramingDetector::has_complete_headers(&header_buffer) {
          break;
        }
      }

      // Phase 2: Parse headers to determine body reading strategy
      let (status_code, reason, headers, remaining_after_headers) =
        Response::parse_headers_only(&header_buffer).map_err(Error::Parse)?;

      // HEAD responses never have a body, even if Content-Length is present
      let is_head_request = current_method == "HEAD";
      let body_strategy = if is_head_request {
        BodyReadStrategy::NoBody
      } else {
        Response::body_read_strategy(&headers, status_code)
      };

      // Phase 3: Read body based on strategy
      let response_body = match body_strategy {
        BodyReadStrategy::NoBody => Body::from_bytes(Vec::new()),
        BodyReadStrategy::ContentLength(len) => {
          let mut body_bytes = Vec::from(remaining_after_headers);
          let bytes_needed = len.saturating_sub(body_bytes.len());

          if bytes_needed > 0 {
            let mut read_buffer = alloc::vec![0u8; bytes_needed.min(8192)];
            let mut bytes_read = 0usize;
            let mut consecutive_zero_reads = 0u32;

            while bytes_read < bytes_needed {
              let to_read = (bytes_needed - bytes_read).min(read_buffer.len());
              if let Some(buf_slice) = read_buffer.get_mut(..to_read) {
                let n = self.socket.read(buf_slice).map_err(Error::Socket)?;

                if n == 0 {
                  consecutive_zero_reads += 1;
                  if consecutive_zero_reads >= 3 {
                    break;
                  }
                  continue;
                }

                consecutive_zero_reads = 0;
                if let Some(slice) = read_buffer.get(..n) {
                  body_bytes.extend_from_slice(slice);
                }
                bytes_read += n;
              }
            }
          }

          Response::parse_body_from_bytes(&body_bytes, &headers, status_code)
            .map_err(Error::Parse)?
        }
        BodyReadStrategy::Chunked => {
          let mut body_bytes = Vec::from(remaining_after_headers);
          let mut chunk_buffer = alloc::vec![0u8; 8192];

          loop {
            let n = self.socket.read(&mut chunk_buffer).map_err(Error::Socket)?;
            if n == 0 {
              break;
            }
            if let Some(slice) = chunk_buffer.get(..n) {
              body_bytes.extend_from_slice(slice);
            }

            if FramingDetector::has_chunked_terminator(&body_bytes) {
              break;
            }
          }

          Response::parse_body_from_bytes(&body_bytes, &headers, status_code)
            .map_err(Error::Parse)?
        }
        BodyReadStrategy::UntilClose => {
          let mut body_bytes = Vec::from(remaining_after_headers);
          let mut read_buffer = alloc::vec![0u8; 8192];

          loop {
            let n = self.socket.read(&mut read_buffer).map_err(Error::Socket)?;
            if n == 0 {
              break;
            }
            if let Some(slice) = read_buffer.get(..n) {
              body_bytes.extend_from_slice(slice);
            }
          }

          Response::parse_body_from_bytes(&body_bytes, &headers, status_code)
            .map_err(Error::Parse)?
        }
      };

      // Phase 4: Construct final response
      let response = Response {
        status_code,
        reason,
        headers,
        body: response_body,
      };

      if self.config.http_status_handling == HttpStatusHandling::AsError
        && (response.status_code >= 400 && response.status_code < 600)
      {
        return Err(Error::HttpStatus(response.status_code));
      }

      if self.config.redirect_policy == RedirectPolicy::NoFollow {
        return Ok(response);
      }

      if response.status_code >= 300 && response.status_code < 400 {
        if redirect_count >= self.config.max_redirects {
          if self.config.redirect_policy == RedirectPolicy::Follow {
            return Err(Error::TooManyRedirects);
          }
          return Ok(response);
        }

        let location = response
          .get_header("location")
          .or_else(|| response.get_header("Location"))
          .ok_or(Error::MissingRedirectLocation)?;

        let next_url = uri.resolve_relative(location).map_err(Error::Parse)?;

        if visited_urls
          .iter()
          .any(|u: &alloc::string::String| u.as_str() == next_url.as_str())
        {
          return Err(Error::RedirectLoop);
        }

        visited_urls.push(current_url.clone());
        current_url = next_url;

        if response.status_code == 303
          || (response.status_code == 301 || response.status_code == 302)
            && current_method == "POST"
        {
          current_method = "GET";
          current_body = None;
        }

        redirect_count += 1;
        continue;
      }

      return Ok(response);
    }
  }
}
