//! DNS resolver trait (hostname → addresses).

use crate::error::DnsError;
use crate::util::IpAddr;
use alloc::vec::Vec;

/// Resolve hostnames to IP addresses.
pub trait DnsResolver {
  /// Look up `host`. Empty `Vec` means NXDOMAIN / no records.
  ///
  /// # Errors
  /// Returns [`DnsError`] on resolver failure.
  fn resolve(
    &self,
    host: &str,
  ) -> Result<Vec<IpAddr>, DnsError>;
}
