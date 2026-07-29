//! Blocking socket trait and OS TCP adapter.

mod os;

pub use core::net::SocketAddr;
pub use os::OsSocket as OsBlockingSocket;

use crate::error::SocketError;

/// Blocking byte-stream socket (object-safe).
///
/// I/O methods only; usable as `dyn BlockingSocket`. Create OS sockets with
/// [`OsBlockingSocket::new`]. Custom adapters for [`crate::HttpClient`] implement
/// [`BlockingSocketFactory`].
///
/// # Examples
///
/// ```no_run
/// use barehttp::{BlockingSocket, OsBlockingSocket};
///
/// let mut sock = OsBlockingSocket::new()?;
/// sock.set_read_timeout(5_000)?;
/// # Ok::<(), barehttp::SocketError>(())
/// ```
pub trait BlockingSocket {
  /// Connect to `addr`.
  ///
  /// `host` is the URI hostname for SNI / TLS identity. Cleartext adapters may ignore it.
  ///
  /// # Errors
  /// [`SocketError::ConnectionRefused`], [`SocketError::TimedOut`],
  /// [`SocketError::InvalidAddress`], or [`SocketError::OsError`].
  fn connect(
    &mut self,
    addr: &SocketAddr,
    host: &str,
  ) -> Result<(), SocketError>;

  /// Read into `buf`; returns bytes read.
  ///
  /// # Errors
  /// [`SocketError::NotConnected`], [`SocketError::TimedOut`],
  /// [`SocketError::Interrupted`], or [`SocketError::OsError`].
  fn read(
    &mut self,
    buf: &mut [u8],
  ) -> Result<usize, SocketError>;

  /// Write `buf`; returns bytes written.
  ///
  /// # Errors
  /// [`SocketError::NotConnected`], [`SocketError::TimedOut`],
  /// [`SocketError::Interrupted`], or [`SocketError::OsError`].
  fn write(
    &mut self,
    buf: &[u8],
  ) -> Result<usize, SocketError>;

  /// Shut down the socket.
  ///
  /// # Errors
  /// [`SocketError::NotConnected`] or [`SocketError::OsError`].
  fn shutdown(&mut self) -> Result<(), SocketError>;

  /// Read timeout in milliseconds (`0` = block until data, if the platform supports it).
  ///
  /// # Errors
  /// [`SocketError::Unsupported`] if timeouts are unavailable; otherwise OS set-option failures.
  fn set_read_timeout(
    &mut self,
    timeout_ms: u32,
  ) -> Result<(), SocketError>;

  /// Write timeout in milliseconds (`0` = block until writable, if supported).
  ///
  /// # Errors
  /// [`SocketError::Unsupported`] if timeouts are unavailable; otherwise OS set-option failures.
  fn set_write_timeout(
    &mut self,
    timeout_ms: u32,
  ) -> Result<(), SocketError>;

  /// `true` for the cleartext OS TCP adapter ([`crate::OsBlockingSocket`]).
  ///
  /// Default is `false` (TLS wrappers). The client uses this to reject
  /// [`crate::config::Config::assume_tls_socket`] with a cleartext OS socket.
  #[must_use]
  fn is_os_cleartext() -> bool
  where
    Self: Sized,
  {
    false
  }
}

/// Factory for unbound sockets used by [`crate::HttpClient`].
///
/// Outside [`BlockingSocket`]: methods returning `Self` break object safety.
pub trait BlockingSocketFactory: BlockingSocket + Sized {
  /// Create an unbound socket.
  ///
  /// # Errors
  /// [`SocketError::OsError`] if the OS cannot create the socket.
  fn new() -> Result<Self, SocketError>;
}
