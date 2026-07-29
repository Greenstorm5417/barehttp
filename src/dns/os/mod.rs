use crate::error::DnsError;
use crate::util::IpAddr;
use alloc::ffi::CString;
use core::net::{Ipv4Addr, Ipv6Addr};

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
pub use unix::resolve_host;
#[cfg(windows)]
pub use windows::resolve_host;

pub fn host_cstring(host: &str) -> Result<CString, DnsError> {
  CString::new(host).map_err(|_| DnsError::ResolutionFailed(-1))
}

pub fn ip_v4(bytes: [u8; 4]) -> IpAddr {
  IpAddr::V4(Ipv4Addr::from(bytes))
}

pub fn ip_v6(bytes: [u8; 16]) -> IpAddr {
  IpAddr::V6(Ipv6Addr::from(bytes))
}
