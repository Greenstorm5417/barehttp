extern crate alloc;
use crate::error::ParseError;
use crate::headers::{Headers, WellKnownHeader, well_known_header};
use crate::parser::headers::is_token_char;
use alloc::vec::Vec;

/// Per-hop fields the client adds on top of the caller's [`Headers`].
///
/// Nothing here is merged into the caller's map: [`RequestHead`] writes the
/// effective field set straight to the wire buffer.
#[derive(Debug, Clone, Copy, Default)]
pub struct HeadOptions<'a> {
  /// `uri-host` (no port). Written first as `Host` (RFC 9110 §7.2); any
  /// caller `Host` field is ignored.
  pub host: &'a str,
  /// Appended to `Host` as `:port` when the port is not the scheme default.
  pub host_port: Option<u16>,
  /// Send `Connection: close` (replacing every caller `Connection` field).
  pub connection_close: bool,
  /// Default `User-Agent` (empty = none); skipped when the caller sets one.
  pub user_agent: &'a str,
  /// Default `Accept` (empty = none); skipped when the caller sets one.
  pub accept: &'a str,
  /// Default `Accept-Encoding` (empty = none); skipped when the caller sets one.
  pub accept_encoding: &'a str,
  /// Cookie-jar pairs (empty = none): appended to the first caller `Cookie`
  /// field with `"; "`, or sent as a new `Cookie` field.
  pub cookie: &'a str,
}

/// Validated request head, ready to write into any buffer with
/// [`Self::write_to`] (exactly [`Self::wire_len`] bytes).
#[derive(Debug, Clone, Copy)]
#[allow(clippy::struct_excessive_bools)] // independent per-field write decisions
pub struct RequestHead<'a> {
  method: &'a str,
  path: &'a str,
  query: Option<&'a str>,
  headers: &'a Headers,
  opts: HeadOptions<'a>,
  has_connection: bool,
  has_cookie: bool,
  send_user_agent: bool,
  send_accept: bool,
  send_accept_encoding: bool,
  inject_cl: Option<usize>,
  closes: bool,
  wire_len: usize,
}

/// Byte sink for the head writer (stack slice or heap `Vec`).
pub trait Sink {
  /// Append `bytes`.
  fn put(
    &mut self,
    bytes: &[u8],
  );
}

impl Sink for Vec<u8> {
  #[inline]
  fn put(
    &mut self,
    bytes: &[u8],
  ) {
    self.extend_from_slice(bytes);
  }
}

/// Fixed-capacity sink over a borrowed slice. Overflow is recorded, not written.
#[derive(Debug)]
pub struct SliceSink<'a> {
  buf: &'a mut [u8],
  len: usize,
  overflow: bool,
}

impl<'a> SliceSink<'a> {
  /// Empty sink writing into `buf`.
  pub const fn new(buf: &'a mut [u8]) -> Self {
    Self {
      buf,
      len: 0,
      overflow: false,
    }
  }

  /// Bytes written so far, or `None` if a write did not fit.
  pub const fn finish(self) -> Option<usize> {
    if self.overflow {
      None
    } else {
      Some(self.len)
    }
  }
}

impl Sink for SliceSink<'_> {
  #[inline]
  fn put(
    &mut self,
    bytes: &[u8],
  ) {
    let end = self.len.saturating_add(bytes.len());
    if let Some(dst) = self.buf.get_mut(self.len..end) {
      dst.copy_from_slice(bytes);
      self.len = end;
    } else {
      self.overflow = true;
    }
  }
}

#[inline]
fn value_ok(value: &str) -> bool {
  !value
    .bytes()
    .any(|b| matches!(b, 0..=8 | 0x0A..=0x1F | 0x7F))
}

#[inline]
fn name_ok(name: &str) -> bool {
  !name.is_empty() && name.bytes().all(is_token_char)
}

#[inline]
const fn line_len(
  name: &str,
  value: &str,
) -> usize {
  // "name: value\r\n"
  name.len().saturating_add(value.len()).saturating_add(4)
}

#[inline]
fn lists_token(
  value: &str,
  token: &str,
) -> bool {
  value
    .split(',')
    .any(|t| t.trim().eq_ignore_ascii_case(token))
}

const fn decimal_len(mut n: usize) -> usize {
  let mut digits = 1usize;
  while n >= 10 {
    n /= 10;
    digits = digits.saturating_add(1);
  }
  digits
}

fn put_decimal<W: Sink>(
  out: &mut W,
  mut n: usize,
) {
  let mut tmp = [0u8; 20];
  let mut i = tmp.len();
  loop {
    i = i.saturating_sub(1);
    if let Some(slot) = tmp.get_mut(i) {
      #[allow(clippy::cast_possible_truncation)] // n % 10 fits in u8
      {
        *slot = b'0' + (n % 10) as u8;
      }
    }
    n /= 10;
    if n == 0 {
      break;
    }
  }
  out.put(tmp.get(i..).unwrap_or(&[]));
}

impl<'a> RequestHead<'a> {
  /// Validate the effective field set and size the head (one pass over `headers`).
  ///
  /// # Errors
  /// Field-name / field-value injection, Host, TE and framing violations of
  /// RFC 9112, or a non origin-form target.
  pub fn new(
    method: &'a str,
    path: &'a str,
    query: Option<&'a str>,
    headers: &'a Headers,
    body: Option<&[u8]>,
    opts: HeadOptions<'a>,
  ) -> Result<Self, ParseError> {
    let mut has_te = false;
    let mut cl_value: Option<&str> = None;
    let mut has_te_field = false;
    let mut te_lists_chunked = false;
    let mut connection_has_te = false;
    let mut has_connection = false;
    let mut has_cookie = false;
    let mut has_ua = false;
    let mut has_accept = false;
    let mut has_ae = false;
    let mut closes = opts.connection_close;
    // "METHOD" + " " + target + " HTTP/1.1\r\n" fixed parts.
    let mut wire = method.len().saturating_add(12);

    for (name, value) in headers {
      let kind = well_known_header(name);
      // Host is always rebuilt from the URI for this hop.
      if kind == Some(WellKnownHeader::Host) {
        continue;
      }
      if kind == Some(WellKnownHeader::Connection) && opts.connection_close {
        // Replaced by a single `Connection: close` (written at the first one).
        if !has_connection {
          has_connection = true;
          wire = wire.saturating_add(line_len(Headers::CONNECTION, "close"));
        }
        continue;
      }
      if !name_ok(name) {
        return Err(ParseError::InvalidHeaderName);
      }
      if !value_ok(value) {
        return Err(ParseError::InvalidHeaderValue);
      }
      wire = wire.saturating_add(line_len(name, value));

      match kind {
        Some(WellKnownHeader::TransferEncoding) => has_te = true,
        Some(WellKnownHeader::ContentLength) => cl_value = Some(value),
        Some(WellKnownHeader::Te) => {
          has_te_field = true;
          for coding in value.split(',') {
            let coding_name = coding.trim().split(';').next().unwrap_or("").trim();
            if coding_name.eq_ignore_ascii_case("chunked") {
              te_lists_chunked = true;
            }
          }
        },
        Some(WellKnownHeader::Connection) => {
          has_connection = true;
          connection_has_te |= lists_token(value, "TE");
          closes |= lists_token(value, "close");
        },
        Some(WellKnownHeader::Cookie) => {
          if !has_cookie && !opts.cookie.is_empty() {
            wire = wire.saturating_add(2).saturating_add(opts.cookie.len());
          }
          has_cookie = true;
        },
        Some(WellKnownHeader::UserAgent) => has_ua = true,
        Some(WellKnownHeader::Accept) => has_accept = true,
        Some(WellKnownHeader::AcceptEncoding) => has_ae = true,
        _ => {},
      }
    }

    // RFC 9112 Section 3.2: Client MUST send Host in every HTTP/1.1 request
    // (an empty host is rejected as an invalid value).
    if !is_valid_host(opts.host, opts.host_port.is_some()) {
      return Err(ParseError::InvalidHostHeaderValue);
    }
    wire = wire.saturating_add(line_len(Headers::HOST, opts.host));
    if let Some(port) = opts.host_port {
      wire = wire
        .saturating_add(1)
        .saturating_add(decimal_len(usize::from(port)));
    }

    // RFC 9112 §7.4: TE must not list chunked; sender of TE MUST also send Connection: TE
    if te_lists_chunked {
      return Err(ParseError::ChunkedInTeHeader);
    }
    if has_te_field && !connection_has_te {
      return Err(ParseError::TeHeaderMissingConnection);
    }

    // This client frames request bodies with Content-Length only (RFC 9112 §6.3).
    if has_te && cl_value.is_some() {
      return Err(ParseError::ConflictingFraming);
    }
    if has_te {
      return Err(ParseError::RequestTransferEncodingUnsupported);
    }

    // Body length is authoritative: reject a mismatched Content-Length.
    let body_len = body.map_or(0, <[u8]>::len);
    if body.is_some()
      && let Some(cl_val) = cl_value
    {
      let parsed = cl_val
        .trim()
        .parse::<usize>()
        .map_err(|_| ParseError::InvalidContentLength)?;
      if parsed != body_len {
        return Err(ParseError::InvalidContentLength);
      }
    }

    // RFC 9112 §3.2.1: origin-form is absolute-path ["?" query]; empty path → "/"
    let target = if path.is_empty() {
      "/"
    } else {
      path
    };
    if !is_origin_form_request_target(target) {
      return Err(ParseError::InvalidUri);
    }
    wire = wire
      .saturating_add(target.len())
      .saturating_add(query.map_or(0, |q| q.len().saturating_add(1)))
      .saturating_add(2); // final CRLF

    if !opts.cookie.is_empty() {
      if !value_ok(opts.cookie) {
        return Err(ParseError::InvalidHeaderValue);
      }
      if !has_cookie {
        wire = wire.saturating_add(line_len(Headers::COOKIE, opts.cookie));
      }
    }
    if opts.connection_close && !has_connection {
      wire = wire.saturating_add(line_len(Headers::CONNECTION, "close"));
    }
    let send_user_agent = !opts.user_agent.is_empty() && !has_ua;
    let send_accept = !opts.accept.is_empty() && !has_accept;
    let send_accept_encoding = !opts.accept_encoding.is_empty() && !has_ae;
    for (send, name, value) in [
      (send_user_agent, Headers::USER_AGENT, opts.user_agent),
      (send_accept, Headers::ACCEPT, opts.accept),
      (send_accept_encoding, Headers::ACCEPT_ENCODING, opts.accept_encoding),
    ] {
      if send {
        if !value_ok(value) {
          return Err(ParseError::InvalidHeaderValue);
        }
        wire = wire.saturating_add(line_len(name, value));
      }
    }

    let inject_cl = if body.is_some() && cl_value.is_none() {
      wire = wire
        .saturating_add(line_len(Headers::CONTENT_LENGTH, ""))
        .saturating_add(decimal_len(body_len));
      Some(body_len)
    } else {
      None
    };

    Ok(Self {
      method,
      path: target,
      query,
      headers,
      opts,
      has_connection,
      has_cookie,
      send_user_agent,
      send_accept,
      send_accept_encoding,
      inject_cl,
      closes,
      wire_len: wire,
    })
  }

  /// Exact byte length [`Self::write_to`] produces.
  #[must_use]
  pub const fn wire_len(&self) -> usize {
    self.wire_len
  }

  /// The request asks the server to close (`Connection: close`, RFC 9112 §9.6).
  #[must_use]
  pub const fn closes(&self) -> bool {
    self.closes
  }

  /// Write request-line, fields, and the terminating blank line.
  pub fn write_to<W: Sink>(
    &self,
    out: &mut W,
  ) {
    out.put(self.method.as_bytes());
    out.put(b" ");
    out.put(self.path.as_bytes());
    if let Some(q) = self.query {
      out.put(b"?");
      out.put(q.as_bytes());
    }
    // RFC 9112: this is an HTTP/1.1 client — request-line version is always 1.1
    out.put(b" HTTP/1.1\r\nHost: ");
    out.put(self.opts.host.as_bytes());
    if let Some(port) = self.opts.host_port {
      out.put(b":");
      put_decimal(out, usize::from(port));
    }
    out.put(b"\r\n");

    let mut wrote_connection = false;
    let mut merged_cookie = false;
    for (name, value) in self.headers {
      // Cheap length-gated checks; only names that get special handling.
      match name.len() {
        4 if name.eq_ignore_ascii_case(Headers::HOST) => continue,
        10 if self.opts.connection_close && name.eq_ignore_ascii_case(Headers::CONNECTION) => {
          if !wrote_connection {
            wrote_connection = true;
            out.put(b"Connection: close\r\n");
          }
          continue;
        },
        6 if !merged_cookie && !self.opts.cookie.is_empty() && name.eq_ignore_ascii_case(Headers::COOKIE) => {
          merged_cookie = true;
          out.put(name.as_bytes());
          out.put(b": ");
          out.put(value.as_bytes());
          out.put(b"; ");
          out.put(self.opts.cookie.as_bytes());
          out.put(b"\r\n");
          continue;
        },
        _ => {},
      }
      put_line(out, name, value);
    }

    if !self.opts.cookie.is_empty() && !self.has_cookie {
      put_line(out, Headers::COOKIE, self.opts.cookie);
    }
    if self.opts.connection_close && !self.has_connection {
      out.put(b"Connection: close\r\n");
    }
    if self.send_user_agent {
      put_line(out, Headers::USER_AGENT, self.opts.user_agent);
    }
    if self.send_accept {
      put_line(out, Headers::ACCEPT, self.opts.accept);
    }
    if self.send_accept_encoding {
      put_line(out, Headers::ACCEPT_ENCODING, self.opts.accept_encoding);
    }
    if let Some(n) = self.inject_cl {
      out.put(b"Content-Length: ");
      put_decimal(out, n);
      out.put(b"\r\n");
    }
    out.put(b"\r\n");
  }

  /// Heap copy of the head (tests / assertions).
  #[cfg(test)]
  #[must_use]
  pub fn to_vec(self) -> Vec<u8> {
    let mut v = Vec::with_capacity(self.wire_len);
    self.write_to(&mut v);
    v
  }
}

#[inline]
fn put_line<W: Sink>(
  out: &mut W,
  name: &str,
  value: &str,
) {
  out.put(name.as_bytes());
  out.put(b": ");
  out.put(value.as_bytes());
  out.put(b"\r\n");
}

/// Wire request with header block and body kept separate (tests).
#[cfg(test)]
#[derive(Debug, Clone)]
pub struct SerializedRequest<'a> {
  /// Request-line, headers, and final `\r\n` (no entity body).
  pub head: Vec<u8>,
  /// Entity body octets (empty when no body).
  pub body: &'a [u8],
}

#[cfg(test)]
impl SerializedRequest<'_> {
  /// Contiguous wire image.
  #[must_use]
  pub fn to_bytes(&self) -> Vec<u8> {
    let mut out = self.head.clone();
    out.extend_from_slice(self.body);
    out
  }
}

/// Serialize with `Host` taken from `headers` (RFC 9112 rule tests).
///
/// # Errors
/// [`ParseError::MissingHostHeader`], host / TE / framing violations of RFC 9112.
#[cfg(test)]
pub fn serialize_request<'a>(
  method: &str,
  path: &str,
  query: Option<&str>,
  headers: &Headers,
  body: Option<&'a [u8]>,
) -> Result<SerializedRequest<'a>, ParseError> {
  let mut found: Option<&str> = None;
  let mut host_count = 0usize;
  for (name, value) in headers {
    if !name_ok(name) {
      return Err(ParseError::InvalidHeaderName);
    }
    if !value_ok(value) {
      return Err(ParseError::InvalidHeaderValue);
    }
    if well_known_header(name) == Some(WellKnownHeader::Host) {
      host_count = host_count.saturating_add(1);
      found = Some(value);
    }
  }
  let host = found.ok_or(ParseError::MissingHostHeader)?;
  if host_count > 1 {
    return Err(ParseError::MultipleHostHeaders);
  }
  let head = RequestHead::new(
    method,
    path,
    query,
    headers,
    body,
    HeadOptions {
      host: host.trim(),
      ..HeadOptions::default()
    },
  )?;
  let bytes = head.to_vec();
  debug_assert_eq!(bytes.len(), head.wire_len());
  Ok(SerializedRequest {
    head: bytes,
    body: body.unwrap_or(&[]),
  })
}

/// RFC 9112 §3.2.1 origin-form, or asterisk-form `*` (OPTIONS).
fn is_origin_form_request_target(path: &str) -> bool {
  if path == "*" {
    return true;
  }
  // absolute-path must start with `/`; reject absolute-form (`http://...`)
  path.starts_with('/') && !path.contains("://")
}

/// RFC 9110 §7.2 `Host = uri-host [ ":" port ]`; client-side validation.
///
/// `numeric_port` = a validated `:port` will follow `host` on the wire.
fn is_valid_host(
  raw_host: &str,
  numeric_port: bool,
) -> bool {
  let host = raw_host.trim();
  if host.is_empty() {
    return false;
  }
  // No whitespace / CTL / userinfo delimiter
  if host.bytes().any(|b| b <= 0x20 || b == 0x7F || b == b'@') {
    return false;
  }
  if host.starts_with('[') {
    let Some(end) = host.find(']') else {
      return false;
    };
    let rest = host.get(end.saturating_add(1)..).unwrap_or("");
    if rest.is_empty() {
      return true;
    }
    if numeric_port {
      // `[v6]:port` + another `:port` is malformed.
      return false;
    }
    return match rest.strip_prefix(':') {
      Some(port) if !port.is_empty() => port.bytes().all(|b| b.is_ascii_digit()),
      _ => false,
    };
  }
  if numeric_port {
    // `host:port` — the appended port is digits; the name part is non-empty.
    return true;
  }
  if let Some((name, port)) = host.rsplit_once(':')
    && (name.is_empty() || port.is_empty() || !port.bytes().all(|b| b.is_ascii_digit()))
  {
    return false;
  }
  true
}

#[cfg(test)]
mod tests {
  #![allow(clippy::unwrap_used)]
  use super::*;

  fn head_text(
    headers: &Headers,
    body: Option<&[u8]>,
    opts: HeadOptions<'_>,
  ) -> alloc::string::String {
    let head = RequestHead::new("GET", "/p", Some("q=1"), headers, body, opts).unwrap();
    let v = head.to_vec();
    assert_eq!(v.len(), head.wire_len(), "wire_len must be exact");
    let mut stack = [0u8; 512];
    let mut sink = SliceSink::new(&mut stack);
    head.write_to(&mut sink);
    assert_eq!(sink.finish(), Some(v.len()));
    alloc::string::String::from_utf8(v).unwrap()
  }

  #[test]
  fn exact_len_with_every_option() {
    let mut h = Headers::new();
    h.insert("Host", "ignored.example");
    h.insert("Connection", "keep-alive");
    h.insert("connection", "TE");
    h.insert("Cookie", "u=1");
    h.insert("X-A", "1");
    let t = head_text(
      &h,
      Some(b"abc"),
      HeadOptions {
        host: "example.com",
        host_port: Some(8080),
        connection_close: true,
        user_agent: "ua/1",
        accept: "*/*",
        accept_encoding: "gzip",
        cookie: "j=2",
      },
    );
    assert_eq!(
      t,
      "GET /p?q=1 HTTP/1.1\r\nHost: example.com:8080\r\nConnection: close\r\nCookie: u=1; j=2\r\nX-A: 1\r\n\
       User-Agent: ua/1\r\nAccept: */*\r\nAccept-Encoding: gzip\r\nContent-Length: 3\r\n\r\n"
    );
  }

  #[test]
  fn defaults_skipped_when_caller_sets_them() {
    let mut h = Headers::new();
    h.insert("user-agent", "mine");
    h.insert("ACCEPT", "text/plain");
    let t = head_text(
      &h,
      None,
      HeadOptions {
        host: "h",
        user_agent: "ua",
        accept: "*/*",
        cookie: "a=b",
        ..HeadOptions::default()
      },
    );
    assert_eq!(
      t,
      "GET /p?q=1 HTTP/1.1\r\nHost: h\r\nuser-agent: mine\r\nACCEPT: text/plain\r\nCookie: a=b\r\n\r\n"
    );
  }

  #[test]
  fn closes_tracks_caller_connection_close() {
    let mut h = Headers::new();
    h.insert("Connection", "Upgrade, close");
    let head = RequestHead::new(
      "GET",
      "/",
      None,
      &h,
      None,
      HeadOptions {
        host: "h",
        ..HeadOptions::default()
      },
    )
    .unwrap();
    assert!(head.closes());
  }

  #[test]
  fn slice_sink_reports_overflow() {
    let h = Headers::new();
    let head = RequestHead::new(
      "GET",
      "/",
      None,
      &h,
      None,
      HeadOptions {
        host: "h",
        ..HeadOptions::default()
      },
    )
    .unwrap();
    let mut tiny = [0u8; 8];
    let mut sink = SliceSink::new(&mut tiny);
    head.write_to(&mut sink);
    assert_eq!(sink.finish(), None);
  }

  #[test]
  fn decimal_len_matches_put_decimal() {
    for n in [0usize, 9, 10, 99, 100, 65535, usize::MAX] {
      let mut v = Vec::new();
      put_decimal(&mut v, n);
      assert_eq!(v.len(), decimal_len(n), "{n}");
      assert_eq!(core::str::from_utf8(&v).unwrap(), alloc::format!("{n}"));
    }
  }
}
