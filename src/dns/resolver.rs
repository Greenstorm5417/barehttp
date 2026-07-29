use crate::dns::adapter::DnsResolver;
use crate::dns::os;
use crate::error::DnsError;
use crate::util::IpAddr;
use alloc::vec::Vec;

/// Operating system DNS resolver (`getaddrinfo`).
#[derive(Debug, Default, Clone, Copy)]
pub struct OsDnsResolver;

impl DnsResolver for OsDnsResolver {
  fn resolve(
    &self,
    host: &str,
  ) -> Result<Vec<IpAddr>, DnsError> {
    os::resolve_host(host)
  }
}
