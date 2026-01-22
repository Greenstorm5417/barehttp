use crate::error::SocketError;
use crate::socket::{SocketAddr, SocketFlags};
use libc::{c_int, c_void, sockaddr, sockaddr_in, socklen_t, timeval};

const fn map_errno(err: c_int) -> SocketError {
  match err {
    libc::ECONNREFUSED => SocketError::ConnectionRefused,
    libc::ETIMEDOUT => SocketError::TimedOut,
    libc::EWOULDBLOCK => SocketError::WouldBlock,
    libc::EINTR => SocketError::Interrupted,
    libc::ENOTCONN => SocketError::NotConnected,
    libc::EINVAL => SocketError::InvalidAddress,
    libc::EOPNOTSUPP => SocketError::Unsupported,
    _ => SocketError::OsError(err),
  }
}

fn get_last_error() -> SocketError {
  unsafe {
    let err = *libc::__errno_location();
    map_errno(err)
  }
}

pub struct OsSocket {
  fd: c_int,
  connected: bool,
}

impl OsSocket {
  pub fn new() -> Result<Self, SocketError> {
    unsafe {
      let fd = libc::socket(libc::AF_INET, libc::SOCK_STREAM, libc::IPPROTO_TCP);
      if fd < 0 {
        return Err(get_last_error());
      }

      Ok(Self {
        fd,
        connected: false,
      })
    }
  }

  pub fn connect(&mut self, addr: &SocketAddr) -> Result<(), SocketError> {
    if self.connected {
      return Ok(());
    }

    match addr {
      SocketAddr::Ip {
        addr: ip_addr,
        port,
      } => match ip_addr {
        crate::util::IpAddr::V4(ipv4) => self.connect_ipv4(*ipv4, *port)?,
        crate::util::IpAddr::V6(_ipv6) => return Err(SocketError::Unsupported),
      },
      SocketAddr::Hostname { host, port: host_port } => {
        let host_str = core::str::from_utf8(host).map_err(|_| SocketError::InvalidAddress)?;

        let addresses = crate::dns::os::resolve_host(host_str).map_err(|e| match e {
          crate::error::DnsError::ResolutionFailed(code) => SocketError::DnsResolutionFailed(code),
          crate::error::DnsError::NoAddressesFound => SocketError::DnsResolutionFailed(0),
          crate::error::DnsError::InvalidHostname => SocketError::InvalidAddress,
          crate::error::DnsError::Unsupported => SocketError::Unsupported,
          crate::error::DnsError::OsError(code) => SocketError::OsError(code),
        })?;

        let mut last_error = SocketError::ConnectionRefused;
        for ip_addr in &addresses {
          let result = match ip_addr {
            crate::util::IpAddr::V4(ipv4) => self.connect_ipv4(*ipv4, *host_port),
            crate::util::IpAddr::V6(_) => Err(SocketError::Unsupported),
          };

          if result.is_ok() {
            return Ok(());
          }
          if let Err(e) = result {
            last_error = e;
          }
        }

        return Err(last_error);
      }
    }

    Ok(())
  }

  fn connect_ipv4(&mut self, addr: [u8; 4], port: u16) -> Result<(), SocketError> {
    unsafe {
      let mut sockaddr: sockaddr_in = core::mem::zeroed();
      #[allow(clippy::cast_possible_truncation)]
      {
        sockaddr.sin_family = libc::AF_INET as u16;
      }
      sockaddr.sin_port = port.to_be();
      sockaddr.sin_addr.s_addr = u32::from_ne_bytes(addr);

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

  pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, SocketError> {
    if !self.connected {
      return Err(SocketError::NotConnected);
    }

    unsafe {
      let result = libc::read(self.fd, buf.as_mut_ptr() as *mut c_void, buf.len());

      if result < 0 {
        return Err(get_last_error());
      }

      if result == 0 {
        self.connected = false;
      }

      #[allow(clippy::cast_sign_loss)]
      {
        Ok(result as usize)
      }
    }
  }

  pub fn write(&mut self, buf: &[u8]) -> Result<usize, SocketError> {
    if !self.connected {
      return Err(SocketError::NotConnected);
    }

    unsafe {
      let result = libc::write(self.fd, buf.as_ptr() as *const c_void, buf.len());

      if result < 0 {
        return Err(get_last_error());
      }

      #[allow(clippy::cast_sign_loss)]
      {
        Ok(result as usize)
      }
    }
  }

  pub fn shutdown(&mut self) -> Result<(), SocketError> {
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

  pub fn set_flags(&mut self, flags: SocketFlags) -> Result<(), SocketError> {
    unsafe {
      if flags.contains(SocketFlags::TCP_NODELAY) {
        let val: c_int = 1;
        #[allow(clippy::cast_possible_truncation)]
        let result = libc::setsockopt(
          self.fd,
          libc::IPPROTO_TCP,
          libc::TCP_NODELAY,
          &raw const val as *const c_void,
          core::mem::size_of::<c_int>() as socklen_t,
        );
        if result < 0 {
          return Err(get_last_error());
        }
      }

      if flags.contains(SocketFlags::KEEPALIVE) {
        let val: c_int = 1;
        #[allow(clippy::cast_possible_truncation)]
        let result = libc::setsockopt(
          self.fd,
          libc::SOL_SOCKET,
          libc::SO_KEEPALIVE,
          &raw const val as *const c_void,
          core::mem::size_of::<c_int>() as socklen_t,
        );
        if result < 0 {
          return Err(get_last_error());
        }
      }

      if flags.contains(SocketFlags::REUSEADDR) {
        let val: c_int = 1;
        #[allow(clippy::cast_possible_truncation)]
        let result = libc::setsockopt(
          self.fd,
          libc::SOL_SOCKET,
          libc::SO_REUSEADDR,
          &raw const val as *const c_void,
          core::mem::size_of::<c_int>() as socklen_t,
        );
        if result < 0 {
          return Err(get_last_error());
        }
      }
    }

    Ok(())
  }

  pub fn set_read_timeout(&mut self, timeout_ms: u32) -> Result<(), SocketError> {
    unsafe {
      #[allow(clippy::cast_lossless, clippy::integer_division)]
      let timeout = timeval {
        tv_sec: i64::from(timeout_ms.wrapping_div(1000)),
        tv_usec: i64::from((timeout_ms % 1000).wrapping_mul(1000)),
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

  pub fn set_write_timeout(&mut self, timeout_ms: u32) -> Result<(), SocketError> {
    unsafe {
      #[allow(clippy::cast_lossless, clippy::integer_division)]
      let timeout = timeval {
        tv_sec: i64::from(timeout_ms.wrapping_div(1000)),
        tv_usec: i64::from((timeout_ms % 1000).wrapping_mul(1000)),
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
