//! Blocking socket trait and OS TCP adapter.

mod os;

pub use core::net::SocketAddr;
pub use os::OsSocket as OsBlockingSocket;

use crate::error::SocketError;

/// Blocking byte-stream socket (object-safe).
///
/// Object-safe I/O only (`dyn BlockingSocket`). Build OS sockets via
/// [`OsBlockingSocket::new`]; custom [`crate::HttpClient`] adapters implement
/// [`BlockingSocketFactory`].
///
/// New methods added to this trait will provide default implementations when
/// possible so existing adapters keep compiling.
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

  /// Connect deadline in milliseconds (`0` = block until connected / OS default).
  ///
  /// On OS adapters this is nonblocking connect plus `poll`/`select`; the trait default
  /// is a no-op so custom adapters can ignore it. Do not use [`Self::set_write_timeout`]
  /// as a connect deadline.
  ///
  /// # Errors
  /// [`SocketError::Unsupported`] if connect timeouts are unavailable; otherwise OS failures.
  fn set_connect_timeout(
    &mut self,
    timeout_ms: u32,
  ) -> Result<(), SocketError> {
    let _ = timeout_ms;
    Ok(())
  }

  /// `true` if this socket is (or wraps) cleartext OS TCP.
  ///
  /// Defaults to `true`. TLS adapters must return `false`.
  /// The client rejects [`crate::config::Config::assume_tls_socket`] when this is `true`.
  #[must_use]
  fn is_os_cleartext() -> bool
  where
    Self: Sized,
  {
    true
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
