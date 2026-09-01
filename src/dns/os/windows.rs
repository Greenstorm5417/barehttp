use crate::dns::os::host_cstring;
use crate::error::DnsError;
use crate::util::IpAddr;
use alloc::vec::Vec;
use core::mem::size_of;
use core::net::{Ipv4Addr, Ipv6Addr};
use core::ptr::{self, NonNull};
use windows_sys::Win32::Networking::WinSock::{
  ADDRINFOA, AF_INET, AF_INET6, SOCKADDR_IN, SOCKADDR_IN6, WSAGetLastError, freeaddrinfo, getaddrinfo,
};

/// Owns a `getaddrinfo` linked list until `freeaddrinfo`.
struct AddrInfoList(NonNull<ADDRINFOA>);

impl Drop for AddrInfoList {
  fn drop(&mut self) {
    // SAFETY: `self.0` is a successful `getaddrinfo` list; freed exactly once here.
    unsafe { freeaddrinfo(self.0.as_ptr()) };
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
  let list = AddrInfoList(NonNull::new(raw).ok_or(DnsError::NoAddressesFound)?);
  let mut addresses = Vec::new();
  let mut current = Some(list.0);

  while let Some(node) = current {
    // SAFETY: `node` is a remaining `getaddrinfo` node until Drop.
    let info = unsafe { node.as_ref() };
    let addr_len = usize::try_from(info.ai_addrlen).unwrap_or(0);

    if info.ai_family == i32::from(AF_INET)
      && addr_len >= size_of::<SOCKADDR_IN>()
      && let Some(addr) = NonNull::new(info.ai_addr.cast::<SOCKADDR_IN>())
    {
      // SAFETY: AF_INET node; `ai_addr` is an OS-aligned `SOCKADDR_IN` of `addr_len` bytes.
      // `IN_ADDR.S_un.S_addr` is the IPv4 view, written by getaddrinfo.
      let ipv4 = unsafe { addr.as_ref().sin_addr.S_un.S_addr };
      addresses.push(IpAddr::V4(Ipv4Addr::from(ipv4.to_ne_bytes())));
    } else if info.ai_family == i32::from(AF_INET6)
      && addr_len >= size_of::<SOCKADDR_IN6>()
      && let Some(addr) = NonNull::new(info.ai_addr.cast::<SOCKADDR_IN6>())
    {
      // SAFETY: AF_INET6 node; `ai_addr` is an OS-aligned `SOCKADDR_IN6` of `addr_len` bytes.
      // `IN6_ADDR.u.Byte` is the octet view, written by getaddrinfo.
      let octets = unsafe { addr.as_ref().sin6_addr.u.Byte };
      addresses.push(IpAddr::V6(Ipv6Addr::from(octets)));
    }

    current = NonNull::new(info.ai_next);
  }

  if addresses.is_empty() {
    return Err(DnsError::NoAddressesFound);
  }

  Ok(addresses)
}
