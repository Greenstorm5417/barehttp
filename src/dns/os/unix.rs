use crate::dns::os::host_cstring;
use crate::error::DnsError;
use crate::util::IpAddr;
use alloc::vec::Vec;
use core::mem::size_of;
use core::net::{Ipv4Addr, Ipv6Addr};
use core::ptr::{self, NonNull};
use libc::{AF_INET, AF_INET6, addrinfo, sockaddr_in, sockaddr_in6};

/// Owns a `getaddrinfo` linked list until `freeaddrinfo`.
struct AddrInfoList(NonNull<addrinfo>);

impl Drop for AddrInfoList {
  fn drop(&mut self) {
    // SAFETY: `self.0` is a successful `getaddrinfo` list; freed exactly once here.
    unsafe { libc::freeaddrinfo(self.0.as_ptr()) };
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
  let list = AddrInfoList(NonNull::new(raw).ok_or(DnsError::NoAddressesFound)?);
  let mut addresses = Vec::new();
  let mut current = Some(list.0);

  while let Some(node) = current {
    // SAFETY: `node` is a remaining `getaddrinfo` node until Drop.
    let info = unsafe { node.as_ref() };
    let addr_len = usize::try_from(info.ai_addrlen).unwrap_or(0);

    if info.ai_family == AF_INET
      && addr_len >= size_of::<sockaddr_in>()
      && let Some(addr) = NonNull::new(info.ai_addr.cast::<sockaddr_in>())
    {
      // SAFETY: AF_INET node; `ai_addr` is an OS-aligned `sockaddr_in` of `addr_len` bytes.
      let sockaddr = unsafe { addr.as_ref() };
      addresses.push(IpAddr::V4(Ipv4Addr::from(sockaddr.sin_addr.s_addr.to_ne_bytes())));
    } else if info.ai_family == AF_INET6
      && addr_len >= size_of::<sockaddr_in6>()
      && let Some(addr) = NonNull::new(info.ai_addr.cast::<sockaddr_in6>())
    {
      // SAFETY: AF_INET6 node; `ai_addr` is an OS-aligned `sockaddr_in6` of `addr_len` bytes.
      let sockaddr = unsafe { addr.as_ref() };
      addresses.push(IpAddr::V6(Ipv6Addr::from(sockaddr.sin6_addr.s6_addr)));
    }

    current = NonNull::new(info.ai_next);
  }

  if addresses.is_empty() {
    return Err(DnsError::NoAddressesFound);
  }

  Ok(addresses)
}
