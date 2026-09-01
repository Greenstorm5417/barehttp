use crate::dns::os::host_cstring;
use crate::error::DnsError;
use crate::util::IpAddr;
use alloc::vec::Vec;
use core::net::{Ipv4Addr, Ipv6Addr};
use core::ptr;
use libc::{AF_INET, AF_INET6, addrinfo, sockaddr_in, sockaddr_in6};

/// Owns a `getaddrinfo` linked list until `freeaddrinfo`.
struct AddrInfoList(*mut addrinfo);

impl Drop for AddrInfoList {
  fn drop(&mut self) {
    if !self.0.is_null() {
      // SAFETY: `self.0` is a successful `getaddrinfo` list; freed exactly once here.
      unsafe { libc::freeaddrinfo(self.0) };
    }
  }
}

pub fn resolve_host(host: &str) -> Result<Vec<IpAddr>, DnsError> {
  let host_c = host_cstring(host)?;

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

  let mut raw: *mut addrinfo = ptr::null_mut();

  // SAFETY: `host_c` is a NUL-terminated CString; `hints`/`raw` are valid for getaddrinfo.
  let ret = unsafe { libc::getaddrinfo(host_c.as_ptr(), ptr::null(), &raw const hints, &raw mut raw) };

  if ret != 0 {
    return Err(DnsError::ResolutionFailed(ret));
  }

  // POSIX: `ret == 0` means `raw` is a getaddrinfo-owned list (empty list is still owned).
  let list = AddrInfoList(raw);
  let mut addresses = Vec::new();
  let mut current = list.0;

  // codeql[rust/access-invalid-pointer]: getaddrinfo wrote a valid list into `raw` when ret==0.
  // SAFETY: `current` is null or a remaining node of `list` until Drop; `as_ref` is the null check.
  while let Some(info) = unsafe { current.as_ref() } {
    if info.ai_family == AF_INET && !info.ai_addr.is_null() {
      // SAFETY: AF_INET + non-null `ai_addr` is a `sockaddr_in` (possibly unaligned).
      let sockaddr = unsafe { ptr::read_unaligned(info.ai_addr.cast::<sockaddr_in>()) };
      addresses.push(IpAddr::V4(Ipv4Addr::from(sockaddr.sin_addr.s_addr.to_ne_bytes())));
    } else if info.ai_family == AF_INET6 && !info.ai_addr.is_null() {
      // SAFETY: AF_INET6 + non-null `ai_addr` is a `sockaddr_in6` (possibly unaligned).
      let sockaddr = unsafe { ptr::read_unaligned(info.ai_addr.cast::<sockaddr_in6>()) };
      addresses.push(IpAddr::V6(Ipv6Addr::from(sockaddr.sin6_addr.s6_addr)));
    }

    current = info.ai_next;
  }

  if addresses.is_empty() {
    return Err(DnsError::NoAddressesFound);
  }

  Ok(addresses)
}
