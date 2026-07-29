use crate::dns::os::{host_cstring, ip_v4};
use crate::error::DnsError;
use crate::util::IpAddr;
use alloc::vec::Vec;
use core::ptr;
use windows_sys::Win32::Networking::WinSock::{
  ADDRINFOA, AF_INET, SOCKADDR_IN, WSAGetLastError, freeaddrinfo, getaddrinfo,
};

pub fn resolve_host(host: &str) -> Result<Vec<IpAddr>, DnsError> {
  let host_c = host_cstring(host)?;

  let mut result: *mut ADDRINFOA = ptr::null_mut();
  // V4-only: WinSock connect path has no IPv6.
  let hints = ADDRINFOA {
    ai_family: i32::from(AF_INET),
    ai_socktype: 0,
    ai_protocol: 0,
    ai_flags: 0,
    ai_addrlen: 0,
    ai_canonname: ptr::null_mut(),
    ai_addr: ptr::null_mut(),
    ai_next: ptr::null_mut(),
  };

  let ret = unsafe { getaddrinfo(host_c.as_ptr().cast(), ptr::null(), &raw const hints, &raw mut result) };

  if ret != 0 {
    let err_code = unsafe { WSAGetLastError() };
    return Err(DnsError::ResolutionFailed(err_code));
  }

  let mut addresses = Vec::new();
  let mut current = result;

  unsafe {
    while !current.is_null() {
      let info = &*current;

      if info.ai_family == i32::from(AF_INET) && !info.ai_addr.is_null() {
        let sockaddr = ptr::read_unaligned(info.ai_addr.cast::<SOCKADDR_IN>());
        addresses.push(ip_v4(sockaddr.sin_addr.S_un.S_addr.to_ne_bytes()));
      }

      current = info.ai_next;
    }

    freeaddrinfo(result);
  }

  if addresses.is_empty() {
    return Err(DnsError::NoAddressesFound);
  }

  Ok(addresses)
}
