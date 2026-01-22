extern crate alloc;
use crate::error::DnsError;
use crate::util::IpAddr;
use alloc::vec::Vec;

pub fn resolve_host(_host: &str) -> Result<Vec<IpAddr>, DnsError> {
  Err(DnsError::Unsupported)
}
