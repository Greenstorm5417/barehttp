//! Blocking socket trait (TLS wrappers, mocks, embedded stacks).

use crate::error::SocketError;
use core::net::SocketAddr;

/// Blocking byte-stream socket. Implement for TLS, proxies, or test doubles.
pub trait BlockingSocket: Sized {
  /// Create a new unbound socket.
  ///
  /// # Errors
  /// Returns [`SocketError`] if the OS socket cannot be created.
  fn new() -> Result<Self, SocketError>;
  /// Connect to `addr`.
  ///
  /// # Errors
  /// Returns [`SocketError`] on connect failure.
  fn connect(
    &mut self,
    addr: &SocketAddr,
  ) -> Result<(), SocketError>;
  /// Read into `buf`. Returns bytes read.
  ///
  /// # Errors
  /// Returns [`SocketError`] on I/O failure.
  fn read(
    &mut self,
    buf: &mut [u8],
  ) -> Result<usize, SocketError>;
  /// Write `buf`. Returns bytes written.
  ///
  /// # Errors
  /// Returns [`SocketError`] on I/O failure.
  fn write(
    &mut self,
    buf: &[u8],
  ) -> Result<usize, SocketError>;
  /// Shut down the socket.
  ///
  /// # Errors
  /// Returns [`SocketError`] on failure.
  fn shutdown(&mut self) -> Result<(), SocketError>;
  /// Read timeout in milliseconds (`0` = blocking forever, if supported).
  ///
  /// # Errors
  /// Returns [`SocketError`] if the timeout cannot be set.
  fn set_read_timeout(
    &mut self,
    timeout_ms: u32,
  ) -> Result<(), SocketError>;
  /// Write timeout in milliseconds.
  ///
  /// # Errors
  /// Returns [`SocketError`] if the timeout cannot be set.
  fn set_write_timeout(
    &mut self,
    timeout_ms: u32,
  ) -> Result<(), SocketError>;
}
