use crate::config::Config;
use crate::dns::DnsResolver;
use crate::error::{Error, SocketError};
use crate::headers::{Headers, WellKnownHeader, well_known_header_bytes};
use crate::parser::chunked::{ChunkedDecoder, FeedResult};
use crate::parser::uri::{Host, Uri};
use crate::parser::version::Version;
use crate::parser::{BodyReadStrategy, Response};
use crate::parser::{has_complete_headers, header_section_end};
use crate::socket::BlockingSocket;
use crate::transport::pool::PooledBuffers;
use crate::util::IpAddr;
use alloc::borrow::Cow;
use alloc::vec::Vec;
use bytes::{Bytes, BytesMut};
use core::net::SocketAddr;
use core::time::Duration;

/// Status line plus headers and body bytes, before client redirect/status policy.
#[derive(Debug, Clone)]
pub struct RawResponse {
  pub status_code: u16,
  pub reason: compact_str::CompactString,
  pub headers: Headers,
  pub version: Version,
  pub body_bytes: Bytes,
  /// When `Some`, [`Self::body_bytes`] is already the decoded chunked payload and
  /// this holds trailer fields (empty if the message had none). When `None`,
  /// `body_bytes` is still strategy wire form (Content-Length / until-close /
  /// offline chunked parse via the response parser).
  pub decoded_chunked_trailers: Option<Headers>,
}

/// Live HTTP connection for raw send and receive.
pub struct Connection<'a, S> {
  socket: &'a mut S,
  max_header_size: usize,
  max_body_size: usize,
  reusable: bool,
  /// Response assemble buffer. Reused across reads on this connection when still
  /// uniquely owned (e.g. HEAD / empty body). After `freeze` into header or body
  /// `Bytes`, the remnant may be shared; the next reserve then reallocates (safe).
  /// Socket bytes are read directly into spare capacity (no intermediate copy).
  buf: BytesMut,
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
    }
  }

  /// Take receive buffers for return to the pool (lengths cleared; capacity kept).
  #[must_use]
  pub fn take_buffers(&mut self) -> PooledBuffers {
    self.buf.clear();
    PooledBuffers {
      buf: core::mem::replace(&mut self.buf, BytesMut::new()),
    }
  }

  /// Write request head then body without concatenating into one buffer.
  ///
  /// Uses [`BlockingSocket::write_vectored`] when the adapter supports it
  /// (cleartext OS TCP); adapters that only implement [`BlockingSocket::write`]
  /// fall back to sequential partial writes (still no head+body copy).
  pub fn send_request(
    &mut self,
    head: &[u8],
    body: &[u8],
  ) -> Result<(), Error> {
    self.write_all_vectored(&[head, body])?;

    // RFC 9112 Section 9.6: If the client sends "Connection: close", it MUST NOT
    // send further requests on that connection. Scan header block only.
    if request_has_connection_close(head) {
      self.reusable = false;
    }

    Ok(())
  }

  /// Write all of `bufs` in order, advancing across buffers on short writes.
  fn write_all_vectored(
    &mut self,
    bufs: &[&[u8]],
  ) -> Result<(), Error> {
    let mut idx = 0usize;
    let mut off = 0usize;
    loop {
      while idx < bufs.len() {
        let Some(cur) = bufs.get(idx).copied() else {
          return Ok(());
        };
        if off < cur.len() {
          break;
        }
        idx = idx.saturating_add(1);
        off = 0;
      }
      if idx >= bufs.len() {
        return Ok(());
      }

      let Some(cur) = bufs.get(idx).copied() else {
        return Ok(());
      };
      let first = cur.get(off..).unwrap_or(&[]);
      let second = bufs.get(idx.saturating_add(1)).copied().unwrap_or(&[]);
      let n = if second.is_empty() {
        self.socket.write(first).map_err(Error::Socket)?
      } else {
        self
          .socket
          .write_vectored(&[first, second])
          .map_err(Error::Socket)?
      };
      if n == 0 {
        return Err(Error::Socket(SocketError::NotConnected));
      }

      let mut remaining = n;
      while remaining > 0 {
        let Some(slice) = bufs.get(idx).copied() else {
          break;
        };
        let avail = slice.len().saturating_sub(off);
        if remaining < avail {
          off = off.saturating_add(remaining);
          remaining = 0;
        } else {
          remaining = remaining.saturating_sub(avail);
          idx = idx.saturating_add(1);
          off = 0;
        }
      }
    }
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
    let chunk_cap = max_header_size.min(8192);
    // Start of a message: drop any leftover (should be empty on a reusable conn).
    self.buf.clear();
    // Header growth is capped by `max_header_size` (see loop below). Prefetch a
    // modest chunk capacity only. Never reserve the full max header budget up front.
    if self.buf.capacity() < chunk_cap {
      self
        .buf
        .reserve(chunk_cap.saturating_sub(self.buf.capacity()));
    }

    loop {
      while !has_complete_headers(&self.buf) {
        if self.buf.len() > max_header_size {
          return Err(Error::ResponseHeaderTooLarge);
        }
        let n = self.read_socket_into_buf(chunk_cap)?;
        if n == 0 {
          // Peer closed before a complete header section: Socket NotConnected / EOF.
          // A partial status line surfaces as InvalidHttpVersion.
          return Err(Error::Socket(SocketError::NotConnected));
        }
        // Check after append even when this read completed headers. Only the header
        // section counts toward the limit; body bytes past `\r\n\r\n` do not.
        if header_section_end(&self.buf).is_some_and(|hdr_len| hdr_len > max_header_size)
          || (!has_complete_headers(&self.buf) && self.buf.len() > max_header_size)
        {
          return Err(Error::ResponseHeaderTooLarge);
        }
      }

      // Zero-copy scan, then adopt the frozen header section as the Headers arena
      // (spans point into that Bytes). Non-ASCII values fall back to a copy+lossy
      // materialize. Body bytes may share the same allocation after freeze.
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

      let reason = Response::reason_owned(reason_bytes);
      // Compute owned spans (or None) while refs still borrow `buf`, then freeze.
      let wire_spans = Response::try_wire_header_spans(self.buf.get(..consumed).unwrap_or(&[]), &header_refs);
      let headers = if let Some(spans) = wire_spans {
        // Adopts the wire header section; no name/value copy.
        Headers::from_spans(self.buf.split_to(consumed).freeze(), spans)
      } else {
        // Obs-text / non-ASCII value: copy+lossy into a fresh arena, then drop
        // the wire header section from the receive buffer.
        let headers = Response::headers_from_refs(&header_refs);
        let _ = self.buf.split_to(consumed);
        headers
      };

      let (body_bytes, decoded_chunked_trailers) = if let Some(strategy) = body_strategy {
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
        (Bytes::new(), None)
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
        decoded_chunked_trailers,
      });
    }
  }

  /// Read up to `max` bytes from the socket directly into `self.buf` spare capacity.
  ///
  /// Reserves spare capacity as needed. For `Content-Length`, callers pass the
  /// remaining byte count so we never pull past the framed body into the next
  /// message (connection reuse).
  fn read_socket_into_buf(
    &mut self,
    max: usize,
  ) -> Result<usize, Error> {
    if max == 0 {
      return Ok(0);
    }
    let existing_spare = self.buf.capacity().saturating_sub(self.buf.len());
    if existing_spare < max {
      self.buf.reserve(max.saturating_sub(existing_spare));
    }

    // Split borrows: `socket` + `buf` are distinct fields.
    let Self { socket, buf, .. } = self;
    let uninit = buf.spare_capacity_mut();
    let to_read = uninit.len().min(max);
    if to_read == 0 {
      return Ok(0);
    }

    // SAFETY: `BlockingSocket::read` only writes into `dst` (OS/TLS adapters never
    // read the destination). Treating spare `MaybeUninit<u8>` as `&mut [u8]` for
    // the duration of the write is the standard direct-into-buffer pattern.
    let dst = unsafe { core::slice::from_raw_parts_mut(uninit.as_mut_ptr().cast::<u8>(), to_read) };

    match socket.read(dst) {
      Ok(n) => {
        // SAFETY: `read` initialized the first `n` spare bytes; `n <= to_read`
        // so the new length stays within capacity.
        unsafe {
          buf.set_len(buf.len().saturating_add(n));
        }
        Ok(n)
      },
      Err(e) => {
        // RFC 9112 Section 9.5: If timing out, implementation SHOULD issue a graceful close
        if e == SocketError::TimedOut {
          let _ = socket.shutdown();
        }
        Err(Error::Socket(e))
      },
    }
  }

  /// Read the entity body according to `strategy`.
  ///
  /// For chunked, returns the decoded payload plus `Some(trailers)` so the client
  /// skips a second decode pass. Other strategies return `(bytes, None)`.
  fn read_body(
    &mut self,
    strategy: BodyReadStrategy,
  ) -> Result<(Bytes, Option<Headers>), Error> {
    let max_body = self.max_body_size;
    match strategy {
      BodyReadStrategy::NoBody => {
        if !self.buf.is_empty() {
          self.reusable = false;
          self.buf.clear();
        }
        Ok((Bytes::new(), None))
      },
      BodyReadStrategy::ContentLength(len) => {
        // Fail fast before reserve: a huge advertised CL must not try to allocate
        // `len` bytes. Cap is `max_body_size` (network-influenced). `Vec::reserve`
        // may still abort on OOM for allowed sizes; we do not expose try_reserve
        // errors in the public API (would need a new Error variant).
        if len > max_body {
          self.reusable = false;
          return Err(Error::BodyExceedsLimit(max_body));
        }
        if self.buf.len() > len {
          // Extra bytes past CL are a framing desync; mark non-reusable.
          self.reusable = false;
        }
        let bytes_needed = len.saturating_sub(self.buf.len().min(len));
        if self.buf.len() > len {
          self.buf.truncate(len);
        }

        if bytes_needed > 0 {
          // Bounded by `max_body` check above (header buffer similarly capped by
          // `max_header_size` in `read_raw_response`).
          self.buf.reserve(bytes_needed);
          let mut bytes_read = 0usize;

          while bytes_read < bytes_needed {
            let to_read = bytes_needed.saturating_sub(bytes_read);
            let n = self.read_socket_into_buf(to_read)?;
            if n == 0 {
              return Err(Error::Socket(SocketError::NotConnected));
            }
            bytes_read = bytes_read.saturating_add(n);
          }
        }

        // split_to + freeze: body `Bytes` may share the allocation; leftover `self.buf`
        // stays empty (and possibly shared). Next request reallocates if still shared.
        Ok((self.buf.split_to(len).freeze(), None))
      },
      BodyReadStrategy::Chunked => {
        // Single-pass: frame and accumulate decoded payload during recv. Reclaim
        // consumed wire from `buf` so we do not hold the full framed message plus
        // the decoded body. Trailers are parsed when the final chunk completes
        // (trailer section is still buffered until its terminating blank line).
        if self.buf.len() > max_body {
          self.reusable = false;
          return Err(Error::BodyExceedsLimit(max_body));
        }
        let mut decoder = ChunkedDecoder::new();
        let mut decoded = Vec::new();

        loop {
          match decoder.feed(self.buf.as_ref(), Some(&mut decoded)) {
            Ok(FeedResult::Done { rest }) => {
              let rest_len = rest.len();
              let framed = self.buf.len().saturating_sub(rest_len);
              if rest_len > 0 {
                // Bytes past the chunked message cannot be unread; do not pool.
                self.reusable = false;
                let _ = self.buf.split_to(framed);
              } else {
                self.buf.clear();
              }
              if decoded.len() > max_body {
                self.reusable = false;
                return Err(Error::BodyExceedsLimit(max_body));
              }
              return Ok((Bytes::from(decoded), Some(decoder.take_trailers())));
            },
            Ok(FeedResult::NeedMore { consumed }) => {
              if consumed > 0 {
                let _ = self.buf.split_to(consumed);
              }
              if decoded.len() > max_body {
                self.reusable = false;
                return Err(Error::BodyExceedsLimit(max_body));
              }
            },
            Err(e) => {
              self.reusable = false;
              return Err(Error::Parse(e));
            },
          }

          let n = self.read_socket_into_buf(8192)?;
          if n == 0 {
            return Err(Error::Socket(SocketError::NotConnected));
          }
          // Cap unparsed wire remainder (decoded payload lives in `decoded`).
          if self.buf.len() > max_body {
            self.reusable = false;
            return Err(Error::BodyExceedsLimit(max_body));
          }
        }
      },
      BodyReadStrategy::UntilClose => {
        if self.buf.len() > max_body {
          self.reusable = false;
          return Err(Error::BodyExceedsLimit(max_body));
        }

        loop {
          let n = self.read_socket_into_buf(8192)?;
          if n == 0 {
            break;
          }
          if self.buf.len() > max_body {
            self.reusable = false;
            return Err(Error::BodyExceedsLimit(max_body));
          }
        }

        Ok((self.buf.split().freeze(), None))
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
    let host_for_sni: Cow<'_, str> = match authority.host() {
      Host::RegName(name) => Cow::Borrowed(*name),
      Host::IpAddr(addr) => Cow::Owned(crate::util::format_ip_for_host(*addr)),
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
      match socket.connect(&SocketAddr::new(*addr, port), host_for_sni.as_ref()) {
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
  // Overflow must not become "no timeout" (None → 0 blocking): saturate to u32::MAX.
  Some(u32::try_from(d?.as_millis()).unwrap_or(u32::MAX))
}

fn headers_section_len(data: &[u8]) -> Option<usize> {
  header_section_end(data)
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
