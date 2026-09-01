use crate::dns::os::host_cstring;
use crate::error::DnsError;
use crate::util::IpAddr;
use alloc::vec::Vec;
use core::net::{Ipv4Addr, Ipv6Addr};
use core::ptr;
use windows_sys::Win32::Networking::WinSock::{
  ADDRINFOA, AF_INET, AF_INET6, SOCKADDR_IN, SOCKADDR_IN6, WSAGetLastError, freeaddrinfo, getaddrinfo,
};

/// Owns a `getaddrinfo` linked list until `freeaddrinfo`.
struct AddrInfoList(*mut ADDRINFOA);

impl Drop for AddrInfoList {
  fn drop(&mut self) {
    if !self.0.is_null() {
      // SAFETY: `self.0` is a successful `getaddrinfo` list; freed exactly once here.
      unsafe { freeaddrinfo(self.0) };
    }
  }
}

pub fn resolve_host(host: &str) -> Result<Vec<IpAddr>, DnsError> {
  let host_c = host_cstring(host)?;

  let mut raw: *mut ADDRINFOA = ptr::null_mut();
  // AF_UNSPEC (0): return both IPv4 and IPv6, matching the Unix resolver.
  let hints = ADDRINFOA {
    ai_family: 0,
    ai_socktype: 0,
    ai_protocol: 0,
    ai_flags: 0,
    ai_addrlen: 0,
    ai_canonname: ptr::null_mut(),
    ai_addr: ptr::null_mut(),
    ai_next: ptr::null_mut(),
  };

  // SAFETY: `host_c` is NUL-terminated; `hints`/`raw` are valid for getaddrinfo.
  let ret = unsafe { getaddrinfo(host_c.as_ptr().cast(), ptr::null(), &raw const hints, &raw mut raw) };

  if ret != 0 {
    // SAFETY: reads thread-local Winsock last-error after a failed getaddrinfo.
    let err_code = unsafe { WSAGetLastError() };
    return Err(DnsError::ResolutionFailed(err_code));
  }

  // WinSock: `ret == 0` means `raw` is a getaddrinfo-owned list (empty list is still owned).
  let list = AddrInfoList(raw);
  let mut addresses = Vec::new();
  let mut current = list.0;

  // codeql[rust/access-invalid-pointer]: getaddrinfo wrote a valid list into `raw` when ret==0.
  // SAFETY: `current` is null or a remaining node of `list` until Drop; `as_ref` is the null check.
  while let Some(info) = unsafe { current.as_ref() } {
    if info.ai_family == i32::from(AF_INET) && !info.ai_addr.is_null() {
      // SAFETY: AF_INET + non-null `ai_addr` is a `SOCKADDR_IN` (possibly unaligned).
      let sockaddr = unsafe { ptr::read_unaligned(info.ai_addr.cast::<SOCKADDR_IN>()) };
      addresses.push(IpAddr::V4(Ipv4Addr::from(sockaddr.sin_addr.S_un.S_addr.to_ne_bytes())));
    } else if info.ai_family == i32::from(AF_INET6) && !info.ai_addr.is_null() {
      // SAFETY: AF_INET6 + non-null `ai_addr` is a `SOCKADDR_IN6` (possibly unaligned).
      let sockaddr = unsafe { ptr::read_unaligned(info.ai_addr.cast::<SOCKADDR_IN6>()) };
      // IN6_ADDR.u is a union; Byte is the octet view.
      let octets = sockaddr.sin6_addr.u.Byte;
      addresses.push(IpAddr::V6(Ipv6Addr::from(octets)));
    }

    current = info.ai_next;
  }

  if addresses.is_empty() {
    return Err(DnsError::NoAddressesFound);
  }

  Ok(addresses)
}
