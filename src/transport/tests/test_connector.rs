use crate::config::Config;
use crate::dns::DnsResolver;
use crate::error::{DnsError, Error};
use crate::parser::uri::Uri;
use crate::transport::connection;
use crate::transport::tests::mock_socket::MockSocket;
use crate::util::IpAddr;
use alloc::vec;
use alloc::vec::Vec;
use core::net::Ipv6Addr;
use core::time::Duration;

struct MockDns {
  addresses: Vec<IpAddr>,
}

impl MockDns {
  fn new(addresses: Vec<IpAddr>) -> Self {
    Self { addresses }
  }

  fn empty() -> Self {
    Self { addresses: Vec::new() }
  }
}

impl DnsResolver for MockDns {
  fn resolve(
    &self,
    _hostname: &str,
  ) -> Result<Vec<IpAddr>, DnsError> {
    if self.addresses.is_empty() {
      return Err(DnsError::ResolutionFailed(0));
    }
    Ok(self.addresses.clone())
  }
}

#[test]
fn connector_resolves_dns_and_connects() {
  let mut socket = MockSocket::empty();
  let dns = MockDns::new(vec![IpAddr::from([127, 0, 0, 1])]);

  let uri = Uri::parse("http://example.com").unwrap();
  let result = connection::connect(&mut socket, &dns, &uri, &Config::default(), false);

  assert!(result.is_ok());
  assert!(socket.connected_addr.is_some());
}

#[test]
fn connector_uses_default_http_port_80() {
  let mut socket = MockSocket::empty();
  let dns = MockDns::new(vec![IpAddr::from([127, 0, 0, 1])]);

  let uri = Uri::parse("http://example.com").unwrap();
  let _result = connection::connect(&mut socket, &dns, &uri, &Config::default(), false);

  let addr = socket.connected_addr.unwrap();
  assert!(addr.contains(":80"), "Should use port 80 for HTTP");
}

#[test]
fn connector_uses_default_https_port_443() {
  let mut socket = MockSocket::empty();
  let dns = MockDns::new(vec![IpAddr::from([127, 0, 0, 1])]);

  let uri = Uri::parse("https://example.com").unwrap();
  let _result = connection::connect(&mut socket, &dns, &uri, &Config::default(), false);

  let addr = socket.connected_addr.unwrap();
  assert!(addr.contains(":443"), "Should use port 443 for HTTPS");
}

#[test]
fn connector_uses_explicit_port() {
  let mut socket = MockSocket::empty();
  let dns = MockDns::new(vec![IpAddr::from([127, 0, 0, 1])]);

  let uri = Uri::parse("http://example.com:8080").unwrap();
  let _result = connection::connect(&mut socket, &dns, &uri, &Config::default(), false);

  let addr = socket.connected_addr.unwrap();
  assert!(addr.contains(":8080"), "Should use explicit port 8080");
}

#[test]
fn connector_connects_literal_ipv4() {
  let mut socket = MockSocket::empty();
  let dns = MockDns::new(vec![]); // must not be consulted for literal IP

  let uri = Uri::parse("http://192.168.1.1").unwrap();
  let result = connection::connect(&mut socket, &dns, &uri, &Config::default(), false);

  assert!(result.is_ok());
  assert_eq!(socket.connected_addr.as_deref(), Some("192.168.1.1:80"));
}

#[test]
fn connector_sets_read_timeout() {
  let mut socket = MockSocket::empty();
  let dns = MockDns::new(vec![IpAddr::from([127, 0, 0, 1])]);

  let config = Config {
    timeout_read: Some(Duration::from_secs(5)),
    ..Default::default()
  };

  let uri = Uri::parse("http://example.com").unwrap();
  let _result = connection::connect(&mut socket, &dns, &uri, &config, false);

  assert_eq!(socket.read_timeout, Some(5000));
}

#[test]
fn connector_sets_write_timeout_on_connect() {
  let mut socket = MockSocket::empty();
  let dns = MockDns::new(vec![IpAddr::from([127, 0, 0, 1])]);

  let config = Config {
    timeout_connect: Some(Duration::from_secs(3)),
    ..Default::default()
  };

  let uri = Uri::parse("http://example.com").unwrap();
  let _result = connection::connect(&mut socket, &dns, &uri, &config, false);

  // Connect timeout must not bleed into post-connect writes.
  assert_eq!(socket.write_timeout, Some(0));
}

#[test]
fn connector_sets_read_and_write_timeouts() {
  let mut socket = MockSocket::empty();
  let dns = MockDns::new(vec![IpAddr::from([127, 0, 0, 1])]);

  let config = Config {
    timeout_read: Some(Duration::from_secs(10)),
    timeout_write: Some(Duration::from_secs(10)),
    ..Default::default()
  };

  let uri = Uri::parse("http://example.com").unwrap();
  let _result = connection::connect(&mut socket, &dns, &uri, &config, false);

  assert_eq!(socket.read_timeout, Some(10000));
  assert_eq!(socket.write_timeout, Some(10000));
}

#[test]
fn connector_returns_error_on_dns_failure() {
  let mut socket = MockSocket::empty();
  let dns = MockDns::empty();

  let uri = Uri::parse("http://example.com").unwrap();
  let result = connection::connect(&mut socket, &dns, &uri, &Config::default(), false);

  assert!(result.is_err());
  if let Err(err) = result {
    assert!(matches!(err, Error::Dns(_)));
  }
}

#[test]
fn connector_returns_error_on_socket_connect_failure() {
  let mut socket = MockSocket::with_connect_failure();
  let dns = MockDns::new(vec![IpAddr::from([127, 0, 0, 1])]);

  let uri = Uri::parse("http://example.com").unwrap();
  let result = connection::connect(&mut socket, &dns, &uri, &Config::default(), false);

  assert!(result.is_err());
  if let Err(err) = result {
    assert!(matches!(err, Error::Socket(_)));
  }
}

#[test]
fn connector_returns_error_on_no_addresses() {
  let mut socket = MockSocket::empty();
  let dns = MockDns::new(vec![]);

  let uri = Uri::parse("http://example.com").unwrap();
  let result = connection::connect(&mut socket, &dns, &uri, &Config::default(), false);

  assert!(result.is_err());
}

#[test]
fn connector_uses_first_resolved_address() {
  let mut socket = MockSocket::empty();
  let dns = MockDns::new(vec![IpAddr::from([127, 0, 0, 1]), IpAddr::from([192, 168, 1, 1])]);

  let uri = Uri::parse("http://example.com").unwrap();
  let _result = connection::connect(&mut socket, &dns, &uri, &Config::default(), false);

  let addr = socket.connected_addr.unwrap();
  assert!(addr.contains("127.0.0.1"), "Should use first resolved address");
}

#[test]
fn connector_tries_next_address_on_connect_failure() {
  let mut socket = MockSocket::with_fail_first_n(1);
  let dns = MockDns::new(vec![IpAddr::from([127, 0, 0, 1]), IpAddr::from([192, 168, 1, 1])]);

  let uri = Uri::parse("http://example.com").unwrap();
  let result = connection::connect(&mut socket, &dns, &uri, &Config::default(), false);

  assert!(result.is_ok());
  assert_eq!(socket.connected_addr.as_deref(), Some("192.168.1.1:80"));
}

#[test]
fn connector_creates_connection_with_config() {
  let mut socket = MockSocket::empty();
  let dns = MockDns::new(vec![IpAddr::from([127, 0, 0, 1])]);

  let config = Config {
    max_response_header_size: 16384,
    ..Default::default()
  };

  let uri = Uri::parse("http://example.com").unwrap();
  let result = connection::connect(&mut socket, &dns, &uri, &config, false);

  assert!(result.is_ok());
}

#[test]
fn connector_handles_ipv6_addresses() {
  let mut socket = MockSocket::empty();
  let dns = MockDns::new(vec![IpAddr::V6(Ipv6Addr::LOCALHOST)]);

  let uri = Uri::parse("http://example.com").unwrap();
  let result = connection::connect(&mut socket, &dns, &uri, &Config::default(), false);

  assert!(result.is_ok());
}

#[test]
fn connector_timeout_conversion_handles_large_values() {
  let mut socket = MockSocket::empty();
  let dns = MockDns::new(vec![IpAddr::from([127, 0, 0, 1])]);

  let config = Config {
    timeout_read: Some(Duration::from_secs(100)),
    timeout_write: Some(Duration::from_secs(100)),
    ..Default::default()
  };

  let uri = Uri::parse("http://example.com").unwrap();
  let result = connection::connect(&mut socket, &dns, &uri, &config, false);

  assert!(result.is_ok());
  assert_eq!(socket.read_timeout, Some(100_000));
}

#[test]
fn connector_no_timeouts_when_not_configured() {
  let mut socket = MockSocket::empty();
  let dns = MockDns::new(vec![IpAddr::from([127, 0, 0, 1])]);

  let uri = Uri::parse("http://example.com").unwrap();
  let _result = connection::connect(&mut socket, &dns, &uri, &Config::default(), false);

  assert_eq!(socket.read_timeout, None);
  assert_eq!(socket.write_timeout, None);
}

#[test]
fn connector_borrows_socket_and_dns() {
  let mut socket = MockSocket::empty();
  let dns = MockDns::new(vec![IpAddr::from([127, 0, 0, 1])]);

  {
    let uri = Uri::parse("http://example.com").unwrap();
    let _result = connection::connect(&mut socket, &dns, &uri, &Config::default(), false);
  }

  assert!(socket.connected_addr.is_some());
}

#[test]
fn connector_skips_dns_when_reused() {
  let mut socket = MockSocket::empty();
  let dns = MockDns::empty(); // would fail if consulted

  let uri = Uri::parse("http://example.com").unwrap();
  let result = connection::connect(&mut socket, &dns, &uri, &Config::default(), true);

  assert!(result.is_ok());
  assert!(socket.connected_addr.is_none());
}
