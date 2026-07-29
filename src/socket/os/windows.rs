use crate::error::SocketError;
use core::net::{SocketAddr, SocketAddrV4};
use core::sync::atomic::{AtomicBool, Ordering};
use windows_sys::Win32::Networking::WinSock::{
  AF_INET, INVALID_SOCKET, IPPROTO_TCP, SD_BOTH, SO_RCVTIMEO, SO_SNDTIMEO, SOCK_STREAM, SOCKADDR_IN, SOCKET,
  SOCKET_ERROR, SOL_SOCKET, WSADATA, WSAGetLastError, WSAStartup, closesocket, connect, recv, send, setsockopt,
  shutdown, socket,
};

static WSA_INITIALIZED: AtomicBool = AtomicBool::new(false);

fn ensure_wsa_initialized() -> Result<(), SocketError> {
  if WSA_INITIALIZED.load(Ordering::Acquire) {
    return Ok(());
  }

  unsafe {
    let mut wsa_data: WSADATA = core::mem::zeroed();
    let result = WSAStartup(0x0202, &raw mut wsa_data);
    if result != 0 {
      return Err(SocketError::OsError(result));
    }
    WSA_INITIALIZED.store(true, Ordering::Release);
  }

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
  unsafe {
    let code = WSAGetLastError();
    map_wsa_error(code)
  }
}

/// OS blocking TCP socket (`WinSock`).
pub struct OsSocket {
  socket: SOCKET,
  connected: bool,
  read_timeout_ms: Option<u32>,
  write_timeout_ms: Option<u32>,
}

impl crate::socket::BlockingSocket for OsSocket {
  fn new() -> Result<Self, SocketError> {
    ensure_wsa_initialized()?;
    Ok(Self {
      socket: INVALID_SOCKET,
      connected: false,
      read_timeout_ms: None,
      write_timeout_ms: None,
    })
  }

  fn connect(
    &mut self,
    addr: &SocketAddr,
  ) -> Result<(), SocketError> {
    if self.connected {
      return Ok(());
    }

    match addr {
      SocketAddr::V4(a) => self.connect_ipv4(a),
      // ponytail: WinSock IPv6 path not wired; DNS filters to V4 on Windows.
      SocketAddr::V6(_) => Err(SocketError::Unsupported),
    }
  }

  fn read(
    &mut self,
    buf: &mut [u8],
  ) -> Result<usize, SocketError> {
    if !self.connected {
      return Err(SocketError::NotConnected);
    }

    unsafe {
      #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
      let result = recv(self.socket, buf.as_mut_ptr() as *mut _, buf.len() as i32, 0);

      if result == SOCKET_ERROR {
        return Err(get_last_wsa_error());
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

  fn write(
    &mut self,
    buf: &[u8],
  ) -> Result<usize, SocketError> {
    if !self.connected {
      return Err(SocketError::NotConnected);
    }

    unsafe {
      #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
      let result = send(self.socket, buf.as_ptr() as *const _, buf.len() as i32, 0);

      if result == SOCKET_ERROR {
        return Err(get_last_wsa_error());
      }

      #[allow(clippy::cast_sign_loss)]
      {
        Ok(result as usize)
      }
    }
  }

  fn shutdown(&mut self) -> Result<(), SocketError> {
    if !self.connected {
      return Ok(());
    }

    unsafe {
      let result = shutdown(self.socket, SD_BOTH);
      if result == SOCKET_ERROR {
        let err = get_last_wsa_error();
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
  fn recreate(&mut self) -> Result<(), SocketError> {
    unsafe {
      if self.socket != INVALID_SOCKET {
        closesocket(self.socket);
        self.socket = INVALID_SOCKET;
      }
      let sock = socket(i32::from(AF_INET), SOCK_STREAM, IPPROTO_TCP);
      if sock == INVALID_SOCKET {
        return Err(get_last_wsa_error());
      }
      self.socket = sock;
    }
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
    self.recreate()?;

    let ip = u32::from_ne_bytes(addr.ip().octets());

    unsafe {
      let mut sockaddr: SOCKADDR_IN = core::mem::zeroed();
      sockaddr.sin_family = AF_INET;
      sockaddr.sin_port = addr.port().to_be();
      sockaddr.sin_addr.S_un.S_addr = ip;

      #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
      let result = connect(
        self.socket,
        &raw const sockaddr as *const _,
        core::mem::size_of::<SOCKADDR_IN>() as i32,
      );

      if result == SOCKET_ERROR {
        return Err(get_last_wsa_error());
      }
    }

    self.connected = true;
    Ok(())
  }

  fn apply_read_timeout(
    &mut self,
    timeout_ms: u32,
  ) -> Result<(), SocketError> {
    unsafe {
      #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
      let result = setsockopt(
        self.socket,
        SOL_SOCKET,
        SO_RCVTIMEO,
        &raw const timeout_ms as *const _,
        core::mem::size_of::<u32>() as i32,
      );
      if result == SOCKET_ERROR {
        return Err(get_last_wsa_error());
      }
    }
    Ok(())
  }

  fn apply_write_timeout(
    &mut self,
    timeout_ms: u32,
  ) -> Result<(), SocketError> {
    unsafe {
      #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
      let result = setsockopt(
        self.socket,
        SOL_SOCKET,
        SO_SNDTIMEO,
        &raw const timeout_ms as *const _,
        core::mem::size_of::<u32>() as i32,
      );
      if result == SOCKET_ERROR {
        return Err(get_last_wsa_error());
      }
    }
    Ok(())
  }
}

impl Drop for OsSocket {
  fn drop(&mut self) {
    if self.socket != INVALID_SOCKET {
      unsafe {
        closesocket(self.socket);
      }
    }
  }
}
