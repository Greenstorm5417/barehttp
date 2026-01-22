extern crate alloc;
use crate::error::DnsError;
use crate::util::IpAddr;
use alloc::vec::Vec;
use core::ptr;
use libc::{AF_INET, AF_INET6, addrinfo, sockaddr_in};

pub fn resolve_host(host: &str) -> Result<Vec<IpAddr>, DnsError> {
  let mut host_cstring = Vec::with_capacity(host.len() + 1);
  host_cstring.extend_from_slice(host.as_bytes());
  host_cstring.push(0);

  let hints = addrinfo {
    ai_family: 0,
    ai_socktype: 0,
    ai_protocol: 0,
    ai_flags: 0,
    ai_addrlen: 0,
    ai_canonname: ptr::null_mut(),
    ai_addr: ptr::null_mut(),
    ai_next: ptr::null_mut(),
  };

  let mut result: *mut addrinfo = ptr::null_mut();

  let ret = unsafe {
    libc::getaddrinfo(
      host_cstring.as_ptr() as *const i8,
      ptr::null(),
      &raw const hints,
      &raw mut result,
    )
  };

  if ret != 0 {
    return Err(DnsError::ResolutionFailed(ret));
  }

  let mut addresses = Vec::new();
  let mut current = result;

  unsafe {
    while !current.is_null() {
      let info = &*current;

      if info.ai_family == AF_INET && !info.ai_addr.is_null() {
        let sockaddr = ptr::read_unaligned(info.ai_addr.cast::<sockaddr_in>());
        let addr_bytes = sockaddr.sin_addr.s_addr.to_ne_bytes();
        addresses.push(IpAddr::V4(addr_bytes));
      } else if info.ai_family == AF_INET6 && !info.ai_addr.is_null() {
        // IPv6 support can be added here if needed
      }

      current = info.ai_next;
    }

    libc::freeaddrinfo(result);
  }

  if addresses.is_empty() {
    return Err(DnsError::NoAddressesFound);
  }

  Ok(addresses)
}
