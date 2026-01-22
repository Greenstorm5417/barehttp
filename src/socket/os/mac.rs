use crate::error::SocketError;
use crate::socket::{SocketAddr, SocketFlags};

/// macOS socket stub - not yet implemented
pub struct OsSocket {
  _placeholder: (),
}

impl OsSocket {
  pub fn new() -> Result<Self, SocketError> {
    Err(SocketError::Unsupported)
  }

  pub fn connect(&mut self, _addr: &SocketAddr) -> Result<(), SocketError> {
    Err(SocketError::Unsupported)
  }

  pub fn read(&mut self, _buf: &mut [u8]) -> Result<usize, SocketError> {
    Err(SocketError::Unsupported)
  }

  pub fn write(&mut self, _buf: &[u8]) -> Result<usize, SocketError> {
    Err(SocketError::Unsupported)
  }

  pub fn shutdown(&mut self) -> Result<(), SocketError> {
    Err(SocketError::Unsupported)
  }

  pub fn set_flags(&mut self, _flags: SocketFlags) -> Result<(), SocketError> {
    Err(SocketError::Unsupported)
  }

  pub fn set_read_timeout(&mut self, _timeout_ms: u32) -> Result<(), SocketError> {
    Err(SocketError::Unsupported)
  }

  pub fn set_write_timeout(&mut self, _timeout_ms: u32) -> Result<(), SocketError> {
    Err(SocketError::Unsupported)
  }
}
