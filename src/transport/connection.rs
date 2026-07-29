use crate::config::Config;
use crate::dns::DnsResolver;
use crate::error::{Error, SocketError};
use crate::headers::{Headers, WellKnownHeader, well_known_header_bytes};
use crate::parser::chunked::{ChunkedDecoder, FeedResult};
use crate::parser::has_complete_headers;
use crate::parser::uri::{Host, Uri};
use crate::parser::version::Version;
use crate::parser::{BodyReadStrategy, Response};
use crate::socket::BlockingSocket;
use crate::transport::pool::PooledBuffers;
use crate::util::IpAddr;
use alloc::string::String;
use alloc::vec::Vec;
use bytes::{Bytes, BytesMut};
use core::net::SocketAddr;
use core::time::Duration;

/// Status line plus headers and body bytes, before client redirect/status policy.
#[derive(Debug, Clone)]
pub struct RawResponse {
  pub status_code: u16,
  pub reason: String,
  pub headers: Headers,
  pub version: Version,
  pub body_bytes: Bytes,
}

/// Live HTTP connection for raw send and receive.
pub struct Connection<'a, S> {
  socket: &'a mut S,
  max_header_size: usize,
  max_body_size: usize,
  reusable: bool,
  /// Response assemble buffer. Reused across reads on this connection when still
  /// uniquely owned (e.g. HEAD / empty body). After `freeze` into a body `Bytes`,
  /// the remnant may be shared — the next extend then reallocates (safe).
  buf: BytesMut,
  /// Socket `read` scratch; never frozen into `Bytes`, always reusable.
  scratch: Vec<u8>,
}

impl<'a, S: BlockingSocket> Connection<'a, S> {
  /// Fresh buffers (tests / non-pooled paths). Production hops use [`Self::with_buffers`].
  #[cfg_attr(not(test), allow(dead_code))]
  pub fn new(
    socket: &'a mut S,
    max_header_size: usize,
    max_body_size: usize,
  ) -> Self {
    Self::with_buffers(socket, max_header_size, max_body_size, PooledBuffers::default())
  }

  /// Like [`new`](Self::new), but reuses buffers taken from the connection pool.
  pub fn with_buffers(
    socket: &'a mut S,
    max_header_size: usize,
    max_body_size: usize,
    buffers: PooledBuffers,
  ) -> Self {
    Self {
      socket,
      max_header_size,
      max_body_size,
      reusable: true,
      buf: buffers.buf,
      scratch: buffers.scratch,
    }
  }

  /// Take receive buffers for return to the pool (lengths cleared; capacity kept).
  #[must_use]
  pub fn take_buffers(&mut self) -> PooledBuffers {
    self.buf.clear();
    PooledBuffers {
      buf: core::mem::replace(&mut self.buf, BytesMut::new()),
      scratch: core::mem::take(&mut self.scratch),
    }
  }

  /// Write the request octets to the socket.
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

  /// Read headers and body into a [`RawResponse`].
  ///
  /// When `expect_body` is false (HEAD), no entity body is read.
  /// 1xx interim responses are discarded until a final non-1xx response (RFC 9112 §15).
  pub fn read_raw_response(
    &mut self,
    expect_body: bool,
  ) -> Result<RawResponse, Error> {
    let max_header_size = self.max_header_size;
    let scratch_cap = max_header_size.min(8192);
    self.ensure_scratch(scratch_cap);
    // Start of a message: drop any leftover (should be empty on a reusable conn).
    self.buf.clear();
    if self.buf.capacity() < scratch_cap {
      self
        .buf
        .reserve(scratch_cap.saturating_sub(self.buf.capacity()));
    }

    loop {
      while !has_complete_headers(&self.buf) {
        if self.buf.len() > max_header_size {
          return Err(Error::ResponseHeaderTooLarge);
        }
        let n = self.read_socket_scratch()?;
        if n == 0 {
          break;
        }
        if let Some(slice) = self.scratch.get(..n) {
          self.buf.extend_from_slice(slice);
        }
        // Check after append even when this read completed headers (header section only —
        // body bytes past `\r\n\r\n` do not count toward the limit).
        if headers_section_len(&self.buf).is_some_and(|hdr_len| hdr_len > max_header_size)
          || (!has_complete_headers(&self.buf) && self.buf.len() > max_header_size)
        {
          return Err(Error::ResponseHeaderTooLarge);
        }
      }

      // Zero-copy scan: framing from borrowed refs, then materialize for RawResponse.
      let (status_code, reason_bytes, header_refs, version, remaining_after_headers) =
        Response::scan_headers_only(&self.buf).map_err(Error::Parse)?;
      let consumed = self.buf.len().saturating_sub(remaining_after_headers.len());

      // RFC 9112: discard 1xx and keep reading the final response
      if (100..200).contains(&status_code) {
        let _ = self.buf.split_to(consumed);
        continue;
      }

      let body_strategy = if expect_body {
        match Response::body_read_strategy_refs(&header_refs, status_code, version) {
          Ok(s) => Some(s),
          Err(e) => {
            self.reusable = false;
            return Err(Error::Parse(e));
          },
        }
      } else {
        None
      };

      let headers = Response::headers_from_refs(&header_refs);
      let reason = Response::reason_owned(reason_bytes);
      let _ = self.buf.split_to(consumed);

      let body_bytes = if let Some(strategy) = body_strategy {
        if matches!(strategy, BodyReadStrategy::UntilClose) {
          // UntilClose ends the connection; never pool it.
          self.reusable = false;
        }
        self.read_body(strategy)?
      } else {
        // HEAD / no-body: no entity body on the wire (RFC 9112); leftover = desync.
        if !self.buf.is_empty() {
          self.reusable = false;
          self.buf.clear();
        }
        Bytes::new()
      };

      // RFC 9112 §9.3 / §9.6: persistence from version + all Connection field lines
      if connection_option_present(&headers, "close") {
        self.reusable = false;
      } else if !version.defaults_to_persistent() {
        // HTTP/1.0 (and earlier): close unless keep-alive is present
        if !connection_option_present(&headers, "keep-alive") {
          self.reusable = false;
        }
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

  fn ensure_scratch(
    &mut self,
    cap: usize,
  ) {
    if self.scratch.len() < cap {
      self.scratch.resize(cap, 0);
    }
  }

  fn read_socket_scratch(&mut self) -> Result<usize, Error> {
    // Split borrows: `socket` + `scratch` are distinct fields.
    let Self { socket, scratch, .. } = self;
    match socket.read(scratch.as_mut_slice()) {
      Ok(n) => Ok(n),
      Err(e) => {
        // RFC 9112 Section 9.5: If timing out, implementation SHOULD issue a graceful close
        if e == SocketError::TimedOut {
          let _ = socket.shutdown();
        }
        Err(Error::Socket(e))
      },
    }
  }

  fn read_socket_into_scratch(
    &mut self,
    to_read: usize,
  ) -> Result<usize, Error> {
    let Self { socket, scratch, .. } = self;
    let Some(buf_slice) = scratch.get_mut(..to_read) else {
      return Ok(0);
    };
    match socket.read(buf_slice) {
      Ok(n) => Ok(n),
      Err(e) => {
        if e == SocketError::TimedOut {
          let _ = socket.shutdown();
        }
        Err(Error::Socket(e))
      },
    }
  }

  fn read_body(
    &mut self,
    strategy: BodyReadStrategy,
  ) -> Result<Bytes, Error> {
    let max_body = self.max_body_size;
    match strategy {
      BodyReadStrategy::NoBody => {
        if !self.buf.is_empty() {
          self.reusable = false;
          self.buf.clear();
        }
        Ok(Bytes::new())
      },
      BodyReadStrategy::ContentLength(len) => {
        if len > max_body {
          self.reusable = false;
          return Err(Error::BodyExceedsLimit(max_body));
        }
        if self.buf.len() > len {
          // Extra bytes past CL are a framing desync — do not reuse the connection.
          self.reusable = false;
        }
        let bytes_needed = len.saturating_sub(self.buf.len().min(len));
        if self.buf.len() > len {
          self.buf.truncate(len);
        }

        if bytes_needed > 0 {
          self.buf.reserve(bytes_needed);
          self.ensure_scratch(bytes_needed.min(8192));
          let mut bytes_read = 0usize;

          while bytes_read < bytes_needed {
            let to_read = (bytes_needed - bytes_read).min(self.scratch.len());
            let n = self.read_socket_into_scratch(to_read)?;
            if n == 0 {
              return Err(Error::Socket(SocketError::NotConnected));
            }
            if let Some(slice) = self.scratch.get(..n) {
              self.buf.extend_from_slice(slice);
            }
            bytes_read += n;
          }
        }

        // split_to + freeze: body `Bytes` may share the allocation; leftover `self.buf`
        // stays empty (and possibly shared). Next request reallocates if still shared.
        Ok(self.buf.split_to(len).freeze())
      },
      BodyReadStrategy::Chunked => {
        // Stateful feed + cursor: each wire byte is framed once (O(n)), no full-buffer
        // re-parse. Framing-only (`output: None`) — payload decode happens in the parser.
        if self.buf.len() > max_body {
          self.reusable = false;
          return Err(Error::BodyExceedsLimit(max_body));
        }
        let mut decoder = ChunkedDecoder::new();
        let mut cursor = 0usize;
        self.ensure_scratch(8192);

        let consumed = loop {
          let unread = self.buf.get(cursor..).unwrap_or(&[]);
          match decoder.feed(unread, None) {
            Ok(FeedResult::Done { rest }) => {
              let framed = unread.len().saturating_sub(rest.len());
              break cursor.saturating_add(framed);
            },
            Ok(FeedResult::NeedMore { consumed }) => {
              cursor = cursor.saturating_add(consumed);
            },
            Err(e) => {
              self.reusable = false;
              return Err(Error::Parse(e));
            },
          }

          let n = self.read_socket_scratch()?;
          if n == 0 {
            return Err(Error::Socket(SocketError::NotConnected));
          }
          if let Some(slice) = self.scratch.get(..n) {
            self.buf.extend_from_slice(slice);
          }
          if self.buf.len() > max_body {
            self.reusable = false;
            return Err(Error::BodyExceedsLimit(max_body));
          }
        };

        if consumed < self.buf.len() {
          // Bytes past the chunked message cannot be unread; do not pool.
          self.reusable = false;
        }
        Ok(self.buf.split_to(consumed).freeze())
      },
      BodyReadStrategy::UntilClose => {
        if self.buf.len() > max_body {
          self.reusable = false;
          return Err(Error::BodyExceedsLimit(max_body));
        }
        self.ensure_scratch(8192);

        loop {
          let n = self.read_socket_scratch()?;
          if n == 0 {
            break;
          }
          if let Some(slice) = self.scratch.get(..n) {
            self.buf.extend_from_slice(slice);
          }
          if self.buf.len() > max_body {
            self.reusable = false;
            return Err(Error::BodyExceedsLimit(max_body));
          }
        }

        Ok(self.buf.split().freeze())
      },
    }
  }

  /// Whether this connection may be returned to the pool.
  ///
  /// False after either side sent `Connection: close` (RFC 9112 §9.6).
  pub const fn is_reusable(&self) -> bool {
    self.reusable
  }
}

/// Connect to `uri`, or wrap a pooled already-connected socket.
///
/// When `reused` is true, skips DNS and connect. Prefer [`connect_with_buffers`]
/// when returning sockets to the idle pool.
///
/// # Errors
/// [`Error::InvalidUrl`], [`Error::Dns`], or [`Error::Socket`].
#[cfg_attr(not(test), allow(dead_code))]
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
  connect_with_buffers(socket, dns, uri, config, reused, PooledBuffers::default())
}

/// Like [`connect`], with receive buffers from the idle pool.
///
/// # Errors
/// [`Error::InvalidUrl`], [`Error::Dns`], or [`Error::Socket`].
pub fn connect_with_buffers<'a, S, D>(
  socket: &'a mut S,
  dns: &D,
  uri: &Uri,
  config: &Config,
  reused: bool,
  buffers: PooledBuffers,
) -> Result<Connection<'a, S>, Error>
where
  S: BlockingSocket,
  D: DnsResolver,
{
  if !reused {
    let authority = uri.authority().ok_or(Error::InvalidUrl)?;
    let port = uri.port_or_default();
    let host_for_sni = match authority.host() {
      Host::RegName(name) => String::from(*name),
      Host::IpAddr(addr) => crate::util::format_ip_for_host(*addr),
    };

    let addresses: Vec<IpAddr> = match authority.host() {
      Host::RegName(name) => dns.resolve(name).map_err(Error::Dns)?,
      Host::IpAddr(addr) => alloc::vec![*addr],
    };
    if addresses.is_empty() {
      return Err(Error::Dns(crate::error::DnsError::NoAddressesFound));
    }

    // Dedicated connect deadline (OS: nonblocking + poll/select). Never abuse write timeout.
    let connect_ms = duration_ms_u32(config.timeout_connect()).unwrap_or(0);
    socket
      .set_connect_timeout(connect_ms)
      .map_err(Error::Socket)?;

    let mut last_error = None;
    for addr in &addresses {
      match socket.connect(&SocketAddr::new(*addr, port), host_for_sni.as_str()) {
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
  }

  apply_io_timeouts(socket, config)?;
  Ok(Connection::with_buffers(
    socket,
    config.max_response_header_size(),
    config.max_response_body_size(),
    buffers,
  ))
}

fn apply_io_timeouts<S: BlockingSocket>(
  socket: &mut S,
  config: &Config,
) -> Result<(), Error> {
  // Always apply (including 0 = blocking) so a pooled socket cannot keep a prior
  // request's timeout when this request leaves timeout_* as None.
  let read_ms = duration_ms_u32(config.timeout_read()).unwrap_or(0);
  socket.set_read_timeout(read_ms).map_err(Error::Socket)?;
  let write_ms = duration_ms_u32(config.timeout_write()).unwrap_or(0);
  socket.set_write_timeout(write_ms).map_err(Error::Socket)?;
  Ok(())
}

fn duration_ms_u32(d: Option<Duration>) -> Option<u32> {
  // Overflow must not become “no timeout” (None → 0 blocking): saturate to u32::MAX.
  Some(u32::try_from(d?.as_millis()).unwrap_or(u32::MAX))
}

/// Length of the header section including the terminating blank line, if complete.
fn headers_section_len(data: &[u8]) -> Option<usize> {
  data
    .windows(4)
    .position(|w| w == b"\r\n\r\n")
    .map(|i| i + 4)
    .or_else(|| data.windows(2).position(|w| w == b"\n\n").map(|i| i + 2))
}

/// True if any `Connection` field line lists the given option (RFC 9110 list rules).
fn connection_option_present(
  headers: &Headers,
  option: &str,
) -> bool {
  headers
    .get_all(Headers::CONNECTION)
    .iter()
    .any(|v| v.split(',').any(|t| t.trim().eq_ignore_ascii_case(option)))
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
    if well_known_header_bytes(name) != Some(WellKnownHeader::Connection) {
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
