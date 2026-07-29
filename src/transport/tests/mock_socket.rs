use crate::error::SocketError;
use crate::socket::{BlockingSocket, BlockingSocketFactory, SocketAddr};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Shared mock for connection + connector tests.
pub struct MockSocket {
  pub read_data: Vec<u8>,
  pub read_pos: usize,
  pub written: Vec<u8>,
  /// Cap bytes returned per `write` call (simulates short writes).
  pub max_write: usize,
  /// Cap bytes returned per `read` call (simulates fragmented TCP).
  pub max_read: usize,
  pub connected_addr: Option<String>,
  /// Hostname passed to [`BlockingSocket::connect`] (SNI).
  pub connected_host: Option<String>,
  pub read_timeout: Option<u32>,
  pub write_timeout: Option<u32>,
  pub should_fail_connect: bool,
  /// Fail this many connect attempts, then succeed.
  pub fail_connects_remaining: usize,
}

impl MockSocket {
  pub fn empty() -> Self {
    Self {
      read_data: Vec::new(),
      read_pos: 0,
      written: Vec::new(),
      max_write: usize::MAX,
      max_read: usize::MAX,
      connected_addr: None,
      connected_host: None,
      read_timeout: None,
      write_timeout: None,
      should_fail_connect: false,
      fail_connects_remaining: 0,
    }
  }

  pub fn with_response(response: &str) -> Self {
    Self {
      read_data: response.as_bytes().to_vec(),
      ..Self::empty()
    }
  }

  pub fn with_max_write(
    response: &str,
    max_write: usize,
  ) -> Self {
    Self {
      read_data: response.as_bytes().to_vec(),
      max_write,
      ..Self::empty()
    }
  }

  pub fn with_max_read(
    response: &str,
    max_read: usize,
  ) -> Self {
    Self {
      read_data: response.as_bytes().to_vec(),
      max_read: max_read.max(1),
      ..Self::empty()
    }
  }

  pub fn with_connect_failure() -> Self {
    Self {
      should_fail_connect: true,
      ..Self::empty()
    }
  }

  pub fn with_fail_first_n(n: usize) -> Self {
    Self {
      fail_connects_remaining: n,
      ..Self::empty()
    }
  }

  pub fn get_written(&self) -> String {
    String::from_utf8_lossy(&self.written).to_string()
  }
}

impl BlockingSocket for MockSocket {
  fn connect(
    &mut self,
    addr: &SocketAddr,
    host: &str,
  ) -> Result<(), SocketError> {
    if self.should_fail_connect {
      return Err(SocketError::NotConnected);
    }
    if self.fail_connects_remaining > 0 {
      self.fail_connects_remaining -= 1;
      return Err(SocketError::ConnectionRefused);
    }
    self.connected_addr = Some(format!("{addr}"));
    self.connected_host = Some(String::from(host));
    Ok(())
  }

  fn read(
    &mut self,
    buf: &mut [u8],
  ) -> Result<usize, SocketError> {
    if self.read_pos >= self.read_data.len() {
      return Ok(0);
    }
    let remaining = &self.read_data[self.read_pos..];
    let to_read = remaining.len().min(buf.len()).min(self.max_read);
    buf[..to_read].copy_from_slice(&remaining[..to_read]);
    self.read_pos += to_read;
    Ok(to_read)
  }

  fn write(
    &mut self,
    buf: &[u8],
  ) -> Result<usize, SocketError> {
    let n = buf.len().min(self.max_write);
    self.written.extend_from_slice(&buf[..n]);
    Ok(n)
  }

  fn shutdown(&mut self) -> Result<(), SocketError> {
    Ok(())
  }

  fn set_read_timeout(
    &mut self,
    timeout_ms: u32,
  ) -> Result<(), SocketError> {
    self.read_timeout = Some(timeout_ms);
    Ok(())
  }

  fn set_write_timeout(
    &mut self,
    timeout_ms: u32,
  ) -> Result<(), SocketError> {
    self.write_timeout = Some(timeout_ms);
    Ok(())
  }
}

impl BlockingSocketFactory for MockSocket {
  fn new() -> Result<Self, SocketError> {
    Ok(Self::empty())
  }
}
