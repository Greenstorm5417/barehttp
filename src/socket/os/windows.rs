use crate::error::SocketError;
use core::net::{SocketAddr, SocketAddrV4, SocketAddrV6};
use core::sync::atomic::{AtomicBool, Ordering};
use windows_sys::Win32::Networking::WinSock::{
  AF_INET, AF_INET6, INVALID_SOCKET, IPPROTO_TCP, SD_BOTH, SO_RCVTIMEO, SO_SNDTIMEO, SOCK_STREAM, SOCKADDR_IN,
  SOCKADDR_IN6, SOCKET, SOCKET_ERROR, SOL_SOCKET, WSADATA, WSAGetLastError, WSAStartup, closesocket, connect, recv,
  send, setsockopt, shutdown, socket,
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

  fn connect_ipv4(
    &mut self,
    addr: &SocketAddrV4,
  ) -> Result<(), SocketError> {
    self.recreate(AF_INET)?;

    let ip = u32::from_ne_bytes(addr.ip().octets());
    let namelen = i32::try_from(core::mem::size_of::<SOCKADDR_IN>()).map_err(|_| SocketError::Unsupported)?;

    // SAFETY: sockaddr is fully initialized POD; pointer valid for `namelen` bytes; socket live.
    let result = unsafe {
      let mut sockaddr: SOCKADDR_IN = core::mem::zeroed();
      sockaddr.sin_family = AF_INET;
      sockaddr.sin_port = addr.port().to_be();
      sockaddr.sin_addr.S_un.S_addr = ip;

      connect(self.socket, &raw const sockaddr as *const _, namelen)
    };

    if result == SOCKET_ERROR {
      return Err(get_last_wsa_error());
    }

    self.connected = true;
    Ok(())
  }

  fn connect_ipv6(
    &mut self,
    addr: &SocketAddrV6,
  ) -> Result<(), SocketError> {
    self.recreate(AF_INET6)?;

    let namelen = i32::try_from(core::mem::size_of::<SOCKADDR_IN6>()).map_err(|_| SocketError::Unsupported)?;

    // SAFETY: sockaddr is fully initialized POD (union Byte / scope_id views written below);
    // pointer valid for `namelen` bytes; socket live.
    let result = unsafe {
      let mut sockaddr: SOCKADDR_IN6 = core::mem::zeroed();
      sockaddr.sin6_family = AF_INET6;
      sockaddr.sin6_port = addr.port().to_be();
      sockaddr.sin6_flowinfo = 0;
      sockaddr.sin6_addr.u.Byte = addr.ip().octets();
      sockaddr.Anonymous.sin6_scope_id = addr.scope_id();

      connect(self.socket, &raw const sockaddr as *const _, namelen)
    };

    if result == SOCKET_ERROR {
      return Err(get_last_wsa_error());
    }

    self.connected = true;
    Ok(())
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
