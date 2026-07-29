use crate::config::Config;
use crate::dns::DnsResolver;
use crate::error::{Error, SocketError};
use crate::headers::Headers;
use crate::parser::framing::has_complete_headers;
use crate::parser::uri::{Host, Uri};
use crate::parser::version::Version;
use crate::parser::{BodyReadStrategy, Response};
use crate::socket::BlockingSocket;
use crate::util::IpAddr;
use alloc::string::String;
use alloc::vec::Vec;
use core::net::SocketAddr;
use core::time::Duration;

/// Raw HTTP response without policy interpretation
#[derive(Debug, Clone)]
pub struct RawResponse {
  pub status_code: u16,
  pub reason: String,
  pub headers: Headers,
  pub version: Version,
  pub body_bytes: Vec<u8>,
}

/// A single live HTTP connection (policy-free I/O operations)
pub struct Connection<'a, S> {
  socket: &'a mut S,
  max_header_size: usize,
  reusable: bool,
}

impl<'a, S: BlockingSocket> Connection<'a, S> {
  pub const fn new(
    socket: &'a mut S,
    max_header_size: usize,
  ) -> Self {
    Self {
      socket,
      max_header_size,
      reusable: true,
    }
  }

  /// Send HTTP request bytes to the socket
  ///
  pub fn send_request(
    &mut self,
    request_bytes: &[u8],
  ) -> Result<(), Error> {
    let mut offset = 0usize;
    while offset < request_bytes.len() {
      let chunk = request_bytes
        .get(offset..)
        .ok_or(Error::Socket(SocketError::NotConnected))?;
      let n = self.socket.write(chunk).map_err(Error::Socket)?;
      if n == 0 {
        return Err(Error::Socket(SocketError::NotConnected));
      }
      offset = offset.saturating_add(n);
    }

    // RFC 9112 Section 9.6: If the client sends "Connection: close", it MUST NOT
    // send further requests on that connection.
    if request_has_connection_close(request_bytes) {
      self.reusable = false;
    }

    Ok(())
  }

  /// Read complete HTTP response (headers + body) with HTTP protocol awareness.
  ///
  /// When `expect_body` is false (HEAD), no entity body is read from the wire.
  /// 1xx interim responses are discarded; reading continues until a non-1xx
  /// final response (RFC 9112 §15).
  pub fn read_raw_response(
    &mut self,
    expect_body: bool,
  ) -> Result<RawResponse, Error> {
    let max_header_size = self.max_header_size;
    let mut buffer = alloc::vec![0u8; max_header_size.min(8192)];
    let mut header_buffer = Vec::new();

    loop {
      while !has_complete_headers(&header_buffer) {
        if header_buffer.len() > max_header_size {
          return Err(Error::ResponseHeaderTooLarge);
        }
        let n = self.read_socket(&mut buffer)?;
        if n == 0 {
          break;
        }
        if let Some(slice) = buffer.get(..n) {
          header_buffer.extend_from_slice(slice);
        }
        // Check after append even when this read completed headers (header section only —
        // body bytes past `\r\n\r\n` do not count toward the limit).
        if headers_section_len(&header_buffer).is_some_and(|hdr_len| hdr_len > max_header_size)
          || (!has_complete_headers(&header_buffer) && header_buffer.len() > max_header_size)
        {
          return Err(Error::ResponseHeaderTooLarge);
        }
      }

      let (status_code, reason, headers, version, remaining_after_headers) =
        Response::parse_headers_only(&header_buffer).map_err(Error::Parse)?;

      // RFC 9112: discard 1xx and keep reading the final response
      if (100..200).contains(&status_code) {
        header_buffer = remaining_after_headers.to_vec();
        continue;
      }

      let body_bytes = if expect_body {
        let body_strategy = match Response::body_read_strategy(&headers, status_code, version) {
          Ok(s) => s,
          Err(e) => {
            self.reusable = false;
            return Err(Error::Parse(e));
          },
        };
        if matches!(body_strategy, BodyReadStrategy::UntilClose) {
          // UntilClose ends the connection; never pool it.
          self.reusable = false;
        }
        self.read_body(body_strategy, remaining_after_headers)?
      } else {
        // HEAD / no-body: no entity body on the wire (RFC 9112); leftover = desync.
        if !remaining_after_headers.is_empty() {
          self.reusable = false;
        }
        Vec::new()
      };

      // RFC 9112 Section 9.3 / 9.6: HTTP/1.0 defaults to close unless keep-alive
      if version == Version::HTTP_10 {
        let keep_alive = headers.get(Headers::CONNECTION).is_some_and(|v| {
          v.split(',')
            .any(|t| t.trim().eq_ignore_ascii_case("keep-alive"))
        });
        if !keep_alive {
          self.reusable = false;
        }
      }

      // RFC 9112 Section 9.6: Connection header is a comma-separated token list
      if let Some(conn_value) = headers.get(Headers::CONNECTION)
        && conn_value
          .split(',')
          .any(|t| t.trim().eq_ignore_ascii_case("close"))
      {
        self.reusable = false;
      }

      return Ok(RawResponse {
        status_code,
        reason,
        headers,
        version,
        body_bytes,
      });
    }
  }

  fn read_socket(
    &mut self,
    buf: &mut [u8],
  ) -> Result<usize, Error> {
    match self.socket.read(buf) {
      Ok(n) => Ok(n),
      Err(e) => {
        // RFC 9112 Section 9.5: If timing out, implementation SHOULD issue a graceful close
        if e == SocketError::TimedOut {
          let _ = self.socket.shutdown();
        }
        Err(Error::Socket(e))
      },
    }
  }

  fn read_body(
    &mut self,
    strategy: BodyReadStrategy,
    initial_bytes: &[u8],
  ) -> Result<Vec<u8>, Error> {
    match strategy {
      BodyReadStrategy::NoBody => {
        if !initial_bytes.is_empty() {
          self.reusable = false;
        }
        Ok(Vec::new())
      },
      BodyReadStrategy::ContentLength(len) => {
        let mut body_bytes = if initial_bytes.len() > len {
          // Extra bytes past CL are a framing desync — do not reuse the connection.
          self.reusable = false;
          Vec::from(initial_bytes.get(..len).unwrap_or(&[]))
        } else {
          Vec::from(initial_bytes)
        };
        let bytes_needed = len.saturating_sub(body_bytes.len());

        if bytes_needed > 0 {
          let mut read_buffer = alloc::vec![0u8; bytes_needed.min(8192)];
          let mut bytes_read = 0usize;

          while bytes_read < bytes_needed {
            let to_read = (bytes_needed - bytes_read).min(read_buffer.len());
            if let Some(buf_slice) = read_buffer.get_mut(..to_read) {
              let n = self.read_socket(buf_slice)?;
              if n == 0 {
                return Err(Error::Socket(SocketError::NotConnected));
              }
              if let Some(slice) = read_buffer.get(..n) {
                body_bytes.extend_from_slice(slice);
              }
              bytes_read += n;
            }
          }
        }

        Ok(body_bytes)
      },
      BodyReadStrategy::Chunked => {
        let mut raw_bytes = Vec::from(initial_bytes);
        let mut chunk_buffer = alloc::vec![0u8; 8192];

        loop {
          // ponytail: read-stop heuristic; ChunkedDecoder is authoritative at parse time.
          if chunked_body_looks_complete(&raw_bytes) {
            break;
          }

          let n = self.read_socket(&mut chunk_buffer)?;
          if n == 0 {
            return Err(Error::Socket(SocketError::NotConnected));
          }
          if let Some(slice) = chunk_buffer.get(..n) {
            raw_bytes.extend_from_slice(slice);
          }
        }

        Ok(raw_bytes)
      },
      BodyReadStrategy::UntilClose => {
        let mut body_bytes = Vec::from(initial_bytes);
        let mut read_buffer = alloc::vec![0u8; 8192];

        loop {
          let n = self.read_socket(&mut read_buffer)?;
          if n == 0 {
            break;
          }
          if let Some(slice) = read_buffer.get(..n) {
            body_bytes.extend_from_slice(slice);
          }
        }

        Ok(body_bytes)
      },
    }
  }

  /// Check if the connection can be reused for another request
  ///
  /// RFC 9112 Section 9.6: Connection cannot be reused if either side sent Connection: close
  pub const fn is_reusable(&self) -> bool {
    self.reusable
  }
}

/// Establish a connection to the given URI, or prepare a pooled already-connected socket.
///
/// When `reused` is true, skips DNS and connect (socket came from the pool).
///
/// # Errors
/// Returns [`Error`] on invalid URL, DNS failure, or socket failure.
pub fn connect<'a, S, D>(
  socket: &'a mut S,
  dns: &D,
  uri: &Uri,
  config: &Config,
  reused: bool,
) -> Result<Connection<'a, S>, Error>
where
  S: BlockingSocket,
  D: DnsResolver,
{
  if !reused {
    let authority = uri.authority().ok_or(Error::InvalidUrl)?;
    let port = uri.port_or_default();

    let addresses: Vec<IpAddr> = match authority.host() {
      Host::RegName(name) => dns.resolve(name).map_err(Error::Dns)?,
      Host::IpAddr(addr) => alloc::vec![*addr],
    };
    if addresses.is_empty() {
      return Err(Error::Dns(crate::error::DnsError::NoAddressesFound));
    }

    let mut last_error = None;
    for addr in &addresses {
      if let Some(ms) = duration_ms_u32(config.timeout_connect) {
        socket.set_write_timeout(ms).map_err(Error::Socket)?;
      }

      match socket.connect(&SocketAddr::new(*addr, port)) {
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

    // Connect used SO_SNDTIMEO; don't leave it as the post-connect write timeout.
    if config.timeout_connect.is_some() && config.timeout_write.is_none() {
      socket.set_write_timeout(0).map_err(Error::Socket)?;
    }
  }

  if let Some(ms) = duration_ms_u32(config.timeout_read) {
    socket.set_read_timeout(ms).map_err(Error::Socket)?;
  }
  if let Some(ms) = duration_ms_u32(config.timeout_write) {
    socket.set_write_timeout(ms).map_err(Error::Socket)?;
  }

  Ok(Connection::new(socket, config.max_response_header_size))
}

fn duration_ms_u32(d: Option<Duration>) -> Option<u32> {
  let timeout_ms = d?.as_millis();
  if timeout_ms <= u128::from(u32::MAX) {
    #[allow(clippy::cast_possible_truncation)]
    Some(timeout_ms as u32)
  } else {
    None
  }
}

/// End-anchored heuristic for when to stop reading a chunked body.
fn chunked_body_looks_complete(data: &[u8]) -> bool {
  if data.ends_with(b"0\r\n\r\n") || data.ends_with(b"0\n\n") {
    return true;
  }
  if data.len() >= 5 && data.ends_with(b"\r\n\r\n") {
    return data.starts_with(b"0\r\n") || data.windows(4).any(|w| w == b"\n0\r\n");
  }
  if data.len() >= 3 && data.ends_with(b"\n\n") {
    return data.starts_with(b"0\n") || data.windows(3).any(|w| w == b"\n0\n");
  }
  false
}

/// Length of the header section including the terminating blank line, if complete.
fn headers_section_len(data: &[u8]) -> Option<usize> {
  data
    .windows(4)
    .position(|w| w == b"\r\n\r\n")
    .map(|i| i + 4)
    .or_else(|| data.windows(2).position(|w| w == b"\n\n").map(|i| i + 2))
}

/// True if a request's `Connection` header list contains the `close` token.
fn request_has_connection_close(request_bytes: &[u8]) -> bool {
  let headers = headers_section_len(request_bytes).map_or(request_bytes, |n| request_bytes.get(..n).unwrap_or(&[]));
  // Skip request-line; scan header fields only.
  let mut lines = headers.split(|&b| b == b'\n');
  let _ = lines.next();
  for raw_line in lines {
    let line = raw_line.strip_suffix(b"\r").unwrap_or(raw_line);
    if line.is_empty() {
      break;
    }
    let Some(colon) = line.iter().position(|&b| b == b':') else {
      continue;
    };
    let name = line.get(..colon).unwrap_or(&[]);
    if !name.eq_ignore_ascii_case(b"connection") {
      continue;
    }
    let value_bytes = line.get(colon + 1..).unwrap_or(&[]);
    let Ok(value) = core::str::from_utf8(value_bytes) else {
      continue;
    };
    if value
      .split(',')
      .any(|t| t.trim().eq_ignore_ascii_case("close"))
    {
      return true;
    }
  }
  false
}
