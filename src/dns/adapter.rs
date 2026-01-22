extern crate alloc;
use crate::error::DnsError;
use crate::util::IpAddr;
use alloc::vec::Vec;

pub trait DnsResolver {
  fn resolve(&self, host: &str) -> Result<Vec<IpAddr>, DnsError>;
}
