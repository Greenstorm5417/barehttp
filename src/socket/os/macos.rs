use crate::error::SocketError;
use crate::socket::{SocketAddr, SocketFlags};

/// macOS socket stub - not yet implemented
pub struct OsSocket {
  _placeholder: (),
}

#[allow(clippy::unused_self, clippy::needless_pass_by_ref_mut)]
impl OsSocket {
  pub const fn new() -> Result<Self, SocketError> {
    Err(SocketError::Unsupported)
  }

  pub const fn connect(&mut self, _addr: &SocketAddr) -> Result<(), SocketError> {
    Err(SocketError::Unsupported)
  }

  pub const fn read(&mut self, _buf: &mut [u8]) -> Result<usize, SocketError> {
    Err(SocketError::Unsupported)
  }

  pub const fn write(&mut self, _buf: &[u8]) -> Result<usize, SocketError> {
    Err(SocketError::Unsupported)
  }

  pub const fn shutdown(&mut self) -> Result<(), SocketError> {
    Err(SocketError::Unsupported)
  }

  pub const fn set_flags(&mut self, _flags: SocketFlags) -> Result<(), SocketError> {
    Err(SocketError::Unsupported)
  }

  pub const fn set_read_timeout(&mut self, _timeout_ms: u32) -> Result<(), SocketError> {
    Err(SocketError::Unsupported)
  }

  pub const fn set_write_timeout(&mut self, _timeout_ms: u32) -> Result<(), SocketError> {
    Err(SocketError::Unsupported)
  }
}
