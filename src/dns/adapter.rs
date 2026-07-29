//! DNS resolver trait (hostname → addresses).

use crate::error::DnsError;
use crate::util::IpAddr;
use alloc::vec::Vec;

/// Resolve hostnames to IP addresses.
pub trait DnsResolver {
  /// Look up `host`. An empty `Vec` means NXDOMAIN / no records.
  ///
  /// # Errors
  /// [`DnsError::ResolutionFailed`] when the OS resolver itself fails.
  fn resolve(
    &self,
    host: &str,
  ) -> Result<Vec<IpAddr>, DnsError>;
}
