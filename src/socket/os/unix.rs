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

fn get_errno() -> c_int {
  // SAFETY: `__errno_location` / `__error` return a valid thread-local errno pointer.
  unsafe {
    #[cfg(target_os = "macos")]
    {
      *libc::__error()
    }
    #[cfg(not(target_os = "macos"))]
    {
      *libc::__errno_location()
    }
  }
}

fn get_last_error() -> SocketError {
  map_errno(get_errno())
}

/// Monotonic milliseconds for connect-deadline accounting (`CLOCK_MONOTONIC`).
fn monotonic_ms() -> u64 {
  let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
  // SAFETY: `ts` is a valid writable `timespec`; `CLOCK_MONOTONIC` is defined on supported unix.
  let result = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &raw mut ts) };
  if result != 0 {
    return 0;
  }
  let secs = u64::try_from(ts.tv_sec).unwrap_or(0);
  #[allow(clippy::integer_division)]
  let millis = u64::try_from(ts.tv_nsec / 1_000_000).unwrap_or(0);
  secs.saturating_mul(1000).saturating_add(millis)
}

/// OS blocking TCP socket (BSD sockets).
#[derive(Debug)]
pub struct OsSocket {
  fd: c_int,
  connected: bool,
  read_timeout_ms: Option<u32>,
  write_timeout_ms: Option<u32>,
  connect_timeout_ms: Option<u32>,
}

impl OsSocket {
  /// Create an unbound socket handle (fd allocated on connect).
  ///
  /// # Errors
  /// Infallible on unix (Windows `new` may fail).
  pub const fn new() -> Result<Self, SocketError> {
    Ok(Self {
      fd: -1,
      connected: false,
      read_timeout_ms: None,
      write_timeout_ms: None,
      connect_timeout_ms: None,
    })
  }
}

impl crate::socket::BlockingSocketFactory for OsSocket {
  fn new() -> Result<Self, SocketError> {
    Self::new()
  }
}

impl crate::socket::BlockingSocket for OsSocket {
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
      // SAFETY: `self.fd` is an open socket fd while connected; `buf` is a valid writable
      // region of `buf.len()` bytes for the duration of the call.
      let result = unsafe { libc::read(self.fd, buf.as_mut_ptr().cast::<c_void>(), buf.len()) };

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

      return usize::try_from(result).map_err(|_| SocketError::Unsupported);
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
      // SAFETY: `self.fd` is an open socket fd while connected; `buf` is a valid readable
      // region of `buf.len()` bytes for the duration of the call.
      let result = unsafe { libc::write(self.fd, buf.as_ptr().cast::<c_void>(), buf.len()) };

      if result < 0 {
        let err = get_last_error();
        if matches!(err, SocketError::Interrupted) {
          continue;
        }
        return Err(err);
      }

      return usize::try_from(result).map_err(|_| SocketError::Unsupported);
    }
  }

  fn write_vectored(
    &mut self,
    bufs: &[&[u8]],
  ) -> Result<usize, SocketError> {
    if !self.connected {
      return Err(SocketError::NotConnected);
    }

    // Request send uses at most head + body.
    let mut iov = [libc::iovec {
      iov_base: core::ptr::null_mut(),
      iov_len: 0,
    }; 2];
    let mut count = 0usize;
    for buf in bufs {
      if buf.is_empty() {
        continue;
      }
      if count >= iov.len() {
        break;
      }
      if let Some(slot) = iov.get_mut(count) {
        slot.iov_base = buf.as_ptr().cast::<c_void>().cast_mut();
        slot.iov_len = buf.len();
        count = count.saturating_add(1);
      }
    }
    if count == 0 {
      return Ok(0);
    }

    loop {
      // SAFETY: `self.fd` is connected; `iov[..count]` points at caller-borrowed
      // readable slices for the duration of `writev`.
      let result = unsafe {
        libc::writev(
          self.fd,
          iov.as_ptr(),
          c_int::try_from(count).unwrap_or(c_int::MAX),
        )
      };

      if result < 0 {
        let err = get_last_error();
        if matches!(err, SocketError::Interrupted) {
          continue;
        }
        return Err(err);
      }

      return usize::try_from(result).map_err(|_| SocketError::Unsupported);
    }
  }

  fn shutdown(&mut self) -> Result<(), SocketError> {
    if !self.connected {
      return Ok(());
    }

    // SAFETY: `self.fd` is an open socket; `SHUT_RDWR` is a valid how value.
    let result = unsafe { libc::shutdown(self.fd, libc::SHUT_RDWR) };
    if result < 0 {
      let err = get_last_error();
      if !matches!(err, SocketError::NotConnected) {
        return Err(err);
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

  fn set_connect_timeout(
    &mut self,
    timeout_ms: u32,
  ) -> Result<(), SocketError> {
    self.connect_timeout_ms = Some(timeout_ms);
    Ok(())
  }
}

impl OsSocket {
  /// Fresh fd per attempt: failed connect leaves the socket unusable on Linux.
  fn recreate(
    &mut self,
    family: c_int,
  ) -> Result<(), SocketError> {
    if self.fd >= 0 {
      // SAFETY: `self.fd` was returned by `socket()` / prior recreate and is still open.
      unsafe {
        libc::close(self.fd);
      }
      self.fd = -1;
    }
    // SAFETY: `family` is `AF_INET` or `AF_INET6`; type/protocol are valid TCP stream args.
    let fd = unsafe { libc::socket(family, libc::SOCK_STREAM, libc::IPPROTO_TCP) };
    if fd < 0 {
      return Err(get_last_error());
    }
    self.fd = fd;
    if let Some(ms) = self.read_timeout_ms {
      self.apply_read_timeout(ms)?;
    }
    if let Some(ms) = self.write_timeout_ms {
      self.apply_write_timeout(ms)?;
    }
    Ok(())
  }

  const fn timeval_from_ms(timeout_ms: u32) -> timeval {
    #[allow(
      clippy::cast_lossless,
      clippy::integer_division,
      clippy::cast_possible_wrap,
      clippy::cast_possible_truncation
    )]
    timeval {
      tv_sec: (timeout_ms.wrapping_div(1000)) as _,
      tv_usec: ((timeout_ms % 1000).wrapping_mul(1000)) as _,
    }
  }

  fn apply_read_timeout(
    &mut self,
    timeout_ms: u32,
  ) -> Result<(), SocketError> {
    let timeout = Self::timeval_from_ms(timeout_ms);
    let optlen = socklen_t::try_from(core::mem::size_of::<timeval>()).map_err(|_| SocketError::Unsupported)?;

    // SAFETY: `self.fd` open; `timeout` is a valid `timeval` for `SO_RCVTIMEO`; pointer live.
    let result = unsafe {
      libc::setsockopt(
        self.fd,
        libc::SOL_SOCKET,
        libc::SO_RCVTIMEO,
        &raw const timeout as *const c_void,
        optlen,
      )
    };

    if result < 0 {
      return Err(get_last_error());
    }
    Ok(())
  }

  fn apply_write_timeout(
    &mut self,
    timeout_ms: u32,
  ) -> Result<(), SocketError> {
    let timeout = Self::timeval_from_ms(timeout_ms);
    let optlen = socklen_t::try_from(core::mem::size_of::<timeval>()).map_err(|_| SocketError::Unsupported)?;

    // SAFETY: `self.fd` open; `timeout` is a valid `timeval` for `SO_SNDTIMEO`; pointer live.
    let result = unsafe {
      libc::setsockopt(
        self.fd,
        libc::SOL_SOCKET,
        libc::SO_SNDTIMEO,
        &raw const timeout as *const c_void,
        optlen,
      )
    };

    if result < 0 {
      return Err(get_last_error());
    }
    Ok(())
  }

  fn set_nonblocking(
    &self,
    nonblocking: bool,
  ) -> Result<(), SocketError> {
    // SAFETY: `self.fd` is an open socket; `F_GETFL` / `F_SETFL` are valid for sockets.
    let flags = unsafe { libc::fcntl(self.fd, libc::F_GETFL) };
    if flags < 0 {
      return Err(get_last_error());
    }
    let new_flags = if nonblocking {
      flags | libc::O_NONBLOCK
    } else {
      flags & !libc::O_NONBLOCK
    };
    // SAFETY: same fd; `new_flags` is a valid flag word for this socket.
    let result = unsafe { libc::fcntl(self.fd, libc::F_SETFL, new_flags) };
    if result < 0 {
      return Err(get_last_error());
    }
    Ok(())
  }

  fn wait_until_connected(
    &self,
    timeout_ms: u32,
  ) -> Result<(), SocketError> {
    let start = monotonic_ms();
    let total_ms = u64::from(timeout_ms);
    loop {
      let elapsed = monotonic_ms().saturating_sub(start);
      let remaining = total_ms.saturating_sub(elapsed);
      if remaining == 0 {
        return Err(SocketError::TimedOut);
      }
      let timeout = c_int::try_from(remaining).unwrap_or(c_int::MAX);
      let mut pfd = libc::pollfd {
        fd: self.fd,
        events: libc::POLLOUT,
        revents: 0,
      };
      // SAFETY: `pfd` is a single valid `pollfd`; `nfds` is 1; timeout is non-negative ms.
      let ready = unsafe { libc::poll(&raw mut pfd, 1, timeout) };
      if ready < 0 {
        let err = get_last_error();
        if matches!(err, SocketError::Interrupted) {
          // Shrink remaining via `start`/`elapsed` on the next iteration.
          continue;
        }
        return Err(err);
      }
      if ready == 0 {
        return Err(SocketError::TimedOut);
      }
      break;
    }

    let mut so_error: c_int = 0;
    let mut len = socklen_t::try_from(core::mem::size_of::<c_int>()).map_err(|_| SocketError::Unsupported)?;
    // SAFETY: open fd; `so_error`/`len` are valid buffers for `SO_ERROR`.
    let result = unsafe {
      libc::getsockopt(
        self.fd,
        libc::SOL_SOCKET,
        libc::SO_ERROR,
        &raw mut so_error as *mut c_void,
        &raw mut len,
      )
    };
    if result < 0 {
      return Err(get_last_error());
    }
    if so_error != 0 {
      return Err(map_errno(so_error));
    }
    Ok(())
  }

  fn connect_with_timeout(
    &mut self,
    addr: *const sockaddr,
    addrlen: socklen_t,
  ) -> Result<(), SocketError> {
    let timeout_ms = self.connect_timeout_ms.unwrap_or(0);
    if timeout_ms == 0 {
      // SAFETY: `self.fd` open; `addr` points at a valid sockaddr of `addrlen` bytes.
      let result = unsafe { libc::connect(self.fd, addr, addrlen) };
      if result < 0 {
        return Err(get_last_error());
      }
      self.connected = true;
      return Ok(());
    }

    self.set_nonblocking(true)?;
    // SAFETY: same as blocking path; nonblocking connect may return EINPROGRESS.
    let result = unsafe { libc::connect(self.fd, addr, addrlen) };
    if result == 0 {
      self.set_nonblocking(false)?;
      self.connected = true;
      return Ok(());
    }

    let errno = get_errno();
    if errno != libc::EINPROGRESS && errno != libc::EALREADY && errno != libc::EWOULDBLOCK {
      let _ = self.set_nonblocking(false);
      return Err(map_errno(errno));
    }

    match self.wait_until_connected(timeout_ms) {
      Ok(()) => {
        self.set_nonblocking(false)?;
        self.connected = true;
        Ok(())
      },
      Err(e) => {
        let _ = self.set_nonblocking(false);
        Err(e)
      },
    }
  }

  fn connect_ipv4(
    &mut self,
    addr: &SocketAddrV4,
  ) -> Result<(), SocketError> {
    self.recreate(libc::AF_INET)?;

    let addrlen = socklen_t::try_from(core::mem::size_of::<sockaddr_in>()).map_err(|_| SocketError::Unsupported)?;

    // SAFETY: sockaddr fully initialized POD; pointer valid for `addrlen` during `connect`.
    let sockaddr = unsafe {
      let mut sockaddr: sockaddr_in = core::mem::zeroed();
      #[allow(clippy::cast_possible_truncation)]
      {
        sockaddr.sin_family = libc::AF_INET as _;
      }
      sockaddr.sin_port = addr.port().to_be();
      sockaddr.sin_addr.s_addr = u32::from_ne_bytes(addr.ip().octets());
      sockaddr
    };

    self.connect_with_timeout(&raw const sockaddr as *const sockaddr, addrlen)
  }

  fn connect_ipv6(
    &mut self,
    addr: &SocketAddrV6,
  ) -> Result<(), SocketError> {
    self.recreate(libc::AF_INET6)?;

    let addrlen = socklen_t::try_from(core::mem::size_of::<sockaddr_in6>()).map_err(|_| SocketError::Unsupported)?;

    // SAFETY: sockaddr fully initialized POD; pointer valid for `addrlen` during `connect`.
    let sockaddr = unsafe {
      let mut sockaddr: sockaddr_in6 = core::mem::zeroed();
      #[allow(clippy::cast_possible_truncation)]
      {
        sockaddr.sin6_family = libc::AF_INET6 as _;
      }
      sockaddr.sin6_port = addr.port().to_be();
      sockaddr.sin6_addr.s6_addr = addr.ip().octets();
      sockaddr
    };

    self.connect_with_timeout(&raw const sockaddr as *const sockaddr, addrlen)
  }
}

impl Drop for OsSocket {
  fn drop(&mut self) {
    if self.fd >= 0 {
      // SAFETY: fd was created by `socket()` and not yet closed.
      unsafe {
        libc::close(self.fd);
      }
    }
  }
}
