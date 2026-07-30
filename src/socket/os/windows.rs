use crate::error::SocketError;
use core::net::{SocketAddr, SocketAddrV4, SocketAddrV6};
use core::ptr;
use core::sync::atomic::{AtomicBool, Ordering};
use windows_sys::Win32::Networking::WinSock::{
  AF_INET, AF_INET6, FD_SET, FIONBIO, INVALID_SOCKET, IPPROTO_TCP, SD_BOTH, SO_ERROR, SO_RCVTIMEO, SO_SNDTIMEO,
  SOCK_STREAM, SOCKADDR_IN, SOCKADDR_IN6, SOCKET, SOCKET_ERROR, SOL_SOCKET, TIMEVAL, WSABUF, WSADATA, WSAGetLastError,
  WSASend, WSAStartup, closesocket, connect, getsockopt, ioctlsocket, recv, select, send, setsockopt, shutdown, socket,
};

static WSA_INITIALIZED: AtomicBool = AtomicBool::new(false);

fn ensure_wsa_initialized() -> Result<(), SocketError> {
  if WSA_INITIALIZED.load(Ordering::Acquire) {
    return Ok(());
  }

  // SAFETY: `WSAStartup` writes only into the local `WSADATA`; version `0x0202` is Winsock 2.2.
  let result = unsafe {
    let mut wsa_data: WSADATA = core::mem::zeroed();
    WSAStartup(0x0202, &raw mut wsa_data)
  };
  if result != 0 {
    return Err(SocketError::OsError(result));
  }
  WSA_INITIALIZED.store(true, Ordering::Release);

  Ok(())
}

const WSAEWOULDBLOCK: i32 = 10035;
const WSAEINPROGRESS: i32 = 10036;

const fn map_wsa_error(code: i32) -> SocketError {
  match code {
    10061 => SocketError::ConnectionRefused,
    10060 => SocketError::TimedOut,
    10004 => SocketError::Interrupted,
    10057 => SocketError::NotConnected,
    10022 => SocketError::InvalidAddress,
    _ => SocketError::OsError(code),
  }
}

fn get_last_wsa_error() -> SocketError {
  // SAFETY: `WSAGetLastError` reads thread-local Winsock error state set by the prior call.
  map_wsa_error(unsafe { WSAGetLastError() })
}

fn get_wsa_errno() -> i32 {
  // SAFETY: thread-local Winsock last-error after a failing Winsock call.
  unsafe { WSAGetLastError() }
}

/// Monotonic milliseconds for connect-deadline accounting (`GetTickCount64`).
fn monotonic_ms() -> u64 {
  // SAFETY: `GetTickCount64` is a process-wide tick counter; no pointers.
  unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount64() }
}

/// Cap a byte length for Winsock `i32` buffer APIs (`recv` / `send`).
fn winsock_buf_len(len: usize) -> i32 {
  i32::try_from(len).unwrap_or(i32::MAX)
}

/// OS blocking TCP socket (`WinSock`).
#[derive(Debug)]
pub struct OsSocket {
  socket: SOCKET,
  connected: bool,
  read_timeout_ms: Option<u32>,
  write_timeout_ms: Option<u32>,
  connect_timeout_ms: Option<u32>,
}

impl OsSocket {
  /// Create an unbound socket (initializes WinSock once).
  ///
  /// # Errors
  /// [`SocketError::OsError`] if `WSAStartup` fails.
  pub fn new() -> Result<Self, SocketError> {
    ensure_wsa_initialized()?;
    Ok(Self {
      socket: INVALID_SOCKET,
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
      SocketAddr::V4(a) => self.connect_ipv4(a),
      SocketAddr::V6(a) => self.connect_ipv6(a),
    }
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

    // Chunk so `len` never truncates / wraps negative via `as i32`.
    let chunk_len = winsock_buf_len(buf.len());
    let chunk_usize = usize::try_from(chunk_len).unwrap_or(0);
    let Some(chunk) = buf.get_mut(..chunk_usize) else {
      return Ok(0);
    };

    // SAFETY: `self.socket` is a live SOCKET while connected; `chunk` is a valid writable
    // buffer of `chunk_len` bytes for the duration of the call.
    let result = unsafe { recv(self.socket, chunk.as_mut_ptr().cast(), chunk_len, 0) };

    if result == SOCKET_ERROR {
      return Err(get_last_wsa_error());
    }

    if result == 0 {
      self.connected = false;
    }

    usize::try_from(result).map_err(|_| SocketError::OsError(result))
  }

  fn write(
    &mut self,
    buf: &[u8],
  ) -> Result<usize, SocketError> {
    if !self.connected {
      return Err(SocketError::NotConnected);
    }

    // Chunk so `len` never truncates / wraps negative via `as i32`.
    let chunk_len = winsock_buf_len(buf.len());
    let chunk_usize = usize::try_from(chunk_len).unwrap_or(0);
    let Some(chunk) = buf.get(..chunk_usize) else {
      return Ok(0);
    };

    // SAFETY: `self.socket` is a live SOCKET while connected; `chunk` is a valid readable
    // buffer of `chunk_len` bytes for the duration of the call.
    let result = unsafe { send(self.socket, chunk.as_ptr().cast(), chunk_len, 0) };

    if result == SOCKET_ERROR {
      return Err(get_last_wsa_error());
    }

    usize::try_from(result).map_err(|_| SocketError::OsError(result))
  }

  fn write_vectored(
    &mut self,
    bufs: &[&[u8]],
  ) -> Result<usize, SocketError> {
    if !self.connected {
      return Err(SocketError::NotConnected);
    }

    // Request send uses at most head + body (same shape as Unix `writev`).
    let mut wsabufs = [
      WSABUF {
        len: 0,
        buf: ptr::null_mut(),
      },
      WSABUF {
        len: 0,
        buf: ptr::null_mut(),
      },
    ];
    let mut count = 0usize;
    for buf in bufs {
      if buf.is_empty() {
        continue;
      }
      if count >= wsabufs.len() {
        break;
      }
      // `WSABUF.len` is `u32`; truncate like `send`'s `i32` cap on huge buffers.
      let chunk_len = u32::try_from(buf.len()).unwrap_or(u32::MAX);
      if let Some(slot) = wsabufs.get_mut(count) {
        slot.len = chunk_len;
        // Winsock takes `PSTR` (`*mut u8`) even for send; buffer is not written.
        slot.buf = buf.as_ptr().cast_mut();
        count = count.saturating_add(1);
      }
    }
    if count == 0 {
      return Ok(0);
    }

    loop {
      let mut bytes_sent: u32 = 0;
      // SAFETY: live SOCKET; `wsabufs[..count]` points at caller-borrowed readable
      // slices for the call; overlapped / completion are null (blocking socket).
      let result = unsafe {
        WSASend(
          self.socket,
          wsabufs.as_ptr(),
          u32::try_from(count).unwrap_or(u32::MAX),
          &raw mut bytes_sent,
          0,
          ptr::null_mut(),
          None,
        )
      };

      if result == SOCKET_ERROR {
        let err = get_last_wsa_error();
        if matches!(err, SocketError::Interrupted) {
          continue;
        }
        return Err(err);
      }

      return usize::try_from(bytes_sent).map_err(|_| SocketError::OsError(result));
    }
  }

  fn shutdown(&mut self) -> Result<(), SocketError> {
    if !self.connected {
      return Ok(());
    }

    // SAFETY: `self.socket` is a live SOCKET; `SD_BOTH` is a valid how value.
    let result = unsafe { shutdown(self.socket, SD_BOTH) };
    if result == SOCKET_ERROR {
      let err = get_last_wsa_error();
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
    if self.socket != INVALID_SOCKET {
      self.apply_read_timeout(timeout_ms)?;
    }
    Ok(())
  }

  fn set_write_timeout(
    &mut self,
    timeout_ms: u32,
  ) -> Result<(), SocketError> {
    self.write_timeout_ms = Some(timeout_ms);
    if self.socket != INVALID_SOCKET {
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
  /// Fresh SOCKET per attempt: failed connect leaves it unusable (same as Unix).
  fn recreate(
    &mut self,
    family: u16,
  ) -> Result<(), SocketError> {
    if self.socket != INVALID_SOCKET {
      // SAFETY: `self.socket` was obtained from `socket()` / prior recreate and is not INVALID.
      unsafe {
        closesocket(self.socket);
      }
      self.socket = INVALID_SOCKET;
    }
    // SAFETY: `family` is `AF_INET` or `AF_INET6`; type/protocol are valid TCP stream args.
    let sock = unsafe { socket(i32::from(family), SOCK_STREAM, IPPROTO_TCP) };
    if sock == INVALID_SOCKET {
      return Err(get_last_wsa_error());
    }
    self.socket = sock;
    if let Some(ms) = self.read_timeout_ms {
      self.apply_read_timeout(ms)?;
    }
    if let Some(ms) = self.write_timeout_ms {
      self.apply_write_timeout(ms)?;
    }
    Ok(())
  }

  fn set_nonblocking(
    &self,
    nonblocking: bool,
  ) -> Result<(), SocketError> {
    let mut mode: u32 = u32::from(nonblocking);
    // SAFETY: live SOCKET; `FIONBIO` with a valid `u32` mode pointer.
    let result = unsafe { ioctlsocket(self.socket, FIONBIO, &raw mut mode) };
    if result == SOCKET_ERROR {
      return Err(get_last_wsa_error());
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

      let mut write_fds = FD_SET {
        fd_count: 1,
        fd_array: [0; 64],
      };
      if let Some(slot) = write_fds.fd_array.get_mut(0) {
        *slot = self.socket;
      }
      let mut except_fds = write_fds;

      #[allow(
        clippy::cast_possible_wrap,
        clippy::cast_possible_truncation,
        clippy::integer_division
      )]
      let mut tv = TIMEVAL {
        tv_sec: (remaining / 1000) as i32,
        tv_usec: ((remaining % 1000) * 1000) as i32,
      };

      // SAFETY: `write_fds` / `except_fds` are initialized with one valid SOCKET;
      // `tv` is a valid timeout; nfds is ignored on Windows.
      let ready = unsafe { select(0, ptr::null_mut(), &raw mut write_fds, &raw mut except_fds, &raw mut tv) };
      if ready == SOCKET_ERROR {
        let err = get_last_wsa_error();
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

    let mut so_error: i32 = 0;
    let mut len = i32::try_from(core::mem::size_of::<i32>()).map_err(|_| SocketError::Unsupported)?;
    // SAFETY: live SOCKET; `so_error`/`len` are valid buffers for `SO_ERROR`.
    let result = unsafe {
      getsockopt(
        self.socket,
        SOL_SOCKET,
        SO_ERROR,
        &raw mut so_error as *mut _,
        &raw mut len,
      )
    };
    if result == SOCKET_ERROR {
      return Err(get_last_wsa_error());
    }
    if so_error != 0 {
      return Err(map_wsa_error(so_error));
    }
    Ok(())
  }

  fn connect_with_timeout(
    &mut self,
    addr: *const windows_sys::Win32::Networking::WinSock::SOCKADDR,
    namelen: i32,
  ) -> Result<(), SocketError> {
    let timeout_ms = self.connect_timeout_ms.unwrap_or(0);
    if timeout_ms == 0 {
      // SAFETY: live SOCKET; `addr` points at a valid sockaddr of `namelen` bytes.
      let result = unsafe { connect(self.socket, addr, namelen) };
      if result == SOCKET_ERROR {
        return Err(get_last_wsa_error());
      }
      self.connected = true;
      return Ok(());
    }

    self.set_nonblocking(true)?;
    // SAFETY: same as blocking path; nonblocking connect may return WSAEWOULDBLOCK.
    let result = unsafe { connect(self.socket, addr, namelen) };
    if result != SOCKET_ERROR {
      self.set_nonblocking(false)?;
      self.connected = true;
      return Ok(());
    }

    let errno = get_wsa_errno();
    if errno != WSAEWOULDBLOCK && errno != WSAEINPROGRESS {
      let _ = self.set_nonblocking(false);
      return Err(map_wsa_error(errno));
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
    self.recreate(AF_INET)?;

    let ip = u32::from_ne_bytes(addr.ip().octets());
    let namelen = i32::try_from(core::mem::size_of::<SOCKADDR_IN>()).map_err(|_| SocketError::Unsupported)?;

    // SAFETY: sockaddr is fully initialized POD; pointer valid for `namelen` during connect.
    let sockaddr = unsafe {
      let mut sockaddr: SOCKADDR_IN = core::mem::zeroed();
      sockaddr.sin_family = AF_INET;
      sockaddr.sin_port = addr.port().to_be();
      sockaddr.sin_addr.S_un.S_addr = ip;
      sockaddr
    };

    self.connect_with_timeout(&raw const sockaddr as *const _, namelen)
  }

  fn connect_ipv6(
    &mut self,
    addr: &SocketAddrV6,
  ) -> Result<(), SocketError> {
    self.recreate(AF_INET6)?;

    let namelen = i32::try_from(core::mem::size_of::<SOCKADDR_IN6>()).map_err(|_| SocketError::Unsupported)?;

    // SAFETY: sockaddr is fully initialized POD; pointer valid for `namelen` during connect.
    let sockaddr = unsafe {
      let mut sockaddr: SOCKADDR_IN6 = core::mem::zeroed();
      sockaddr.sin6_family = AF_INET6;
      sockaddr.sin6_port = addr.port().to_be();
      sockaddr.sin6_flowinfo = 0;
      sockaddr.sin6_addr.u.Byte = addr.ip().octets();
      sockaddr.Anonymous.sin6_scope_id = addr.scope_id();
      sockaddr
    };

    self.connect_with_timeout(&raw const sockaddr as *const _, namelen)
  }

  fn apply_read_timeout(
    &mut self,
    timeout_ms: u32,
  ) -> Result<(), SocketError> {
    let optlen = i32::try_from(core::mem::size_of::<u32>()).map_err(|_| SocketError::Unsupported)?;
    // SAFETY: socket live; `timeout_ms` is a valid `SO_RCVTIMEO` DWORD; pointer valid for optlen.
    let result = unsafe {
      setsockopt(
        self.socket,
        SOL_SOCKET,
        SO_RCVTIMEO,
        &raw const timeout_ms as *const _,
        optlen,
      )
    };
    if result == SOCKET_ERROR {
      return Err(get_last_wsa_error());
    }
    Ok(())
  }

  fn apply_write_timeout(
    &mut self,
    timeout_ms: u32,
  ) -> Result<(), SocketError> {
    let optlen = i32::try_from(core::mem::size_of::<u32>()).map_err(|_| SocketError::Unsupported)?;
    // SAFETY: socket live; `timeout_ms` is a valid `SO_SNDTIMEO` DWORD; pointer valid for optlen.
    let result = unsafe {
      setsockopt(
        self.socket,
        SOL_SOCKET,
        SO_SNDTIMEO,
        &raw const timeout_ms as *const _,
        optlen,
      )
    };
    if result == SOCKET_ERROR {
      return Err(get_last_wsa_error());
    }
    Ok(())
  }
}

impl Drop for OsSocket {
  fn drop(&mut self) {
    if self.socket != INVALID_SOCKET {
      // SAFETY: socket was created by Winsock and not yet closed.
      unsafe {
        closesocket(self.socket);
      }
    }
  }
}
