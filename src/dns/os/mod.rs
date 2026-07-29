use crate::error::DnsError;
use alloc::ffi::CString;

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
pub use unix::resolve_host;
#[cfg(windows)]
pub use windows::resolve_host;

#[cfg(not(any(unix, windows)))]
pub fn resolve_host(_host: &str) -> Result<alloc::vec::Vec<crate::util::IpAddr>, DnsError> {
  Err(DnsError::ResolutionFailed(-1))
}

pub fn host_cstring(host: &str) -> Result<CString, DnsError> {
  CString::new(host).map_err(|_| DnsError::ResolutionFailed(-1))
}
