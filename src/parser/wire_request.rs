extern crate alloc;
use crate::error::ParseError;
use crate::headers::{Headers, WellKnownHeader, well_known_header};
use crate::parser::headers::is_token_char;
use bytes::{Bytes, BytesMut};

/// Wire request with header block and body kept separate (no concat copy).
///
/// `head` is request-line + fields + terminating blank line. `body` borrows the
/// caller buffer so large payloads are not copied into the header allocation.
#[derive(Debug, Clone)]
pub struct SerializedRequest<'a> {
  /// Request-line, headers, and final `\r\n` (no entity body).
  pub head: Bytes,
  /// Entity body octets (empty when no body).
  pub body: &'a [u8],
}

impl SerializedRequest<'_> {
  /// Contiguous wire image (tests / assertions). Copies the body when present.
  #[cfg(test)]
  #[must_use]
  pub fn to_bytes(&self) -> Bytes {
    if self.body.is_empty() {
      return self.head.clone();
    }
    let mut out = BytesMut::with_capacity(self.head.len().saturating_add(self.body.len()));
    out.extend_from_slice(&self.head);
    out.extend_from_slice(self.body);
    out.freeze()
  }
}

/// Serialize an HTTP/1.1 request: header block in [`SerializedRequest::head`], body uncopied.
///
/// # Errors
/// [`ParseError::MissingHostHeader`], host / TE / framing violations of RFC 9112.
pub fn serialize_request<'a>(
  method: &str,
  path: &str,
  headers: &Headers,
  body: Option<&'a [u8]>,
) -> Result<SerializedRequest<'a>, ParseError> {
  let mut host_value: Option<&str> = None;
  let mut host_count = 0usize;
  let mut has_te = false;
  let mut has_cl = false;
  let mut cl_value: Option<&str> = None;
  let mut has_te_field = false;
  let mut te_lists_chunked = false;
  let mut connection_has_te = false;
  let mut wire_bytes = method.len().saturating_add(1).saturating_add(11); // " METHOD" + " HTTP/1.1\r\n"

  // Single scan: Host / framing / TE rules / injection checks / size estimate.
  for (name, value) in headers {
    if name.is_empty() || !name.bytes().all(is_token_char) {
      return Err(ParseError::InvalidHeaderName);
    }
    if value
      .bytes()
      .any(|b| matches!(b, 0..=8 | 0x0A..=0x1F | 0x7F))
    {
      return Err(ParseError::InvalidHeaderValue);
    }

    wire_bytes = wire_bytes
      .saturating_add(name.len())
      .saturating_add(value.len())
      .saturating_add(4); // ": \r\n"

    match well_known_header(name) {
      Some(WellKnownHeader::Host) => {
        host_count = host_count.saturating_add(1);
        host_value = Some(value);
      },
      Some(WellKnownHeader::TransferEncoding) => {
        has_te = true;
      },
      Some(WellKnownHeader::ContentLength) => {
        has_cl = true;
        cl_value = Some(value);
      },
      Some(WellKnownHeader::Te) => {
        has_te_field = true;
        for coding in value.split(',') {
          let coding_name = coding.trim().split(';').next().unwrap_or("").trim();
          if coding_name.eq_ignore_ascii_case("chunked") {
            te_lists_chunked = true;
          }
        }
      },
      Some(WellKnownHeader::Connection)
        if value
          .split(',')
          .any(|t| t.trim().eq_ignore_ascii_case("TE")) =>
      {
        connection_has_te = true;
      },
      _ => {},
    }
  }

  // RFC 9112 Section 3.2: Client MUST send Host in every HTTP/1.1 request
  let host = host_value.ok_or(ParseError::MissingHostHeader)?;
  if host_count > 1 {
    return Err(ParseError::MultipleHostHeaders);
  }
  if !is_valid_host_field_value(host) {
    return Err(ParseError::InvalidHostHeaderValue);
  }

  // RFC 9112 §7.4: TE must not list chunked; sender of TE MUST also send Connection: TE
  if te_lists_chunked {
    return Err(ParseError::ChunkedInTeHeader);
  }
  if has_te_field && !connection_has_te {
    return Err(ParseError::TeHeaderMissingConnection);
  }

  // This client frames request bodies with Content-Length only (RFC 9112 §6.3).
  if has_te && has_cl {
    return Err(ParseError::ConflictingFraming);
  }
  if has_te {
    return Err(ParseError::RequestTransferEncodingUnsupported);
  }

  let body_bytes = body.unwrap_or(&[]);

  // Body length is authoritative: reject a mismatched Content-Length.
  if body.is_some()
    && let Some(cl_val) = cl_value
  {
    let parsed = cl_val
      .trim()
      .parse::<usize>()
      .map_err(|_| ParseError::InvalidContentLength)?;
    if parsed != body_bytes.len() {
      return Err(ParseError::InvalidContentLength);
    }
  }

  // RFC 9112 §3.2.1: origin-form is absolute-path ["?" query]; empty path → "/"
  // (Direct-to-origin client: never absolute-form.)
  let request_path = if path.is_empty() {
    "/"
  } else {
    path
  };
  if !is_origin_form_request_target(request_path) {
    return Err(ParseError::InvalidUri);
  }
  wire_bytes = wire_bytes
    .saturating_add(request_path.len())
    .saturating_add(2); // final CRLF

  let inject_cl = body.is_some() && !has_cl;
  if inject_cl {
    // "Content-Length: " + digits + "\r\n" — digits ≤ 20 for usize.
    wire_bytes = wire_bytes.saturating_add(32);
  }
  // Capacity is header block only — body is written from the caller slice.

  let mut request = BytesMut::with_capacity(wire_bytes);

  request.extend_from_slice(method.as_bytes());
  request.extend_from_slice(b" ");
  request.extend_from_slice(request_path.as_bytes());
  // RFC 9112: this is an HTTP/1.1 client — request-line version is always 1.1
  request.extend_from_slice(b" HTTP/1.1\r\n");

  // RFC 9110 §7.2: user agent SHOULD send Host as the first header field
  write_header_line(&mut request, Headers::HOST, host);
  for (name, value) in headers {
    // Cheap Host skip (already validated once above); avoid a second PHF probe.
    if name.eq_ignore_ascii_case(Headers::HOST) {
      continue;
    }
    write_header_line(&mut request, name, value);
  }

  if inject_cl {
    request.extend_from_slice(b"Content-Length: ");
    push_usize_decimal(&mut request, body_bytes.len());
    request.extend_from_slice(b"\r\n");
  }

  request.extend_from_slice(b"\r\n");

  Ok(SerializedRequest {
    head: request.freeze(),
    body: body_bytes,
  })
}

fn write_header_line(
  out: &mut BytesMut,
  name: &str,
  value: &str,
) {
  out.extend_from_slice(name.as_bytes());
  out.extend_from_slice(b": ");
  out.extend_from_slice(value.as_bytes());
  out.extend_from_slice(b"\r\n");
}

/// Decimal digits of `n` with no heap allocation (`usize` ≤ 20 digits).
fn push_usize_decimal(
  out: &mut BytesMut,
  mut n: usize,
) {
  let mut tmp = [0u8; 20];
  let mut i = tmp.len();
  if n == 0 {
    out.extend_from_slice(b"0");
    return;
  }
  while n > 0 {
    i = i.saturating_sub(1);
    if let Some(slot) = tmp.get_mut(i) {
      #[allow(clippy::cast_possible_truncation)] // n % 10 fits in u8
      {
        *slot = b'0' + (n % 10) as u8;
      }
    }
    n /= 10;
  }
  if let Some(digits) = tmp.get(i..) {
    out.extend_from_slice(digits);
  }
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
fn is_valid_host_field_value(value: &str) -> bool {
  let host = value.trim();
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
    return match rest.strip_prefix(':') {
      Some(port) if !port.is_empty() => port.bytes().all(|b| b.is_ascii_digit()),
      _ => false,
    };
  }
  if let Some((name, port)) = host.rsplit_once(':')
    && (name.is_empty() || port.is_empty() || !port.bytes().all(|b| b.is_ascii_digit()))
  {
    return false;
  }
  true
}
