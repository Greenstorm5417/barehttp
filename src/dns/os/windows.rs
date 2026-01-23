extern crate alloc;
use crate::error::DnsError;
use crate::util::IpAddr;
use alloc::vec::Vec;
use core::ptr;
use windows_sys::Win32::Networking::WinSock::{
  ADDRINFOA, AF_INET, AF_INET6, SOCKADDR_IN, SOCKADDR_IN6, WSAGetLastError, freeaddrinfo, getaddrinfo,
};

pub fn resolve_host(host: &str) -> Result<Vec<IpAddr>, DnsError> {
  let mut host_cstring = Vec::with_capacity(host.len() + 1);
  host_cstring.extend_from_slice(host.as_bytes());
  host_cstring.push(0);

  let mut result: *mut ADDRINFOA = ptr::null_mut();
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

  let ret = unsafe {
    getaddrinfo(
      host_cstring.as_ptr() as *const _,
      ptr::null(),
      &raw const hints,
      &raw mut result,
    )
  };

  if ret != 0 {
    let err_code = unsafe { WSAGetLastError() };
    return Err(DnsError::ResolutionFailed(err_code));
  }

  let mut addresses = Vec::new();
  let mut current = result;

  unsafe {
    while !current.is_null() {
      let info = &*current;

      if info.ai_family == i32::from(AF_INET) {
        if !info.ai_addr.is_null() {
          let sockaddr = ptr::read_unaligned(info.ai_addr.cast::<SOCKADDR_IN>());
          let addr_bytes = sockaddr.sin_addr.S_un.S_addr.to_ne_bytes();
          addresses.push(IpAddr::V4(addr_bytes));
        }
      } else if info.ai_family == i32::from(AF_INET6) && !info.ai_addr.is_null() {
        let sockaddr = ptr::read_unaligned(info.ai_addr.cast::<SOCKADDR_IN6>());
        let addr_bytes = sockaddr.sin6_addr.u.Byte;

        let mut addr = [0u16; 8];
        for i in 0usize..8 {
          let idx1 = i.wrapping_mul(2);
          let idx2 = idx1.wrapping_add(1);
          if let (Some(&byte1), Some(&byte2), Some(dest)) =
            (addr_bytes.get(idx1), addr_bytes.get(idx2), addr.get_mut(i))
          {
            *dest = u16::from_be_bytes([byte1, byte2]);
          }
        }
        addresses.push(IpAddr::V6(addr));
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
