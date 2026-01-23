use crate::config::Config;
use crate::dns::DnsResolver;
use crate::error::{DnsError, Error, SocketError};
use crate::parser::uri::Uri;
use crate::socket::{BlockingSocket, SocketAddr, SocketFlags};
use crate::transport::connector::Connector;
use crate::util::IpAddr;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::time::Duration;

struct MockSocket {
  connected_addr: Option<String>,
  read_timeout: Option<u32>,
  write_timeout: Option<u32>,
  should_fail_connect: bool,
}

impl MockSocket {
  fn new() -> Self {
    Self {
      connected_addr: None,
      read_timeout: None,
      write_timeout: None,
      should_fail_connect: false,
    }
  }

  fn with_connect_failure() -> Self {
    Self {
      connected_addr: None,
      read_timeout: None,
      write_timeout: None,
      should_fail_connect: true,
    }
  }
}

impl BlockingSocket for MockSocket {
  fn new() -> Result<Self, SocketError> {
    Ok(Self {
      connected_addr: None,
      read_timeout: None,
      write_timeout: None,
      should_fail_connect: false,
    })
  }

  fn connect(&mut self, addr: &SocketAddr<'_>) -> Result<(), SocketError> {
    if self.should_fail_connect {
      return Err(SocketError::NotConnected);
    }
    match addr {
      SocketAddr::Ip {
        addr: ip_addr,
        port,
      } => {
        self.connected_addr = Some(format!("{ip_addr:?}:{port}"));
      }
      SocketAddr::Hostname { host, port } => {
        let host_str = core::str::from_utf8(host).unwrap_or("invalid");
        self.connected_addr = Some(format!("{host_str}:{port}"));
      }
    }
    Ok(())
  }

  fn read(&mut self, _buf: &mut [u8]) -> Result<usize, SocketError> {
    Ok(0)
  }

  fn write(&mut self, _buf: &[u8]) -> Result<usize, SocketError> {
    Ok(0)
  }

  fn shutdown(&mut self) -> Result<(), SocketError> {
    Ok(())
  }

  fn set_flags(&mut self, _flags: SocketFlags) -> Result<(), SocketError> {
    Ok(())
  }

  fn set_read_timeout(&mut self, timeout_ms: u32) -> Result<(), SocketError> {
    self.read_timeout = Some(timeout_ms);
    Ok(())
  }

  fn set_write_timeout(&mut self, timeout_ms: u32) -> Result<(), SocketError> {
    self.write_timeout = Some(timeout_ms);
    Ok(())
  }
}

struct MockDns {
  addresses: Vec<IpAddr>,
}

impl MockDns {
  fn new(addresses: Vec<IpAddr>) -> Self {
    Self { addresses }
  }

  fn empty() -> Self {
    Self {
      addresses: Vec::new(),
    }
  }
}

impl DnsResolver for MockDns {
  fn resolve(&self, _hostname: &str) -> Result<Vec<IpAddr>, DnsError> {
    if self.addresses.is_empty() {
      return Err(DnsError::ResolutionFailed(0));
    }
    Ok(self.addresses.clone())
  }
}

#[test]
fn connector_resolves_dns_and_connects() {
  let mut socket = MockSocket::new();
  let dns = MockDns::new(vec![IpAddr::V4([127, 0, 0, 1])]);
  let connector = Connector::new(&mut socket, &dns);

  let uri = Uri::parse("http://example.com").unwrap();
  let result = connector.connect(&uri, &Config::default());

  assert!(result.is_ok());
  assert!(socket.connected_addr.is_some());
}

#[test]
fn connector_uses_default_http_port_80() {
  let mut socket = MockSocket::new();
  let dns = MockDns::new(vec![IpAddr::V4([127, 0, 0, 1])]);
  let connector = Connector::new(&mut socket, &dns);

  let uri = Uri::parse("http://example.com").unwrap();
  let _result = connector.connect(&uri, &Config::default());

  let addr = socket.connected_addr.unwrap();
  assert!(addr.contains(":80"), "Should use port 80 for HTTP");
}

#[test]
fn connector_uses_default_https_port_443() {
  let mut socket = MockSocket::new();
  let dns = MockDns::new(vec![IpAddr::V4([127, 0, 0, 1])]);
  let connector = Connector::new(&mut socket, &dns);

  let uri = Uri::parse("https://example.com").unwrap();
  let _result = connector.connect(&uri, &Config::default());

  let addr = socket.connected_addr.unwrap();
  assert!(addr.contains(":443"), "Should use port 443 for HTTPS");
}

#[test]
fn connector_uses_explicit_port() {
  let mut socket = MockSocket::new();
  let dns = MockDns::new(vec![IpAddr::V4([127, 0, 0, 1])]);
  let connector = Connector::new(&mut socket, &dns);

  let uri = Uri::parse("http://example.com:8080").unwrap();
  let _result = connector.connect(&uri, &Config::default());

  let addr = socket.connected_addr.unwrap();
  assert!(addr.contains(":8080"), "Should use explicit port 8080");
}

#[test]
fn connector_rejects_ip_address_hosts() {
  let mut socket = MockSocket::new();
  let dns = MockDns::new(vec![IpAddr::V4([127, 0, 0, 1])]);
  let connector = Connector::new(&mut socket, &dns);

  let uri = Uri::parse("http://192.168.1.1").unwrap();
  let result = connector.connect(&uri, &Config::default());

  assert!(result.is_err());
  if let Err(err) = result {
    assert!(matches!(err, Error::IpAddressNotSupported));
  }
}

#[test]
fn connector_sets_read_timeout() {
  let mut socket = MockSocket::new();
  let dns = MockDns::new(vec![IpAddr::V4([127, 0, 0, 1])]);
  let connector = Connector::new(&mut socket, &dns);

  let config = Config {
    timeout_read: Some(Duration::from_millis(5000)),
    ..Default::default()
  };

  let uri = Uri::parse("http://example.com").unwrap();
  let _result = connector.connect(&uri, &config);

  assert_eq!(socket.read_timeout, Some(5000));
}

#[test]
fn connector_sets_write_timeout_on_connect() {
  let mut socket = MockSocket::new();
  let dns = MockDns::new(vec![IpAddr::V4([127, 0, 0, 1])]);
  let connector = Connector::new(&mut socket, &dns);

  let config = Config {
    timeout_connect: Some(Duration::from_millis(3000)),
    ..Default::default()
  };

  let uri = Uri::parse("http://example.com").unwrap();
  let _result = connector.connect(&uri, &config);

  assert_eq!(socket.write_timeout, Some(3000));
}

#[test]
fn connector_sets_both_timeouts_from_general_timeout() {
  let mut socket = MockSocket::new();
  let dns = MockDns::new(vec![IpAddr::V4([127, 0, 0, 1])]);
  let connector = Connector::new(&mut socket, &dns);

  let config = Config {
    timeout: Some(Duration::from_millis(10000)),
    ..Default::default()
  };

  let uri = Uri::parse("http://example.com").unwrap();
  let _result = connector.connect(&uri, &config);

  assert_eq!(socket.read_timeout, Some(10000));
  assert_eq!(socket.write_timeout, Some(10000));
}

#[test]
fn connector_prioritizes_specific_timeout_over_general() {
  let mut socket = MockSocket::new();
  let dns = MockDns::new(vec![IpAddr::V4([127, 0, 0, 1])]);
  let connector = Connector::new(&mut socket, &dns);

  let config = Config {
    timeout: Some(Duration::from_millis(10000)),
    timeout_read: Some(Duration::from_millis(5000)),
    ..Default::default()
  };

  let uri = Uri::parse("http://example.com").unwrap();
  let _result = connector.connect(&uri, &config);

  assert_eq!(
    socket.read_timeout,
    Some(5000),
    "Specific read timeout should override general timeout"
  );
}

#[test]
fn connector_returns_error_on_dns_failure() {
  let mut socket = MockSocket::new();
  let dns = MockDns::empty();
  let connector = Connector::new(&mut socket, &dns);

  let uri = Uri::parse("http://example.com").unwrap();
  let result = connector.connect(&uri, &Config::default());

  assert!(result.is_err());
  if let Err(err) = result {
    assert!(matches!(err, Error::Dns(_)));
  }
}

#[test]
fn connector_returns_error_on_socket_connect_failure() {
  let mut socket = MockSocket::with_connect_failure();
  let dns = MockDns::new(vec![IpAddr::V4([127, 0, 0, 1])]);
  let connector = Connector::new(&mut socket, &dns);

  let uri = Uri::parse("http://example.com").unwrap();
  let result = connector.connect(&uri, &Config::default());

  assert!(result.is_err());
  if let Err(err) = result {
    assert!(matches!(err, Error::Socket(_)));
  }
}

#[test]
fn connector_returns_error_on_no_addresses() {
  let mut socket = MockSocket::new();
  let dns = MockDns::new(vec![]);
  let connector = Connector::new(&mut socket, &dns);

  let uri = Uri::parse("http://example.com").unwrap();
  let result = connector.connect(&uri, &Config::default());

  assert!(result.is_err());
}

#[test]
fn connector_uses_first_resolved_address() {
  let mut socket = MockSocket::new();
  let dns = MockDns::new(vec![
    IpAddr::V4([127, 0, 0, 1]),
    IpAddr::V4([192, 168, 1, 1]),
  ]);
  let connector = Connector::new(&mut socket, &dns);

  let uri = Uri::parse("http://example.com").unwrap();
  let _result = connector.connect(&uri, &Config::default());

  let addr = socket.connected_addr.unwrap();
  assert!(
    addr.contains("V4([127, 0, 0, 1])"),
    "Should use first resolved address"
  );
}

#[test]
fn connector_creates_connection_with_config() {
  let mut socket = MockSocket::new();
  let dns = MockDns::new(vec![IpAddr::V4([127, 0, 0, 1])]);
  let connector = Connector::new(&mut socket, &dns);

  let config = Config {
    max_response_header_size: 16384,
    ..Default::default()
  };

  let uri = Uri::parse("http://example.com").unwrap();
  let result = connector.connect(&uri, &config);

  assert!(result.is_ok());
}

#[test]
fn connector_handles_ipv6_addresses() {
  let mut socket = MockSocket::new();
  let dns = MockDns::new(vec![IpAddr::V6([0, 0, 0, 0, 0, 0, 0, 1])]);
  let connector = Connector::new(&mut socket, &dns);

  let uri = Uri::parse("http://example.com").unwrap();
  let result = connector.connect(&uri, &Config::default());

  assert!(result.is_ok());
}

#[test]
fn connector_timeout_conversion_handles_large_values() {
  let mut socket = MockSocket::new();
  let dns = MockDns::new(vec![IpAddr::V4([127, 0, 0, 1])]);
  let connector = Connector::new(&mut socket, &dns);

  let config = Config {
    timeout: Some(Duration::from_secs(100)),
    ..Default::default()
  };

  let uri = Uri::parse("http://example.com").unwrap();
  let result = connector.connect(&uri, &config);

  assert!(result.is_ok());
  assert_eq!(socket.read_timeout, Some(100_000));
}

#[test]
fn connector_no_timeouts_when_not_configured() {
  let mut socket = MockSocket::new();
  let dns = MockDns::new(vec![IpAddr::V4([127, 0, 0, 1])]);
  let connector = Connector::new(&mut socket, &dns);

  let uri = Uri::parse("http://example.com").unwrap();
  let _result = connector.connect(&uri, &Config::default());

  assert_eq!(socket.read_timeout, None);
  assert_eq!(socket.write_timeout, None);
}

#[test]
fn connector_borrows_socket_and_dns() {
  let mut socket = MockSocket::new();
  let dns = MockDns::new(vec![IpAddr::V4([127, 0, 0, 1])]);

  {
    let connector = Connector::new(&mut socket, &dns);
    let uri = Uri::parse("http://example.com").unwrap();
    let _result = connector.connect(&uri, &Config::default());
  }

  assert!(socket.connected_addr.is_some());
}
