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

pub fn host_cstring(host: &str) -> Result<CString, DnsError> {
  CString::new(host).map_err(|_| DnsError::ResolutionFailed(-1))
}
