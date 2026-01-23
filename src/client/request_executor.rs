/// Single HTTP request execution logic
///
/// This module handles the low-level details of executing a single HTTP request:
/// - Socket management (pooling/creation)
/// - Connection establishment
/// - Request serialization
/// - Response reading
/// - Connection reuse logic
use crate::config::Config;
use crate::dns::DnsResolver;
use crate::error::Error;
use crate::headers::{HeaderName, Headers};
use crate::method::Method;
use crate::parser::RequestBuilder as ParserRequestBuilder;
use crate::parser::uri::Uri;
use crate::socket::BlockingSocket;
use crate::transport::{ConnectionPool, Connector, PoolKey, RawResponse, ResponseBodyExpectation};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

/// Executes a single HTTP request without redirect handling
pub struct RequestExecutor<'a, S, D> {
  pool: &'a Arc<ConnectionPool<S>>,
  dns: &'a D,
  config: &'a Config,
}

impl<'a, S, D> RequestExecutor<'a, S, D>
where
  S: BlockingSocket,
  D: DnsResolver,
{
  pub const fn new(
    pool: &'a Arc<ConnectionPool<S>>,
    dns: &'a D,
    config: &'a Config,
  ) -> Self {
    Self { pool, dns, config }
  }

  /// Execute a single HTTP request and return raw response
  pub fn execute(
    &mut self,
    uri: &Uri,
    method: Method,
    custom_headers: &Headers,
    body: Option<&[u8]>,
  ) -> Result<RawResponse, Error> {
    // Extract host information from URI (copy to avoid lifetime issues)
    let host_str = Self::extract_host_from_uri(uri)?;
    let port = Self::extract_port_from_uri(uri);
    let pool_key = PoolKey::new(host_str.clone(), port);

    // Get or create socket
    let mut socket = self.get_or_create_socket(&pool_key)?;

    // Establish connection
    let connector = Connector::new(&mut socket, self.dns);
    let mut conn = connector.connect(uri, self.config)?;

    // Build and send request
    let request_bytes = self.build_request(uri, method, &host_str, port, custom_headers, body)?;
    conn.send_request(&request_bytes)?;

    // Read response
    let expectation = if method == Method::Head {
      ResponseBodyExpectation::NoBody
    } else {
      ResponseBodyExpectation::Normal
    };
    let raw = conn.read_raw_response(expectation)?;

    // Handle connection pooling
    self.handle_connection_reuse(conn.is_reusable(), pool_key, socket);

    Ok(raw)
  }

  /// Extract hostname from URI
  fn extract_host_from_uri(uri: &Uri) -> Result<String, Error> {
    let authority = uri.authority();
    authority.map_or_else(
      || Ok(String::new()),
      |auth| match auth.host() {
        crate::parser::uri::Host::RegName(name) => Ok(String::from(*name)),
        crate::parser::uri::Host::IpAddr(_) => Err(Error::IpAddressNotSupported),
      },
    )
  }

  /// Extract port from URI with defaults
  fn extract_port_from_uri(uri: &Uri) -> u16 {
    uri
      .authority()
      .and_then(super::super::parser::uri::Authority::port)
      .unwrap_or_else(|| {
        if uri.scheme() == "https" {
          443
        } else {
          80
        }
      })
  }

  /// Get socket from pool or create new one
  fn get_or_create_socket(
    &self,
    pool_key: &PoolKey,
  ) -> Result<S, Error> {
    if self.config.connection_pooling {
      self
        .pool
        .get(pool_key)
        .map_or_else(|| S::new().map_err(Error::Socket), |s| Ok(s))
    } else {
      S::new().map_err(Error::Socket)
    }
  }

  /// Build HTTP request bytes
  fn build_request(
    &self,
    uri: &Uri,
    method: Method,
    host_str: &str,
    port: u16,
    custom_headers: &Headers,
    body: Option<&[u8]>,
  ) -> Result<Vec<u8>, Error> {
    use alloc::format;

    // Build Host header with port if non-default
    let host_header = if (uri.scheme() == "http" && port == 80) || (uri.scheme() == "https" && port == 443) {
      String::from(host_str)
    } else {
      format!("{host_str}:{port}")
    };

    let mut builder =
      ParserRequestBuilder::new(method.as_str(), &uri.path_and_query()).header(HeaderName::HOST, host_header.as_str());

    // RFC 9112 Section 9.3: Send Connection: close if pooling is disabled
    if !self.config.connection_pooling {
      builder = builder.header(HeaderName::CONNECTION, "close");
    }

    // Add default headers from config
    if let Some(ref user_agent) = self.config.user_agent {
      builder = builder.header(HeaderName::USER_AGENT, user_agent.as_str());
    }

    // Only add default Accept if user hasn't specified it in custom headers
    if let Some(ref accept) = self.config.accept
      && !custom_headers.contains(HeaderName::ACCEPT)
    {
      builder = builder.header(HeaderName::ACCEPT, accept.as_str());
    }

    // Add Accept-Encoding header based on enabled decompression features
    // Only add if user hasn't specified it in custom headers
    if !custom_headers.contains(HeaderName::ACCEPT_ENCODING) {
      #[allow(unused_mut)]
      let mut encodings: Vec<&str> = Vec::new();

      #[cfg(feature = "gzip-decompression")]
      {
        encodings.push("gzip");
        encodings.push("deflate");
      }

      #[cfg(feature = "zstd-decompression")]
      encodings.push("zstd");

      if !encodings.is_empty() {
        let accept_encoding = encodings.join(", ");
        builder = builder.header(HeaderName::ACCEPT_ENCODING, accept_encoding.as_str());
      }
    }

    // Add custom headers
    for (name, value) in custom_headers {
      builder = builder.header(name.as_str(), value.as_str());
    }

    // Add body if present
    if let Some(body_data) = body {
      builder = builder.body(body_data.to_vec());
    }

    builder.build().map_err(Error::Parse)
  }

  /// Handle connection reuse based on pooling config
  fn handle_connection_reuse(
    &self,
    is_reusable: bool,
    pool_key: PoolKey,
    socket: S,
  ) {
    if self.config.connection_pooling && is_reusable {
      self.pool.return_connection(pool_key, socket);
    }
  }
}
