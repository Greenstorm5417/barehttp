//! Custom `DnsResolver` + `BlockingSocket` on `HttpClient`.
//! Logging wrappers over the OS resolver and socket.

use barehttp::config::Config;
use barehttp::{
  BlockingSocket, BlockingSocketFactory, DnsResolver, Error, HttpClient, IpAddr, OsBlockingSocket,
  OsDnsResolver, SocketAddr,
};
use barehttp::{DnsError, SocketError};

/// OS DNS with resolve logging.
struct LoggingDns {
  inner: OsDnsResolver,
}

impl DnsResolver for LoggingDns {
  fn resolve(
    &self,
    host: &str,
  ) -> Result<Vec<IpAddr>, DnsError> {
    println!("dns: resolving {host}");
    let addrs = self.inner.resolve(host)?;
    println!("dns: {host} -> {addrs:?}");
    Ok(addrs)
  }
}

/// OS TCP with connect / byte-count logging.
struct LoggingSocket {
  inner: OsBlockingSocket,
}

impl BlockingSocket for LoggingSocket {
  fn connect(
    &mut self,
    addr: &SocketAddr,
    host: &str,
  ) -> Result<(), SocketError> {
    println!("tcp: connect {addr} (host={host})");
    self.inner.connect(addr, host)
  }

  fn read(
    &mut self,
    buf: &mut [u8],
  ) -> Result<usize, SocketError> {
    let n = self.inner.read(buf)?;
    println!("tcp: read {n} bytes");
    Ok(n)
  }

  fn write(
    &mut self,
    buf: &[u8],
  ) -> Result<usize, SocketError> {
    let n = self.inner.write(buf)?;
    println!("tcp: write {n} bytes");
    Ok(n)
  }

  fn shutdown(&mut self) -> Result<(), SocketError> {
    println!("tcp: shutdown");
    self.inner.shutdown()
  }

  fn set_read_timeout(
    &mut self,
    timeout_ms: u32,
  ) -> Result<(), SocketError> {
    self.inner.set_read_timeout(timeout_ms)
  }

  fn set_write_timeout(
    &mut self,
    timeout_ms: u32,
  ) -> Result<(), SocketError> {
    self.inner.set_write_timeout(timeout_ms)
  }
}

impl BlockingSocketFactory for LoggingSocket {
  fn new() -> Result<Self, SocketError> {
    Ok(Self {
      inner: OsBlockingSocket::new()?,
    })
  }
}

fn main() -> Result<(), Error> {
  let dns = LoggingDns { inner: OsDnsResolver };
  let client: HttpClient<LoggingSocket, LoggingDns> = HttpClient::with_adapters(dns, Config::default());

  let response = client.get("http://example.com/").call()?;
  let preview: String = response.to_text()?.chars().take(120).collect();
  println!("{} {}", response.status(), preview);
  Ok(())
}
