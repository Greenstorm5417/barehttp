use crate::error::SocketError;
use core::net::{SocketAddr, SocketAddrV4, SocketAddrV6};
use libc::{c_int, c_void, sockaddr, sockaddr_in, sockaddr_in6, socklen_t, timeval};

const fn map_errno(err: c_int) -> SocketError {
  match err {
    libc::ECONNREFUSED => SocketError::ConnectionRefused,
    libc::ETIMEDOUT => SocketError::TimedOut,
    libc::EINTR => SocketError::Interrupted,
    libc::ENOTCONN => SocketError::NotConnected,
    libc::EINVAL => SocketError::InvalidAddress,
    libc::EOPNOTSUPP => SocketError::Unsupported,
    _ => SocketError::OsError(err),
  }
}

fn get_last_error() -> SocketError {
  unsafe {
    #[cfg(target_os = "macos")]
    let err = *libc::__error();
    #[cfg(not(target_os = "macos"))]
    let err = *libc::__errno_location();
    map_errno(err)
  }
}

/// OS blocking TCP socket (`WinSock` / BSD sockets).
pub struct OsSocket {
  fd: c_int,
  connected: bool,
  read_timeout_ms: Option<u32>,
  write_timeout_ms: Option<u32>,
}

impl crate::socket::BlockingSocket for OsSocket {
  fn new() -> Result<Self, SocketError> {
    Ok(Self {
      fd: -1,
      connected: false,
      read_timeout_ms: None,
      write_timeout_ms: None,
    })
  }

  fn connect(
    &mut self,
    addr: &SocketAddr,
    _host: &str,
  ) -> Result<(), SocketError> {
    if self.connected {
      return Ok(());
    }

    match addr {
      SocketAddr::V4(a) => self.connect_ipv4(a)?,
      SocketAddr::V6(a) => self.connect_ipv6(a)?,
    }
    Ok(())
  }

  fn is_os_cleartext() -> bool {
    true
  }

  fn read(
    &mut self,
    buf: &mut [u8],
  ) -> Result<usize, SocketError> {
    if !self.connected {
      return Err(SocketError::NotConnected);
    }

    loop {
      unsafe {
        let result = libc::read(self.fd, buf.as_mut_ptr() as *mut c_void, buf.len());

        if result < 0 {
          let err = get_last_error();
          if matches!(err, SocketError::Interrupted) {
            continue;
          }
          return Err(err);
        }

        if result == 0 {
          self.connected = false;
        }

        #[allow(clippy::cast_sign_loss)]
        {
          return Ok(result as usize);
        }
      }
    }
  }

  fn write(
    &mut self,
    buf: &[u8],
  ) -> Result<usize, SocketError> {
    if !self.connected {
      return Err(SocketError::NotConnected);
    }

    loop {
      unsafe {
        let result = libc::write(self.fd, buf.as_ptr() as *const c_void, buf.len());

        if result < 0 {
          let err = get_last_error();
          if matches!(err, SocketError::Interrupted) {
            continue;
          }
          return Err(err);
        }

        #[allow(clippy::cast_sign_loss)]
        {
          return Ok(result as usize);
        }
      }
    }
  }

  fn shutdown(&mut self) -> Result<(), SocketError> {
    if !self.connected {
      return Ok(());
    }

    unsafe {
      let result = libc::shutdown(self.fd, libc::SHUT_RDWR);
      if result < 0 {
        let err = get_last_error();
        if !matches!(err, SocketError::NotConnected) {
          return Err(err);
        }
      }
    }

    self.connected = false;
    Ok(())
  }

  fn set_read_timeout(
    &mut self,
    timeout_ms: u32,
  ) -> Result<(), SocketError> {
    self.read_timeout_ms = Some(timeout_ms);
    if self.fd >= 0 {
      self.apply_read_timeout(timeout_ms)?;
    }
    Ok(())
  }

  fn set_write_timeout(
    &mut self,
    timeout_ms: u32,
  ) -> Result<(), SocketError> {
    self.write_timeout_ms = Some(timeout_ms);
    if self.fd >= 0 {
      self.apply_write_timeout(timeout_ms)?;
    }
    Ok(())
  }
}

impl OsSocket {
  /// Fresh fd per attempt: failed connect leaves the socket unusable on Linux.
  fn recreate(
    &mut self,
    family: c_int,
  ) -> Result<(), SocketError> {
    unsafe {
      if self.fd >= 0 {
        libc::close(self.fd);
        self.fd = -1;
      }
      let fd = libc::socket(family, libc::SOCK_STREAM, libc::IPPROTO_TCP);
      if fd < 0 {
        return Err(get_last_error());
      }
      self.fd = fd;
    }
    if let Some(ms) = self.read_timeout_ms {
      self.apply_read_timeout(ms)?;
    }
    if let Some(ms) = self.write_timeout_ms {
      self.apply_write_timeout(ms)?;
    }
    Ok(())
  }

  fn apply_read_timeout(
    &mut self,
    timeout_ms: u32,
  ) -> Result<(), SocketError> {
    unsafe {
      #[allow(
        clippy::cast_lossless,
        clippy::integer_division,
        clippy::cast_possible_wrap,
        clippy::cast_possible_truncation
      )]
      let timeout = timeval {
        tv_sec: (timeout_ms.wrapping_div(1000)) as _,
        tv_usec: ((timeout_ms % 1000).wrapping_mul(1000)) as _,
      };

      #[allow(clippy::cast_possible_truncation)]
      let result = libc::setsockopt(
        self.fd,
        libc::SOL_SOCKET,
        libc::SO_RCVTIMEO,
        &raw const timeout as *const c_void,
        core::mem::size_of::<timeval>() as socklen_t,
      );

      if result < 0 {
        return Err(get_last_error());
      }
    }
    Ok(())
  }

  fn apply_write_timeout(
    &mut self,
    timeout_ms: u32,
  ) -> Result<(), SocketError> {
    unsafe {
      #[allow(
        clippy::cast_lossless,
        clippy::integer_division,
        clippy::cast_possible_wrap,
        clippy::cast_possible_truncation
      )]
      let timeout = timeval {
        tv_sec: (timeout_ms.wrapping_div(1000)) as _,
        tv_usec: ((timeout_ms % 1000).wrapping_mul(1000)) as _,
      };

      #[allow(clippy::cast_possible_truncation)]
      let result = libc::setsockopt(
        self.fd,
        libc::SOL_SOCKET,
        libc::SO_SNDTIMEO,
        &raw const timeout as *const c_void,
        core::mem::size_of::<timeval>() as socklen_t,
      );

      if result < 0 {
        return Err(get_last_error());
      }
    }
    Ok(())
  }

  fn connect_ipv4(
    &mut self,
    addr: &SocketAddrV4,
  ) -> Result<(), SocketError> {
    self.recreate(libc::AF_INET)?;

    unsafe {
      let mut sockaddr: sockaddr_in = core::mem::zeroed();
      #[allow(clippy::cast_possible_truncation)]
      {
        sockaddr.sin_family = libc::AF_INET as _;
      }
      sockaddr.sin_port = addr.port().to_be();
      sockaddr.sin_addr.s_addr = u32::from_ne_bytes(addr.ip().octets());

      #[allow(clippy::cast_possible_truncation)]
      let result = libc::connect(
        self.fd,
        &raw const sockaddr as *const sockaddr,
        core::mem::size_of::<sockaddr_in>() as socklen_t,
      );

      if result < 0 {
        return Err(get_last_error());
      }
    }

    self.connected = true;
    Ok(())
  }

  fn connect_ipv6(
    &mut self,
    addr: &SocketAddrV6,
  ) -> Result<(), SocketError> {
    self.recreate(libc::AF_INET6)?;

    unsafe {
      let mut sockaddr: sockaddr_in6 = core::mem::zeroed();
      #[allow(clippy::cast_possible_truncation)]
      {
        sockaddr.sin6_family = libc::AF_INET6 as _;
      }
      sockaddr.sin6_port = addr.port().to_be();
      sockaddr.sin6_addr.s6_addr = addr.ip().octets();

      #[allow(clippy::cast_possible_truncation)]
      let result = libc::connect(
        self.fd,
        &raw const sockaddr as *const sockaddr,
        core::mem::size_of::<sockaddr_in6>() as socklen_t,
      );

      if result < 0 {
        return Err(get_last_error());
      }
    }

    self.connected = true;
    Ok(())
  }
}

impl Drop for OsSocket {
  fn drop(&mut self) {
    if self.fd >= 0 {
      unsafe {
        libc::close(self.fd);
      }
    }
  }
}
