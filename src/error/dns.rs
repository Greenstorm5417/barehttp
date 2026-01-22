/// Errors that can occur during DNS resolution
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DnsError {
  /// DNS resolution failed with error code
  ResolutionFailed(i32),
  /// No IP addresses found for hostname
  NoAddressesFound,
  /// Hostname is invalid or malformed
  InvalidHostname,
  /// DNS operation not supported on this platform
  Unsupported,
  /// Operating system error with code
  OsError(i32),
}

impl core::fmt::Display for DnsError {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    match self {
      Self::ResolutionFailed(code) => write!(f, "DNS resolution failed: {code}"),
      Self::NoAddressesFound => write!(f, "no addresses found for hostname"),
      Self::InvalidHostname => write!(f, "invalid hostname"),
      Self::Unsupported => write!(f, "DNS operation not supported"),
      Self::OsError(code) => write!(f, "OS error: {code}"),
    }
  }
}
