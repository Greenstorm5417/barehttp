/// Errors that can occur during socket operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketError {
  /// Socket is not connected
  NotConnected,
  /// Connection refused by remote host
  ConnectionRefused,
  /// Operation timed out
  TimedOut,
  /// Operation would block (non-blocking mode)
  WouldBlock,
  /// Operation was interrupted
  Interrupted,
  /// Invalid socket address
  InvalidAddress,
  /// Operation not supported
  Unsupported,
  /// DNS resolution failed with error code
  DnsResolutionFailed(i32),
  /// Operating system error with code
  OsError(i32),
}

impl core::fmt::Display for SocketError {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    match self {
      Self::NotConnected => write!(f, "socket not connected"),
      Self::ConnectionRefused => write!(f, "connection refused"),
      Self::TimedOut => write!(f, "operation timed out"),
      Self::WouldBlock => write!(f, "operation would block"),
      Self::Interrupted => write!(f, "operation interrupted"),
      Self::InvalidAddress => write!(f, "invalid address"),
      Self::Unsupported => write!(f, "operation not supported"),
      Self::DnsResolutionFailed(code) => write!(f, "DNS resolution failed: {code}"),
      Self::OsError(code) => write!(f, "OS error: {code}"),
    }
  }
}
