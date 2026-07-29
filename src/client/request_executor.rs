//! One HTTP hop: pool/connect, send, read, maybe return socket to pool.

use crate::config::Config;
use crate::dns::DnsResolver;
use crate::error::Error;
use crate::headers::Headers;
use crate::method::Method;
use crate::parser::WireRequest;
use crate::parser::uri::Uri;
use crate::socket::BlockingSocket;
use crate::transport::{ConnectionPool, Connector, PoolKey, RawResponse, ResponseBodyExpectation};
use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

/// Execute a single request (no redirects).
pub fn execute<S, D>(
  pool: &Arc<ConnectionPool<S>>,
  dns: &D,
  config: &Config,
  uri: &Uri,
  method: Method,
  custom_headers: &Headers,
  body: Option<&[u8]>,
) -> Result<RawResponse, Error>
where
  S: BlockingSocket,
  D: DnsResolver,
{
  let host_str = host_from_uri(uri);
  let port = port_from_uri(uri);
  let pool_key = PoolKey::new(uri.scheme().to_ascii_lowercase(), host_str.clone(), port);

  let mut socket = get_or_create_socket(pool, config, &pool_key)?;
  let connector = Connector::new(&mut socket, dns);
  let mut conn = connector.connect(uri, config)?;

  let request_bytes = build_request(uri, method, &host_str, port, custom_headers, body, config)?;
  conn.send_request(&request_bytes)?;

  let expectation = if method == Method::Head {
    ResponseBodyExpectation::NoBody
  } else {
    ResponseBodyExpectation::Normal
  };
  let raw = conn.read_raw_response(expectation)?;

  if config.connection_pooling && conn.is_reusable() {
    pool.return_connection(pool_key, socket);
  }

  Ok(raw)
}

fn host_from_uri(uri: &Uri) -> String {
  let Some(auth) = uri.authority() else {
    return String::new();
  };
  match auth.host() {
    crate::parser::uri::Host::RegName(name) => String::from(*name),
    crate::parser::uri::Host::IpAddr(addr) => alloc::format!("{addr}"),
  }
}

fn port_from_uri(uri: &Uri) -> u16 {
  uri
    .authority()
    .and_then(crate::parser::uri::Authority::port)
    .unwrap_or_else(|| {
      if uri.scheme().eq_ignore_ascii_case("https") {
        443
      } else {
        80
      }
    })
}

fn get_or_create_socket<S>(
  pool: &Arc<ConnectionPool<S>>,
  config: &Config,
  pool_key: &PoolKey,
) -> Result<S, Error>
where
  S: BlockingSocket,
{
  if config.connection_pooling
    && let Some(s) = pool.get(pool_key)
  {
    return Ok(s);
  }
  S::new().map_err(Error::Socket)
}

fn build_request(
  uri: &Uri,
  method: Method,
  host_str: &str,
  port: u16,
  custom_headers: &Headers,
  body: Option<&[u8]>,
  config: &Config,
) -> Result<Vec<u8>, Error> {
  if uri
    .authority()
    .and_then(crate::parser::uri::Authority::userinfo)
    .is_some()
  {
    // Credentials in the URL are not sent as Authorization (avoid silent drop).
    return Err(Error::Parse(crate::error::ParseError::InvalidUri));
  }

  let host_header = if (uri.scheme().eq_ignore_ascii_case("http") && port == 80)
    || (uri.scheme().eq_ignore_ascii_case("https") && port == 443)
  {
    String::from(host_str)
  } else {
    format!("{host_str}:{port}")
  };

  let mut builder =
    WireRequest::new(method.as_str(), &uri.path_and_query()).header(Headers::HOST, host_header.as_str());

  if !config.connection_pooling {
    builder = builder.header(Headers::CONNECTION, "close");
  }

  if let Some(ref user_agent) = config.user_agent {
    builder = builder.header(Headers::USER_AGENT, user_agent.as_str());
  }

  if let Some(ref accept) = config.accept
    && !custom_headers.contains(Headers::ACCEPT)
  {
    builder = builder.header(Headers::ACCEPT, accept.as_str());
  }

  if !custom_headers.contains(Headers::ACCEPT_ENCODING) {
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
      builder = builder.header(Headers::ACCEPT_ENCODING, accept_encoding.as_str());
    }
  }

  for (name, value) in custom_headers {
    builder = builder.header(name.as_str(), value.as_str());
  }

  if let Some(body_data) = body {
    builder = builder.body(body_data.to_vec());
  }

  builder.build().map_err(Error::Parse)
}
