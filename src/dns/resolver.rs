extern crate alloc;
use crate::dns::adapter::DnsResolver;
use crate::dns::os;
use crate::error::DnsError;
use crate::util::IpAddr;
use alloc::vec::Vec;

/// Operating system DNS resolver
///
/// Uses the platform's native DNS resolution (e.g., `getaddrinfo` on Unix).
pub struct OsDnsResolver {
  _marker: (),
}

impl OsDnsResolver {
  /// Create a new OS DNS resolver
  #[must_use]
  pub const fn new() -> Self {
    Self { _marker: () }
  }
}

impl Default for OsDnsResolver {
  fn default() -> Self {
    Self::new()
  }
}

impl DnsResolver for OsDnsResolver {
  fn resolve(
    &self,
    host: &str,
  ) -> Result<Vec<IpAddr>, DnsError> {
    os::resolve_host(host)
  }
}
