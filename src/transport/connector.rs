use crate::config::Config;
use crate::dns::DnsResolver;
use crate::error::Error;
use crate::parser::uri::{Host, Uri};
use crate::socket::{BlockingSocket, SocketAddr};
use crate::transport::connection::Connection;
use crate::util::IpAddr;
use alloc::vec::Vec;

/// Handles DNS resolution and socket connection setup
pub struct Connector<'a, S, D> {
  socket: &'a mut S,
  dns: &'a D,
}

impl<'a, S, D> Connector<'a, S, D>
where
  S: BlockingSocket,
  D: DnsResolver,
{
  pub const fn new(
    socket: &'a mut S,
    dns: &'a D,
  ) -> Self {
    Self { socket, dns }
  }

  /// Establish a connection to the given URI.
  ///
  /// Performs DNS resolution when the host is a name, connects literal IPs directly,
  /// and applies timeout configuration. Tries each resolved address until one connects.
  ///
  /// # Errors
  /// Returns [`Error`] on invalid URL, DNS failure, or socket failure.
  pub fn connect(
    self,
    uri: &Uri,
    config: &Config,
  ) -> Result<Connection<'a, S>, Error> {
    let authority = uri.authority().ok_or(Error::InvalidUrl)?;
    let port = authority.port().unwrap_or_else(|| {
      if uri.scheme().eq_ignore_ascii_case("https") {
        443
      } else {
        80
      }
    });

    let addresses: Vec<IpAddr> = match authority.host() {
      Host::RegName(name) => self.dns.resolve(name).map_err(Error::Dns)?,
      Host::IpAddr(addr) => alloc::vec![*addr],
    };
    if addresses.is_empty() {
      return Err(Error::NoAddresses);
    }

    let mut last_error = None;
    for addr in &addresses {
      if let Some(timeout_connect) = config.timeout_connect {
        let timeout_ms = timeout_connect.as_millis();
        if timeout_ms <= u128::from(u32::MAX) {
          #[allow(clippy::cast_possible_truncation)]
          let timeout_u32 = timeout_ms as u32;
          self
            .socket
            .set_write_timeout(timeout_u32)
            .map_err(Error::Socket)?;
        }
      }

      let socket_addr = SocketAddr::Ip { addr: *addr, port };
      match self.socket.connect(&socket_addr) {
        Ok(()) => {
          last_error = None;
          break;
        },
        Err(e) => last_error = Some(e),
      }
    }
    if let Some(e) = last_error {
      return Err(Error::Socket(e));
    }

    if let Some(timeout_read) = config.timeout_read {
      let timeout_ms = timeout_read.as_millis();
      if timeout_ms <= u128::from(u32::MAX) {
        #[allow(clippy::cast_possible_truncation)]
        let timeout_u32 = timeout_ms as u32;
        self
          .socket
          .set_read_timeout(timeout_u32)
          .map_err(Error::Socket)?;
      }
    } else if let Some(timeout) = config.timeout {
      let timeout_ms = timeout.as_millis();
      if timeout_ms <= u128::from(u32::MAX) {
        #[allow(clippy::cast_possible_truncation)]
        let timeout_u32 = timeout_ms as u32;
        self
          .socket
          .set_read_timeout(timeout_u32)
          .map_err(Error::Socket)?;
        self
          .socket
          .set_write_timeout(timeout_u32)
          .map_err(Error::Socket)?;
      }
    }

    Ok(Connection::new(self.socket, config.max_response_header_size))
  }
}
