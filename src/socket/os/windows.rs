use crate::error::SocketError;
use crate::socket::{SocketAddr, SocketFlags};

use core::sync::atomic::{AtomicBool, Ordering};
use windows_sys::Win32::Foundation::TRUE;
use windows_sys::Win32::Networking::WinSock::{
  AF_INET, INVALID_SOCKET, IPPROTO_TCP, SD_BOTH, SO_KEEPALIVE, SO_RCVTIMEO, SO_REUSEADDR,
  SO_SNDTIMEO, SOCK_STREAM, SOCKADDR_IN, SOCKET, SOCKET_ERROR, SOL_SOCKET, TCP_NODELAY,
  WSADATA, WSAGetLastError, WSAStartup, closesocket, connect, recv, send, setsockopt,
  shutdown, socket,
};
use windows_sys::core::BOOL;

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
    10035 => SocketError::WouldBlock,
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

pub struct OsSocket {
  socket: SOCKET,
  connected: bool,
}

impl OsSocket {
  pub fn new() -> Result<Self, SocketError> {
    ensure_wsa_initialized()?;

    unsafe {
      let sock = socket(i32::from(AF_INET), SOCK_STREAM, IPPROTO_TCP);
      if sock == INVALID_SOCKET {
        return Err(get_last_wsa_error());
      }

      Ok(Self {
        socket: sock,
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
        crate::util::IpAddr::V6(ipv6) => Self::connect_ipv6(*ipv6, *port)?,
      },
      SocketAddr::Hostname {
        host,
        port: host_port,
      } => {
        let host_str =
          core::str::from_utf8(host).map_err(|_| SocketError::InvalidAddress)?;

        let addresses = crate::dns::os::resolve_host(host_str).map_err(|e| match e {
          crate::error::DnsError::ResolutionFailed(code) => {
            SocketError::DnsResolutionFailed(code)
          }
          crate::error::DnsError::NoAddressesFound => SocketError::DnsResolutionFailed(0),
          crate::error::DnsError::InvalidHostname => SocketError::InvalidAddress,
          crate::error::DnsError::Unsupported => SocketError::Unsupported,
          crate::error::DnsError::OsError(code) => SocketError::OsError(code),
        })?;

        let mut last_error = SocketError::ConnectionRefused;
        for ip_addr in &addresses {
          let result = match ip_addr {
            crate::util::IpAddr::V4(ipv4) => self.connect_ipv4(*ipv4, *host_port),
            crate::util::IpAddr::V6(ipv6) => Self::connect_ipv6(*ipv6, *host_port),
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
    let ip = u32::from_ne_bytes(addr);

    unsafe {
      let mut sockaddr: SOCKADDR_IN = core::mem::zeroed();
      sockaddr.sin_family = AF_INET;
      sockaddr.sin_port = port.to_be();
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

  const fn connect_ipv6(_addr: [u16; 8], _port: u16) -> Result<(), SocketError> {
    Err(SocketError::Unsupported)
  }

  pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, SocketError> {
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

  pub fn write(&mut self, buf: &[u8]) -> Result<usize, SocketError> {
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

  pub fn shutdown(&mut self) -> Result<(), SocketError> {
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

  pub fn set_flags(&mut self, flags: SocketFlags) -> Result<(), SocketError> {
    unsafe {
      if flags.contains(SocketFlags::TCP_NODELAY) {
        let val: BOOL = TRUE;
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let result = setsockopt(
          self.socket,
          IPPROTO_TCP,
          TCP_NODELAY,
          &raw const val as *const _,
          core::mem::size_of::<BOOL>() as i32,
        );
        if result == SOCKET_ERROR {
          return Err(get_last_wsa_error());
        }
      }

      if flags.contains(SocketFlags::KEEPALIVE) {
        let val: BOOL = TRUE;
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let result = setsockopt(
          self.socket,
          SOL_SOCKET,
          SO_KEEPALIVE,
          &raw const val as *const _,
          core::mem::size_of::<BOOL>() as i32,
        );
        if result == SOCKET_ERROR {
          return Err(get_last_wsa_error());
        }
      }

      if flags.contains(SocketFlags::REUSEADDR) {
        let val: BOOL = TRUE;
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let result = setsockopt(
          self.socket,
          SOL_SOCKET,
          SO_REUSEADDR,
          &raw const val as *const _,
          core::mem::size_of::<BOOL>() as i32,
        );
        if result == SOCKET_ERROR {
          return Err(get_last_wsa_error());
        }
      }
    }

    Ok(())
  }

  pub fn set_read_timeout(&mut self, timeout_ms: u32) -> Result<(), SocketError> {
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

  pub fn set_write_timeout(&mut self, timeout_ms: u32) -> Result<(), SocketError> {
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
