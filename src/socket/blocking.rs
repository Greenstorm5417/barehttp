use crate::error::SocketError;
use crate::socket::{SocketFlags, adapter::BlockingSocket, adapter::SocketAddr, os};

/// Operating system blocking socket
///
/// Uses platform-specific socket APIs (`WinSock` on Windows, BSD sockets on Unix).
pub struct OsBlockingSocket {
  inner: os::OsSocket,
}

impl OsBlockingSocket {
  /// # Errors
  /// Returns an error if the underlying OS socket cannot be created.
  pub fn new() -> Result<Self, SocketError> {
    Ok(Self {
      inner: os::OsSocket::new()?,
    })
  }

  /// # Errors
  /// Returns an error if the read timeout cannot be set on the underlying socket.
  pub fn set_read_timeout(
    &mut self,
    timeout_ms: u32,
  ) -> Result<(), SocketError> {
    self.inner.set_read_timeout(timeout_ms)
  }

  /// # Errors
  /// Returns an error if the write timeout cannot be set on the underlying socket.
  pub fn set_write_timeout(
    &mut self,
    timeout_ms: u32,
  ) -> Result<(), SocketError> {
    self.inner.set_write_timeout(timeout_ms)
  }
}

impl BlockingSocket for OsBlockingSocket {
  fn new() -> Result<Self, SocketError> {
    Self::new()
  }

  fn connect(
    &mut self,
    addr: &SocketAddr,
  ) -> Result<(), SocketError> {
    self.inner.connect(addr)
  }

  fn read(
    &mut self,
    buf: &mut [u8],
  ) -> Result<usize, SocketError> {
    self.inner.read(buf)
  }

  fn write(
    &mut self,
    buf: &[u8],
  ) -> Result<usize, SocketError> {
    self.inner.write(buf)
  }

  fn shutdown(&mut self) -> Result<(), SocketError> {
    self.inner.shutdown()
  }

  fn set_flags(
    &mut self,
    flags: SocketFlags,
  ) -> Result<(), SocketError> {
    self.inner.set_flags(flags)
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
