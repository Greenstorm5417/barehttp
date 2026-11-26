use crate::config::Config;
use crate::dns::DnsResolver;
use crate::error::{Error, SocketError};
use crate::headers::{Headers, WellKnownHeader, well_known_header_bytes};
use crate::parser::chunked::{ChunkedDecoder, Step};
use crate::parser::header_section_end_from;
use crate::parser::uri::Uri;
use crate::parser::version::Version;
use crate::parser::{BodyReadStrategy, RequestHead, Response, SliceSink, TAIL_BODY_MAX};
use crate::socket::BlockingSocket;
use crate::util::{HostText, IpAddr};
use alloc::vec::Vec;
use bytes::Bytes;
use core::net::SocketAddr;
use core::time::Duration;

/// Bytes of in-struct scratch for the request head and the response head.
///
/// The request head is written here (no heap) and the response is read here
/// until its header section completes; then exactly-sized heap buffers are
/// allocated for the packed headers and the body. Heads larger than this spill
/// to a heap buffer, so this bounds stack use, not header size.
pub const SCRATCH_LEN: usize = 4096;

/// Read size for until-close / chunked bodies when nothing better is known.
const BODY_READ_CHUNK: usize = 8192;

/// Status line plus headers and body bytes, before client redirect/status policy.
#[derive(Debug, Clone)]
pub struct RawResponse {
  pub status_code: u16,
  pub reason: compact_str::CompactString,
  pub headers: Headers,
  pub version: Version,
  pub body_bytes: Bytes,
  /// When `Some((start, len))`, the body is `headers.arena()[start..start + len]` (small
  /// identity-coded Content-Length body sharing the header allocation) and
  /// [`Self::body_bytes`] is empty. Read it through [`Self::body`].
  pub body_tail: Option<(u32, u32)>,
  /// When `Some`, [`Self::body_bytes`] is already the decoded payload and this
  /// holds trailer fields (empty if the message had none). When `None`,
  /// `body_bytes` is still strategy wire form (offline chunked parse via the
  /// response parser). The live connection always returns `Some`.
  pub decoded_chunked_trailers: Option<Headers>,
}

impl RawResponse {
  /// Body bytes wherever they are stored.
  #[must_use]
  #[cfg_attr(not(test), allow(dead_code))]
  pub fn body(&self) -> &[u8] {
    match self.body_tail {
      Some((start, len)) => crate::parser::tail_slice(self.headers.arena(), start, len),
      None => &self.body_bytes,
    }
  }
}

/// Live HTTP connection for raw send and receive.
pub struct Connection<'a, S> {
  socket: &'a mut S,
  max_header_size: usize,
  max_body_size: usize,
  reusable: bool,
  /// Request head out, then response head in (see [`SCRATCH_LEN`]).
  scratch: [u8; SCRATCH_LEN],
  /// Response head bytes once they outgrow `scratch` (empty otherwise).
  spill: Vec<u8>,
}

impl<'a, S: BlockingSocket> Connection<'a, S> {
  /// Wrap a connected socket with response header / body limits.
  pub const fn new(
    socket: &'a mut S,
    max_header_size: usize,
    max_body_size: usize,
  ) -> Self {
    Self {
      socket,
      max_header_size,
      max_body_size,
      reusable: true,
      scratch: [0; SCRATCH_LEN],
      spill: Vec::new(),
    }
  }

  /// Write request head then body without concatenating into one buffer.
  ///
  /// Uses [`BlockingSocket::write_vectored`] when the adapter supports it
  /// (cleartext OS TCP); adapters that only implement [`BlockingSocket::write`]
  /// fall back to sequential partial writes (still no head+body copy).
  ///
  /// Raw-bytes entry point (tests); the client uses [`Self::send_head`].
  #[cfg_attr(not(test), allow(dead_code))]
  pub fn send_request(
    &mut self,
    head: &[u8],
    body: &[u8],
  ) -> Result<(), Error> {
    write_all_vectored(self.socket, &[head, body])?;

    // RFC 9112 Section 9.6: If the client sends "Connection: close", it MUST NOT
    // send further requests on that connection. Scan header block only.
    if request_has_connection_close(head) {
      self.reusable = false;
    }

    Ok(())
  }

  /// Serialize `head` into the connection scratch (heap only when it does not
  /// fit) and send it followed by `body`.
  pub fn send_head(
    &mut self,
    head: &RequestHead<'_>,
    body: &[u8],
  ) -> Result<(), Error> {
    let len = head.wire_len();
    let Self { socket, scratch, .. } = self;
    let mut sink = SliceSink::new(scratch.get_mut(..len).unwrap_or(&mut []));
    head.write_to(&mut sink);
    if let Some(n) = sink.finish() {
      write_all_vectored(*socket, &[scratch.get(..n).unwrap_or(&[]), body])?;
    } else {
      let mut v = Vec::with_capacity(len);
      head.write_to(&mut v);
      write_all_vectored(*socket, &[&v, body])?;
    }
    if head.closes() {
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
    self.spill.clear();
    // Bytes of response head currently buffered (scratch, or spill once spilled).
    let mut filled = 0usize;
    let mut searched = 0usize;

    loop {
      while header_section_end_from(self.head_bytes(filled), searched.saturating_sub(3)).is_none() {
        if filled > max_header_size {
          return Err(Error::ResponseHeaderTooLarge);
        }
        searched = filled;
        let n = self.read_head_more(&mut filled)?;
        if n == 0 {
          // Peer closed before a complete header section: Socket NotConnected / EOF.
          return Err(Error::Socket(SocketError::NotConnected));
        }
      }

      // Split borrows: the head lives in `scratch`/`spill` while the body is
      // read from `socket` into fresh, exactly-sized buffers.
      let Self {
        socket,
        max_body_size,
        reusable,
        scratch,
        spill,
        ..
      } = self;
      let data: &mut [u8] = if spill.is_empty() {
        scratch.get_mut(..filled).unwrap_or(&mut [])
      } else {
        spill.get_mut(..filled).unwrap_or(&mut [])
      };

      let head = Response::scan_head(data).map_err(Error::Parse)?;
      let consumed = head.consumed;
      if consumed > max_header_size {
        return Err(Error::ResponseHeaderTooLarge);
      }

      // RFC 9112: discard 1xx and keep reading the final response
      if (100..200).contains(&head.status_code) {
        data.copy_within(consumed.., 0);
        filled = filled.saturating_sub(consumed);
        searched = 0;
        if !spill.is_empty() {
          spill.truncate(filled);
        }
        continue;
      }

      let status_code = head.status_code;
      let version = head.version;
      let body_strategy = if expect_body {
        match Response::body_read_strategy_pairs(head.fields.pairs(head.section), status_code, version) {
          Ok(s) => Some(s),
          Err(e) => {
            *reusable = false;
            return Err(Error::Parse(e));
          },
        }
      } else {
        None
      };
      let reason = Response::reason_owned(head.reason);
      let prefix: &[u8] = data.get(consumed..).unwrap_or(&[]);

      // Small identity-coded Content-Length body: read it straight into the
      // header arena's tail — one allocation for the whole response.
      if let Some(BodyReadStrategy::ContentLength(len)) = body_strategy
        && len <= TAIL_BODY_MAX
        && len <= *max_body_size
        && !head.has_content_encoding()
      {
        let mut headers = head.into_headers_with_tail(len);
        let start = u32::try_from(headers.arena().len()).unwrap_or(u32::MAX);
        let tail = headers.arena_tail_mut();
        read_content_length_into(&mut **socket, prefix, len, tail, reusable)?;
        let (mut raw, keep) = finish_raw(
          status_code,
          reason,
          headers,
          version,
          Bytes::new(),
          Headers::new(),
          *reusable,
        );
        raw.body_tail = Some((start, u32::try_from(len).unwrap_or(0)));
        *reusable = keep;
        return Ok(raw);
      }

      // The only header copy: live name/value bytes into one exact arena.
      let headers = head.into_headers();

      let body_bytes = match body_strategy {
        Some(BodyReadStrategy::UntilClose) => {
          // UntilClose ends the connection; never pool it.
          *reusable = false;
          read_until_close(&mut **socket, prefix, *max_body_size)?
        },
        Some(BodyReadStrategy::ContentLength(len)) => {
          read_content_length(&mut **socket, prefix, len, *max_body_size, reusable)?
        },
        Some(BodyReadStrategy::Chunked) => {
          let (body, trailers) = read_chunked(&mut **socket, prefix, *max_body_size, reusable)?;
          let (raw, keep) = finish_raw(status_code, reason, headers, version, body, trailers, *reusable);
          *reusable = keep;
          return Ok(raw);
        },
        // NoBody / HEAD: no entity body on the wire (RFC 9112); leftover = desync.
        Some(BodyReadStrategy::NoBody) | None => {
          if !prefix.is_empty() {
            *reusable = false;
          }
          Bytes::new()
        },
      };

      let (raw, keep) = finish_raw(
        status_code,
        reason,
        headers,
        version,
        body_bytes,
        Headers::new(),
        *reusable,
      );
      *reusable = keep;
      return Ok(raw);
    }
  }

  /// Buffered response-head bytes.
  fn head_bytes(
    &self,
    filled: usize,
  ) -> &[u8] {
    if self.spill.is_empty() {
      self.scratch.get(..filled).unwrap_or(&[])
    } else {
      self.spill.get(..filled).unwrap_or(&[])
    }
  }

  /// Read more response-head bytes into scratch (or the spill buffer once full).
  fn read_head_more(
    &mut self,
    filled: &mut usize,
  ) -> Result<usize, Error> {
    if self.spill.is_empty() && *filled < SCRATCH_LEN {
      let dst = self.scratch.get_mut(*filled..).unwrap_or(&mut []);
      let n = read_socket(self.socket, dst)?;
      *filled = filled.saturating_add(n);
      return Ok(n);
    }
    if self.spill.is_empty() {
      // Head outgrew the scratch: move it to the heap and keep going (bounded by
      // `max_header_size` in the caller).
      self.spill.reserve(SCRATCH_LEN.saturating_mul(2));
      self
        .spill
        .extend_from_slice(self.scratch.get(..*filled).unwrap_or(&[]));
    }
    let n = read_into_spare(self.socket, &mut self.spill, SCRATCH_LEN)?;
    *filled = filled.saturating_add(n);
    Ok(n)
  }

  /// Whether this connection may be returned to the pool.
  ///
  /// False after either side sent `Connection: close` (RFC 9112 §9.6).
  pub const fn is_reusable(&self) -> bool {
    self.reusable
  }
}

/// Assemble the [`RawResponse`] and apply response-side persistence rules.
fn finish_raw(
  status_code: u16,
  reason: compact_str::CompactString,
  headers: Headers,
  version: Version,
  body_bytes: Bytes,
  trailers: Headers,
  mut reusable: bool,
) -> (RawResponse, bool) {
  // RFC 9112 §9.3 / §9.6: persistence from version + all Connection field lines
  if connection_option_present(&headers, "close") {
    reusable = false;
  } else if !version.defaults_to_persistent() && !connection_option_present(&headers, "keep-alive") {
    // HTTP/1.0 (and earlier): close unless keep-alive is present
    reusable = false;
  }
  (
    RawResponse {
      status_code,
      reason,
      headers,
      version,
      body_bytes,
      body_tail: None,
      decoded_chunked_trailers: Some(trailers),
    },
    reusable,
  )
}

/// `socket.read` into `dst`, with the RFC 9112 §9.5 graceful close on timeout.
fn read_socket<S: BlockingSocket>(
  socket: &mut S,
  dst: &mut [u8],
) -> Result<usize, Error> {
  if dst.is_empty() {
    return Ok(0);
  }
  match socket.read(dst) {
    Ok(n) => Ok(n.min(dst.len())),
    Err(e) => {
      // RFC 9112 Section 9.5: If timing out, implementation SHOULD issue a graceful close
      if e == SocketError::TimedOut {
        let _ = socket.shutdown();
      }
      Err(Error::Socket(e))
    },
  }
}

/// Read up to `max` bytes from the socket directly into `buf`'s spare capacity.
fn read_into_spare<S: BlockingSocket>(
  socket: &mut S,
  buf: &mut Vec<u8>,
  max: usize,
) -> Result<usize, Error> {
  if max == 0 {
    return Ok(0);
  }
  if buf.capacity().saturating_sub(buf.len()) < max {
    buf.reserve(max);
  }
  let uninit = buf.spare_capacity_mut();
  let to_read = uninit.len().min(max);
  // SAFETY: `BlockingSocket::read` only writes into `dst` (OS/TLS adapters never
  // read the destination). Treating spare `MaybeUninit<u8>` as `&mut [u8]` for
  // the duration of the write is the standard direct-into-buffer pattern.
  let dst = unsafe { core::slice::from_raw_parts_mut(uninit.as_mut_ptr().cast::<u8>(), to_read) };
  let n = read_socket(socket, dst)?;
  // SAFETY: `read` initialized the first `n <= to_read` spare bytes.
  unsafe {
    buf.set_len(buf.len().saturating_add(n));
  }
  Ok(n)
}

/// `Vec` → `Bytes` without a shared-header allocation or retained slack.
fn into_exact_bytes(mut v: Vec<u8>) -> Bytes {
  if v.is_empty() {
    return Bytes::new();
  }
  if v.len() != v.capacity() {
    v.shrink_to_fit();
  }
  Bytes::from(v)
}

/// Content-Length body appended to `out` (capacity already reserved), socket
/// bytes landing in it directly.
fn read_content_length_into<S: BlockingSocket>(
  socket: &mut S,
  prefix: &[u8],
  len: usize,
  out: &mut Vec<u8>,
  reusable: &mut bool,
) -> Result<(), Error> {
  if prefix.len() > len {
    // Extra bytes past CL are a framing desync; mark non-reusable.
    *reusable = false;
  }
  let target = out.len().saturating_add(len);
  out.extend_from_slice(prefix.get(..prefix.len().min(len)).unwrap_or(&[]));
  while out.len() < target {
    let want = target.saturating_sub(out.len());
    let n = read_into_spare(socket, out, want)?;
    if n == 0 {
      return Err(Error::Socket(SocketError::NotConnected));
    }
  }
  Ok(())
}

/// Content-Length body: one exact allocation; socket bytes land in it directly.
fn read_content_length<S: BlockingSocket>(
  socket: &mut S,
  prefix: &[u8],
  len: usize,
  max_body: usize,
  reusable: &mut bool,
) -> Result<Bytes, Error> {
  // Fail fast before allocating: a huge advertised CL must not reserve `len`.
  if len > max_body {
    *reusable = false;
    return Err(Error::BodyExceedsLimit(max_body));
  }
  if prefix.len() > len {
    // Extra bytes past CL are a framing desync; mark non-reusable.
    *reusable = false;
  }
  if len == 0 {
    return Ok(Bytes::new());
  }
  let mut body = Vec::with_capacity(len);
  body.extend_from_slice(prefix.get(..prefix.len().min(len)).unwrap_or(&[]));
  while body.len() < len {
    let want = len.saturating_sub(body.len());
    let n = read_into_spare(socket, &mut body, want)?;
    if n == 0 {
      return Err(Error::Socket(SocketError::NotConnected));
    }
  }
  Ok(Bytes::from(body))
}

/// Until-close body: grow one buffer, then trim slack.
fn read_until_close<S: BlockingSocket>(
  socket: &mut S,
  prefix: &[u8],
  max_body: usize,
) -> Result<Bytes, Error> {
  if prefix.len() > max_body {
    return Err(Error::BodyExceedsLimit(max_body));
  }
  let mut body = Vec::with_capacity(prefix.len().saturating_add(BODY_READ_CHUNK));
  body.extend_from_slice(prefix);
  loop {
    let n = read_into_spare(socket, &mut body, BODY_READ_CHUNK)?;
    if n == 0 {
      break;
    }
    if body.len() > max_body {
      return Err(Error::BodyExceedsLimit(max_body));
    }
  }
  Ok(into_exact_bytes(body))
}

/// Chunked body decoded **in place**: one buffer holds `[payload | unparsed
/// wire]`; each data span is moved down over the framing already consumed, so
/// there is never a second (wire + decoded) copy of the body in memory.
fn read_chunked<S: BlockingSocket>(
  socket: &mut S,
  prefix: &[u8],
  max_body: usize,
  reusable: &mut bool,
) -> Result<(Bytes, Headers), Error> {
  if prefix.len() > max_body {
    *reusable = false;
    return Err(Error::BodyExceedsLimit(max_body));
  }
  let mut decoder = ChunkedDecoder::new();
  let mut buf: Vec<u8> = Vec::new();
  buf.extend_from_slice(prefix);
  // `buf[..dlen]` is decoded payload; `buf[pos..]` is wire not yet parsed.
  let mut dlen = 0usize;
  let mut pos = 0usize;

  loop {
    let step = match decoder.step(buf.get(pos..).unwrap_or(&[])) {
      Ok(step) => step,
      Err(e) => {
        *reusable = false;
        return Err(Error::Parse(e));
      },
    };
    match step {
      Step::Data { start, len } => {
        let src = pos.saturating_add(start);
        if src != dlen {
          buf.copy_within(src..src.saturating_add(len), dlen);
        }
        dlen = dlen.saturating_add(len);
        pos = src.saturating_add(len);
        if dlen > max_body {
          *reusable = false;
          return Err(Error::BodyExceedsLimit(max_body));
        }
      },
      Step::Done { end } => {
        if pos.saturating_add(end) < buf.len() {
          // Bytes past the chunked message cannot be unread; do not pool.
          *reusable = false;
        }
        buf.truncate(dlen);
        return Ok((into_exact_bytes(buf), decoder.take_trailers()));
      },
      Step::NeedMore { consumed } => {
        pos = pos.saturating_add(consumed);
        // Slide the (small) unparsed framing tail down onto the payload end.
        if pos > dlen {
          let tail = buf.len().saturating_sub(pos);
          buf.copy_within(pos.., dlen);
          buf.truncate(dlen.saturating_add(tail));
          pos = dlen;
        }
        // Size the next read from the chunk still owed (bounded by the cap).
        let budget = max_body.saturating_sub(dlen).saturating_add(64);
        let want = decoder
          .pending_data()
          .max(BODY_READ_CHUNK)
          .min(budget)
          .max(1);
        let n = read_into_spare(socket, &mut buf, want)?;
        if n == 0 {
          return Err(Error::Socket(SocketError::NotConnected));
        }
        // Cap unparsed wire remainder too.
        if buf.len().saturating_sub(dlen) > max_body {
          *reusable = false;
          return Err(Error::BodyExceedsLimit(max_body));
        }
      },
    }
  }
}

/// Write all of `bufs` in order, advancing across buffers on short writes.
fn write_all_vectored<S: BlockingSocket>(
  socket: &mut S,
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
      socket.write(first).map_err(Error::Socket)?
    } else {
      socket
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

/// Connect to `uri`, or wrap a pooled already-connected socket.
///
/// When `reused` is true, skips DNS and connect.
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
  let host = crate::util::host_text(uri);
  connect_host(socket, dns, uri, &host, config, reused)
}

/// Like [`connect`], with the hop host text already computed by the caller
/// (shared with the `Host` header; no per-hop SNI formatting).
///
/// # Errors
/// [`Error::InvalidUrl`], [`Error::Dns`], or [`Error::Socket`].
pub fn connect_host<'a, S, D>(
  socket: &'a mut S,
  dns: &D,
  uri: &Uri,
  host: &HostText<'_>,
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

    // IP literals connect directly: no resolver call, no address `Vec`.
    let resolved: Vec<IpAddr>;
    let literal: [IpAddr; 1];
    let addresses: &[IpAddr] = match authority.host() {
      crate::parser::uri::Host::RegName(name) => {
        resolved = dns.resolve(name).map_err(Error::Dns)?;
        &resolved
      },
      crate::parser::uri::Host::IpAddr(addr) => {
        literal = [*addr];
        &literal
      },
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
    for addr in addresses {
      match socket.connect(&SocketAddr::new(*addr, port), host.as_str()) {
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
  Ok(Connection::new(
    socket,
    config.max_response_header_size(),
    config.max_response_body_size(),
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

#[cfg_attr(not(test), allow(dead_code))]
fn headers_section_len(data: &[u8]) -> Option<usize> {
  crate::parser::header_section_end(data)
}

/// True if any `Connection` field line lists the given option (RFC 9110 list rules).
fn connection_option_present(
  headers: &Headers,
  option: &str,
) -> bool {
  headers
    .values(Headers::CONNECTION)
    .any(|v| v.split(',').any(|t| t.trim().eq_ignore_ascii_case(option)))
}

/// True if a request's `Connection` header list contains the `close` token.
#[cfg_attr(not(test), allow(dead_code))]
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
