extern crate alloc;
use crate::error::ParseError;
use crate::headers::Headers;
use crate::parser::headers::is_token_char;
use alloc::vec::Vec;

/// Serialize an HTTP/1.1 request to wire bytes.
///
/// # Errors
/// [`ParseError::MissingHostHeader`], host / TE / framing violations of RFC 9112.
pub fn serialize_request(
  method: &str,
  path: &str,
  headers: &Headers,
  body: Option<&[u8]>,
) -> Result<Vec<u8>, ParseError> {
  // RFC 9112 Section 3.2: Client MUST send Host in every HTTP/1.1 request
  if !headers.contains(Headers::HOST) {
    return Err(ParseError::MissingHostHeader);
  }

  // RFC 9112 Section 3.2: Server responds 400 if multiple Host headers present
  let host_headers = headers.get_all(Headers::HOST);
  if host_headers.len() > 1 {
    return Err(ParseError::MultipleHostHeaders);
  }

  let host_value = *host_headers.first().ok_or(ParseError::MissingHostHeader)?;
  if !is_valid_host_field_value(host_value) {
    return Err(ParseError::InvalidHostHeaderValue);
  }

  // RFC 9112 §7.4: TE must not list chunked; sender of TE MUST also send Connection: TE
  validate_te_header(headers)?;

  // This client frames request bodies with Content-Length only (RFC 9112 §6.3).
  let has_te = headers.contains(Headers::TRANSFER_ENCODING);
  let has_cl = headers.contains(Headers::CONTENT_LENGTH);
  if has_te && has_cl {
    return Err(ParseError::ConflictingFraming);
  }
  if has_te {
    return Err(ParseError::RequestTransferEncodingUnsupported);
  }

  // Validate all header names/values for RFC 9112 compliance (no injection)
  for (name, value) in headers {
    if name.is_empty() || !name.bytes().all(is_token_char) {
      return Err(ParseError::InvalidHeaderName);
    }
    // No CTLs except HTAB; blocks CRLF / LF injection into the wire message
    if value
      .bytes()
      .any(|b| matches!(b, 0..=8 | 0x0A..=0x1F | 0x7F))
    {
      return Err(ParseError::InvalidHeaderValue);
    }
  }

  // Body length is authoritative: reject a mismatched Content-Length.
  if let Some(body_bytes) = body
    && let Some(cl_val) = headers.get(Headers::CONTENT_LENGTH)
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

  let mut request = Vec::new();

  request.extend_from_slice(method.as_bytes());
  request.push(b' ');
  request.extend_from_slice(request_path.as_bytes());
  // RFC 9112: this is an HTTP/1.1 client — request-line version is always 1.1
  request.extend_from_slice(b" HTTP/1.1\r\n");

  // RFC 9110 §7.2: user agent SHOULD send Host as the first header field
  write_header_line(&mut request, Headers::HOST, host_value);
  for (name, value) in headers {
    if name.eq_ignore_ascii_case(Headers::HOST) {
      continue;
    }
    write_header_line(&mut request, name, value);
  }

  if let Some(body_bytes) = body
    && !headers.contains(Headers::CONTENT_LENGTH)
  {
    use alloc::string::ToString;
    request.extend_from_slice(b"Content-Length: ");
    request.extend_from_slice(body_bytes.len().to_string().as_bytes());
    request.extend_from_slice(b"\r\n");
  }

  request.extend_from_slice(b"\r\n");

  if let Some(body_bytes) = body {
    request.extend_from_slice(body_bytes);
  }

  Ok(request)
}

fn write_header_line(
  out: &mut Vec<u8>,
  name: &str,
  value: &str,
) {
  out.extend_from_slice(name.as_bytes());
  out.extend_from_slice(b": ");
  out.extend_from_slice(value.as_bytes());
  out.extend_from_slice(b"\r\n");
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

fn validate_te_header(headers: &Headers) -> Result<(), ParseError> {
  if !headers.contains(Headers::TE) {
    return Ok(());
  }

  for value in headers.get_all(Headers::TE) {
    for coding in value.split(',') {
      let name = coding.trim().split(';').next().unwrap_or("").trim();
      if name.eq_ignore_ascii_case("chunked") {
        return Err(ParseError::ChunkedInTeHeader);
      }
    }
  }

  let connection_has_te = headers
    .get_all(Headers::CONNECTION)
    .iter()
    .any(|v| v.split(',').any(|t| t.trim().eq_ignore_ascii_case("TE")));
  if !connection_has_te {
    return Err(ParseError::TeHeaderMissingConnection);
  }
  Ok(())
}
