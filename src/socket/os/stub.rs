//! Stub OS socket for targets that are neither unix nor windows.

use crate::error::SocketError;
use crate::socket::{BlockingSocket, SocketAddr};

/// Cleartext OS TCP is unavailable on this target.
#[derive(Debug, Default)]
pub struct OsSocket;

impl BlockingSocket for OsSocket {
  fn new() -> Result<Self, SocketError> {
    Err(SocketError::Unsupported)
  }

  fn connect(
    &mut self,
    _addr: &SocketAddr,
    _host: &str,
  ) -> Result<(), SocketError> {
    Err(SocketError::Unsupported)
  }

  fn read(
    &mut self,
    _buf: &mut [u8],
  ) -> Result<usize, SocketError> {
    Err(SocketError::Unsupported)
  }

  fn write(
    &mut self,
    _buf: &[u8],
  ) -> Result<usize, SocketError> {
    Err(SocketError::Unsupported)
  }

  fn shutdown(&mut self) -> Result<(), SocketError> {
    Err(SocketError::Unsupported)
  }

  fn set_read_timeout(
    &mut self,
    _timeout_ms: u32,
  ) -> Result<(), SocketError> {
    Err(SocketError::Unsupported)
  }

  fn set_write_timeout(
    &mut self,
    _timeout_ms: u32,
  ) -> Result<(), SocketError> {
    Err(SocketError::Unsupported)
  }

  fn is_os_cleartext() -> bool {
    true
  }
}
