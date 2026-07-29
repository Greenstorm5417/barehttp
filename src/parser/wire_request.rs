extern crate alloc;
use crate::error::ParseError;
use crate::headers::Headers;
use crate::parser::headers::is_token_char;
use alloc::vec::Vec;

/// Serialize an HTTP/1.1 request to wire bytes.
///
/// # Errors
/// Returns [`ParseError`] if headers or framing violate RFC 9112.
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

  // RFC 9112 Section 6.2: Sender MUST NOT send CL when TE present
  let has_te = headers.contains(Headers::TRANSFER_ENCODING);
  let has_cl = headers.contains(Headers::CONTENT_LENGTH);
  if has_te && has_cl {
    return Err(ParseError::ConflictingFraming);
  }

  let mut request = Vec::new();

  request.extend_from_slice(method.as_bytes());
  request.push(b' ');

  // RFC 9112 Section 3.2.1: If origin-form path is empty, send "/"
  let request_path = if path.is_empty() {
    "/"
  } else {
    path
  };
  request.extend_from_slice(request_path.as_bytes());
  request.extend_from_slice(b" HTTP/1.1\r\n");

  for (name, value) in headers {
    request.extend_from_slice(name.as_bytes());
    request.extend_from_slice(b": ");
    request.extend_from_slice(value.as_bytes());
    request.extend_from_slice(b"\r\n");
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
