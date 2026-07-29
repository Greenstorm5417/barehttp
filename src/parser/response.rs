//! Parsed HTTP/1.1 responses and body-read strategy.

extern crate alloc;
use crate::body::Body;
use crate::error::ParseError;
use crate::headers::Headers;
use crate::parser::chunked::ChunkedDecoder;
use crate::parser::headers::HeaderField;
use crate::parser::status_line::StatusLine;
use crate::parser::version::Version;
use alloc::string::String;
use alloc::vec::Vec;

#[cfg(feature = "gzip-decompression")]
use miniz_oxide::inflate::{decompress_to_vec, decompress_to_vec_zlib};

#[cfg(feature = "zstd-decompression")]
use ruzstd::decoding::StreamingDecoder;

/// Parsed HTTP response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
  /// Status code (e.g. 200).
  pub status_code: u16,
  /// Reason phrase from the status line.
  pub reason: String,
  /// Response headers.
  pub headers: Headers,
  /// Response body.
  pub body: Body,
  /// Trailer fields from chunked responses (RFC 9112 §7.1.2).
  pub trailers: Vec<(String, String)>,
}

impl Response {
  /// Parse HTTP/1.1 response with RFC 9112 robustness features.
  ///
  /// Per Section 2.2: clients MAY skip leading empty lines before status-line.
  /// Per Section 5.2: clients MUST handle obsolete line folding (obs-fold).
  ///
  /// # Errors
  /// Returns [`ParseError`] if the message is malformed.
  pub fn parse(input: &[u8]) -> Result<Self, ParseError> {
    // RFC 9112 Section 2.2: Skip leading CRLF (robustness)
    let mut data = input;
    loop {
      if data.len() >= 2 {
        let byte0 = data.first().copied();
        let byte1 = data.get(1).copied();
        if byte0 == Some(b'\r') && byte1 == Some(b'\n') {
          data = data.get(2..).unwrap_or(&[]);
          continue;
        }
      }
      if !data.is_empty() {
        let byte0 = data.first().copied();
        if byte0 == Some(b'\n') {
          data = data.get(1..).unwrap_or(&[]);
          continue;
        }
      }
      break;
    }

    let (status_line, after_status) = StatusLine::parse(data)?;

    // RFC 9112 Section 5.2: Use obs-fold aware parsing for responses
    let (headers_bytes, remaining) = HeaderField::parse(after_status)?;

    let mut headers = Vec::new();
    for (name_bytes, value_bytes) in &headers_bytes {
      let name_str = String::from_utf8_lossy(name_bytes).into_owned();
      let value_str = String::from_utf8_lossy(value_bytes).into_owned();
      headers.push((name_str, value_str));
    }

    let (body_bytes, trailer_bytes) = Self::parse_body_internal(
      remaining,
      &headers_bytes,
      Some(status_line.version),
      status_line.status.as_u16(),
      None,
    )?;

    let trailers = trailer_bytes
      .into_iter()
      .map(|(name, value)| {
        (
          String::from_utf8_lossy(&name).into_owned(),
          String::from_utf8_lossy(&value).into_owned(),
        )
      })
      .collect();

    let body = Self::decompress_body_if_needed(&Headers::from_vec(headers.clone()), body_bytes)?;

    Ok(Self {
      status_code: status_line.status.as_u16(),
      reason: String::from_utf8_lossy(status_line.reason).into_owned(),
      headers: Headers::from_vec(headers),
      body: Body::from_bytes(body),
      trailers,
    })
  }

  fn decompress_body_if_needed(
    headers: &Headers,
    mut body_bytes: Vec<u8>,
  ) -> Result<Vec<u8>, ParseError> {
    let encodings = headers.get_all("content-encoding");
    if encodings.is_empty() {
      return Ok(body_bytes);
    }

    // RFC 9110: comma-separated codings, applied in listed order → decompress reverse.
    let tokens: Vec<&str> = encodings
      .iter()
      .flat_map(|v| v.split(','))
      .map(str::trim)
      .filter(|t| !t.is_empty() && !t.eq_ignore_ascii_case("identity"))
      .collect();

    if tokens.is_empty() {
      return Ok(body_bytes);
    }

    for coding in tokens.iter().rev() {
      body_bytes = decompress_coding(coding, body_bytes)?;
    }
    Ok(body_bytes)
  }

  /// Parse a response body given raw header pairs (test helper).
  ///
  /// # Errors
  /// Returns [`ParseError`] if framing or decoding fails.
  #[cfg(test)]
  pub fn parse_body(
    input: &[u8],
    headers: &[(Vec<u8>, Vec<u8>)],
    status_code: u16,
    method: Option<&str>,
  ) -> Result<Vec<u8>, ParseError> {
    let (body, _trailers) = Self::parse_body_internal(input, headers, None, status_code, method)?;
    Ok(body)
  }

  fn parse_body_internal(
    input: &[u8],
    headers: &[(Vec<u8>, Vec<u8>)],
    version: Option<Version>,
    status_code: u16,
    method: Option<&str>,
  ) -> Result<(Vec<u8>, Vec<(Vec<u8>, Vec<u8>)>), ParseError> {
    // Check if Transfer-Encoding is present
    let has_transfer_encoding = headers
      .iter()
      .any(|(name, _)| name.eq_ignore_ascii_case(Headers::TRANSFER_ENCODING.as_bytes()));

    // RFC 9112 Section 6.1: Transfer-Encoding is a feature of HTTP/1.1.
    // Reject TE in an HTTP/1.0 response.
    if has_transfer_encoding
      && let Some(v) = version
      && v != Version::HTTP_11
    {
      return Err(ParseError::TransferEncodingRequiresHttp11);
    }

    // RFC 9112 Section 6.1: Server MUST NOT send Transfer-Encoding in:
    // - Any 1xx (informational) response
    // - 204 (No Content) response
    // Note: For 2xx CONNECT responses, RFC 9112 Section 6.3 says clients should
    // IGNORE (not reject) TE/CL headers, so we don't validate that case here.
    if has_transfer_encoding {
      if (100..200).contains(&status_code) {
        return Err(ParseError::InvalidTransferEncodingForStatus);
      }
      if status_code == 204 {
        return Err(ParseError::InvalidTransferEncodingForStatus);
      }
    }

    // RFC 9112 Section 6.3: 2xx to CONNECT ignores CL/TE
    if method == Some("CONNECT") && (200..300).contains(&status_code) {
      return Ok((Vec::new(), Vec::new()));
    }

    if (100..200).contains(&status_code) || status_code == 204 || status_code == 304 {
      return Ok((Vec::new(), Vec::new()));
    }

    let content_length = resolve_content_length(headers)?;

    // RFC 9112 Section 6.3: If both Transfer-Encoding and Content-Length are present,
    // this is a potential request smuggling attack. Client MUST close connection
    // and discard the response.
    if has_transfer_encoding && content_length.is_some() {
      return Err(ParseError::ConflictingFraming);
    }

    if has_transfer_encoding {
      // RFC 9112: multiple Transfer-Encoding fields are equivalent to a comma-joined list
      let te_str = headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case(Headers::TRANSFER_ENCODING.as_bytes()))
        .filter_map(|(_, value)| core::str::from_utf8(value).ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(",")
        .to_lowercase();

      let last = te_str
        .split(',')
        .map(str::trim)
        .rfind(|t| !t.is_empty())
        .unwrap_or("");
      let is_chunked_final = last == "chunked";

      if !is_chunked_final && te_str.split(',').map(str::trim).any(|t| t == "chunked") {
        return Err(ParseError::ChunkedNotFinal);
      }

      if is_chunked_final {
        let mut decoder = ChunkedDecoder::new();
        let mut output = Vec::new();
        // RFC 9112 Section 8: Handle incomplete chunked message
        let remaining = decoder.decode_chunk(input, &mut output)?;

        // RFC 9112 Section 6.3: Client MUST NOT process/cache/forward extra data
        if !remaining.is_empty() {
          return Err(ParseError::ExtraDataAfterResponse);
        }

        // RFC 9112 Section 7.1.2: trailer fields from chunked response
        let trailer_fields = decoder.trailers();
        return Ok((output, trailer_fields.to_vec()));
      }

      // RFC 9112 Section 6.3: TE present but not chunked → read until connection closes
      return Ok((input.to_vec(), Vec::new()));
    }

    if let Some(len) = content_length {
      // RFC 9112 Section 8: A message with valid Content-Length is incomplete
      // if the size received is less than the value given by Content-Length
      if input.len() < len {
        return Err(ParseError::UnexpectedEndOfInput);
      }
      let body_data = input.get(..len).ok_or(ParseError::UnexpectedEndOfInput)?;

      // RFC 9112 Section 6.3: Client MUST NOT process/cache/forward extra data
      // Check if there's extra data beyond Content-Length
      if input.len() > len {
        return Err(ParseError::ExtraDataAfterResponse);
      }

      return Ok((body_data.to_vec(), Vec::new()));
    }

    // RFC 9112 §6.3: no framing → body ends at connection close (all remaining bytes here)
    Ok((input.to_vec(), Vec::new()))
  }

  /// Look up a header by name (case-insensitive).
  #[must_use]
  pub fn get_header(
    &self,
    name: &str,
  ) -> Option<&str> {
    self.headers.get(name)
  }

  /// Parse response headers only (for two-phase reading).
  ///
  /// Returns (`status_code`, reason, headers, version, `remaining_bytes_after_headers`).
  ///
  /// # Errors
  /// Returns [`ParseError`] if the status line or headers are invalid.
  pub fn parse_headers_only(input: &[u8]) -> Result<(u16, String, Headers, Version, &[u8]), ParseError> {
    // Skip leading CRLF (RFC 9112 Section 2.2 robustness)
    let mut data = input;
    loop {
      if data.len() >= 2 {
        let byte0 = data.first().copied();
        let byte1 = data.get(1).copied();
        if byte0 == Some(b'\r') && byte1 == Some(b'\n') {
          data = data.get(2..).unwrap_or(&[]);
          continue;
        }
      }
      if !data.is_empty() {
        let byte0 = data.first().copied();
        if byte0 == Some(b'\n') {
          data = data.get(1..).unwrap_or(&[]);
          continue;
        }
      }
      break;
    }

    let (status_line, after_status) = StatusLine::parse(data)?;

    // RFC 9112 Section 5.2: Use obs-fold aware parsing for responses
    let (headers_bytes, remaining) = HeaderField::parse(after_status)?;

    let mut headers = Vec::new();
    for (name_bytes, value_bytes) in &headers_bytes {
      let name_str = String::from_utf8_lossy(name_bytes).into_owned();
      let value_str = String::from_utf8_lossy(value_bytes).into_owned();
      headers.push((name_str, value_str));
    }

    Ok((
      status_line.status.as_u16(),
      String::from_utf8_lossy(status_line.reason).into_owned(),
      Headers::from_vec(headers),
      status_line.version,
      remaining,
    ))
  }

  /// Determine how many bytes to read for the response body.
  ///
  /// # Errors
  /// Returns [`ParseError::InvalidContentLength`] or [`ParseError::ChunkedNotFinal`]
  /// when framing headers are malformed.
  pub fn body_read_strategy(
    headers: &Headers,
    status_code: u16,
  ) -> Result<BodyReadStrategy, ParseError> {
    // No body for certain status codes
    if (100..200).contains(&status_code) || status_code == 204 || status_code == 304 {
      return Ok(BodyReadStrategy::NoBody);
    }

    let has_transfer_encoding = headers
      .iter()
      .any(|(name, _)| name.eq_ignore_ascii_case(Headers::TRANSFER_ENCODING));

    // RFC 9112: Transfer-Encoding overrides Content-Length — both present is an error
    if has_transfer_encoding {
      if headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case(Headers::CONTENT_LENGTH))
      {
        return Err(ParseError::ConflictingFraming);
      }

      // Multiple TE fields ≡ comma-joined list
      let te = headers
        .get_all(Headers::TRANSFER_ENCODING)
        .iter()
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .collect::<Vec<_>>()
        .join(",")
        .to_lowercase();

      let last = te
        .split(',')
        .map(str::trim)
        .rfind(|t| !t.is_empty())
        .unwrap_or("");
      if last == "chunked" {
        return Ok(BodyReadStrategy::Chunked);
      }
      if te.split(',').map(str::trim).any(|t| t == "chunked") {
        return Err(ParseError::ChunkedNotFinal);
      }
      return Ok(BodyReadStrategy::UntilClose);
    }

    if let Some(len) = resolve_content_length_str(headers)? {
      return Ok(BodyReadStrategy::ContentLength(len));
    }

    // RFC 9112 §6.3: no framing headers → message body ends when connection closes
    Ok(BodyReadStrategy::UntilClose)
  }

  /// Parse body from remaining bytes after headers (for two-phase reading).
  ///
  /// Returns decoded body and any chunked trailer fields.
  ///
  /// # Errors
  /// Returns [`ParseError`] if the body cannot be decoded.
  pub fn parse_body_from_bytes(
    body_bytes: &[u8],
    headers: &Headers,
    status_code: u16,
    version: Version,
  ) -> Result<(Body, Vec<(String, String)>), ParseError> {
    if (100..200).contains(&status_code) || status_code == 204 || status_code == 304 {
      return Ok((Body::from_bytes(Vec::new()), Vec::new()));
    }

    let headers_bytes: Vec<(Vec<u8>, Vec<u8>)> = headers
      .iter()
      .map(|(k, v)| (k.as_bytes().to_vec(), v.as_bytes().to_vec()))
      .collect();

    let (body_vec, trailer_bytes) =
      Self::parse_body_internal(body_bytes, &headers_bytes, Some(version), status_code, None)?;

    let trailers = trailer_bytes
      .into_iter()
      .map(|(name, value)| {
        (
          String::from_utf8_lossy(&name).into_owned(),
          String::from_utf8_lossy(&value).into_owned(),
        )
      })
      .collect();

    // Decompress if needed
    let decompressed_body = Self::decompress_body_if_needed(headers, body_vec)?;
    Ok((Body::from_bytes(decompressed_body), trailers))
  }

  /// Response headers.
  #[must_use]
  pub const fn headers(&self) -> &Headers {
    &self.headers
  }

  /// Mutable response headers.
  pub const fn headers_mut(&mut self) -> &mut Headers {
    &mut self.headers
  }

  /// Response body.
  #[must_use]
  pub const fn body(&self) -> &Body {
    &self.body
  }

  /// Mutable response body.
  pub const fn body_mut(&mut self) -> &mut Body {
    &mut self.body
  }

  /// `true` if `Connection` contains the `close` token (case-insensitive).
  #[must_use]
  pub fn has_connection_close(&self) -> bool {
    self
      .headers
      .get(Headers::CONNECTION)
      .is_some_and(|val| {
        val
          .split(',')
          .map(str::trim)
          .any(|token| token.eq_ignore_ascii_case("close"))
      })
  }

  /// `true` if status is 2xx.
  #[must_use]
  pub const fn is_success(&self) -> bool {
    matches!(self.status_code, 200..300)
  }

  /// `true` if status is 3xx.
  #[must_use]
  pub const fn is_redirect(&self) -> bool {
    matches!(self.status_code, 300..400)
  }

  /// `true` if status is 4xx.
  #[must_use]
  pub const fn is_client_error(&self) -> bool {
    matches!(self.status_code, 400..500)
  }

  /// `true` if status is 5xx.
  #[must_use]
  pub const fn is_server_error(&self) -> bool {
    matches!(self.status_code, 500..600)
  }

  /// HTTP status code.
  #[must_use]
  pub const fn status(&self) -> u16 {
    self.status_code
  }

  /// All `Set-Cookie` header values.
  #[must_use]
  pub fn cookies(&self) -> Vec<&str> {
    self.headers.get_all("Set-Cookie")
  }

  /// Body as UTF-8 string.
  ///
  /// # Errors
  /// Returns [`crate::Error::Utf8Error`] if the body is not valid UTF-8.
  pub fn text(&self) -> Result<String, crate::Error> {
    self.body.to_string().map_err(Into::into)
  }

  /// Body as a byte slice.
  #[must_use]
  pub fn bytes(&self) -> &[u8] {
    self.body.as_bytes()
  }

  /// Consume the response and return body bytes.
  #[must_use]
  pub fn into_bytes(self) -> Vec<u8> {
    self.body.into_bytes()
  }
}

/// Strategy for reading response body
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyReadStrategy {
  /// No body expected
  NoBody,
  /// Read exactly n bytes
  ContentLength(usize),
  /// Read chunks until terminating chunk
  Chunked,
  /// Read until connection closes
  UntilClose,
}

fn decompress_coding(
  coding: &str,
  body_bytes: Vec<u8>,
) -> Result<Vec<u8>, ParseError> {
  #[cfg(feature = "gzip-decompression")]
  if coding.eq_ignore_ascii_case("gzip") {
    let deflate_data = gzip_deflate_payload(&body_bytes).ok_or(ParseError::DecompressionFailed)?;
    return decompress_to_vec(deflate_data).map_err(|_| ParseError::DecompressionFailed);
  }

  #[cfg(feature = "gzip-decompression")]
  if coding.eq_ignore_ascii_case("deflate") {
    return decompress_to_vec_zlib(&body_bytes).map_err(|_| ParseError::DecompressionFailed);
  }

  #[cfg(feature = "zstd-decompression")]
  if coding.eq_ignore_ascii_case("zstd") {
    use ruzstd::io_nostd::Read;
    let mut decoder =
      StreamingDecoder::new(&body_bytes[..]).map_err(|_| ParseError::DecompressionFailed)?;
    let mut decompressed = Vec::new();
    decoder
      .read_to_end(&mut decompressed)
      .map_err(|_| ParseError::DecompressionFailed)?;
    return Ok(decompressed);
  }

  let _ = (coding, body_bytes);
  Err(ParseError::DecompressionFailed)
}

/// RFC 1952: skip gzip header/footer, return raw deflate payload.
#[cfg(feature = "gzip-decompression")]
fn gzip_deflate_payload(data: &[u8]) -> Option<&[u8]> {
  if data.len() < 18 {
    return None;
  }
  if data.first().copied() != Some(0x1f) || data.get(1).copied() != Some(0x8b) {
    return None;
  }
  let flags = data.get(3).copied()?;
  let mut i = 10usize;
  // FEXTRA
  if flags & 0x04 != 0 {
    let b0 = data.get(i).copied()?;
    let b1 = data.get(i + 1).copied()?;
    let xlen = usize::from(u16::from_le_bytes([b0, b1]));
    i = i.checked_add(2)?.checked_add(xlen)?;
  }
  // FNAME
  if flags & 0x08 != 0 {
    while *data.get(i)? != 0 {
      i = i.checked_add(1)?;
    }
    i = i.checked_add(1)?;
  }
  // FCOMMENT
  if flags & 0x10 != 0 {
    while *data.get(i)? != 0 {
      i = i.checked_add(1)?;
    }
    i = i.checked_add(1)?;
  }
  // FHCRC
  if flags & 0x02 != 0 {
    i = i.checked_add(2)?;
  }
  let end = data.len().checked_sub(8)?;
  if end < i {
    return None;
  }
  data.get(i..end)
}

fn parse_content_length(value: &[u8]) -> Option<usize> {
  let s = core::str::from_utf8(value).ok()?;
  parse_content_length_str(s)
}

fn parse_content_length_str(value: &str) -> Option<usize> {
  let trimmed = value.trim();
  if trimmed.is_empty() {
    return None;
  }

  // RFC 9112 Section 6.3: Check for multiple values (comma-separated)
  if trimmed.contains(',') {
    // RFC 9112 allows comma-separated identical values
    let parts: Vec<&str> = trimmed.split(',').map(str::trim).collect();
    let first = parts.first()?.parse::<usize>().ok()?;
    // All values must be identical
    if parts.iter().all(|p| p.parse::<usize>().ok() == Some(first)) {
      return Some(first);
    }
    return None;
  }

  // Check for invalid characters (only digits allowed)
  if !trimmed.chars().all(|c| c.is_ascii_digit()) {
    return None;
  }

  trimmed.parse().ok()
}

/// Resolve Content-Length from raw header pairs. Present-but-invalid → error.
fn resolve_content_length(headers: &[(Vec<u8>, Vec<u8>)]) -> Result<Option<usize>, ParseError> {
  let mut resolved: Option<usize> = None;
  let mut seen = false;
  for (name, value) in headers {
    if !name.eq_ignore_ascii_case(Headers::CONTENT_LENGTH.as_bytes()) {
      continue;
    }
    seen = true;
    let len = parse_content_length(value).ok_or(ParseError::InvalidContentLength)?;
    match resolved {
      None => resolved = Some(len),
      Some(prev) if prev != len => return Err(ParseError::InvalidContentLength),
      _ => {},
    }
  }
  if seen {
    Ok(resolved)
  } else {
    Ok(None)
  }
}

/// Resolve Content-Length from [`Headers`]. Present-but-invalid → error.
fn resolve_content_length_str(headers: &Headers) -> Result<Option<usize>, ParseError> {
  let mut resolved: Option<usize> = None;
  let mut seen = false;
  for (name, value) in headers.iter() {
    if !name.eq_ignore_ascii_case(Headers::CONTENT_LENGTH) {
      continue;
    }
    seen = true;
    let len = parse_content_length_str(value).ok_or(ParseError::InvalidContentLength)?;
    match resolved {
      None => resolved = Some(len),
      Some(prev) if prev != len => return Err(ParseError::InvalidContentLength),
      _ => {},
    }
  }
  if seen {
    Ok(resolved)
  } else {
    Ok(None)
  }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod response_helpers_tests {
  use super::*;
  use crate::body::Body;

  fn make_response(
    status_code: u16,
    body: &[u8],
  ) -> Response {
    Response {
      status_code,
      reason: String::from("Test"),
      headers: Headers::new(),
      body: Body::from_bytes(body.to_vec()),
      trailers: Vec::new(),
    }
  }

  #[test]
  fn is_success_true_for_2xx() {
    assert!(make_response(200, b"").is_success());
    assert!(make_response(201, b"").is_success());
    assert!(make_response(204, b"").is_success());
    assert!(make_response(299, b"").is_success());
  }

  #[test]
  fn is_success_false_for_non_2xx() {
    assert!(!make_response(199, b"").is_success());
    assert!(!make_response(300, b"").is_success());
    assert!(!make_response(404, b"").is_success());
    assert!(!make_response(500, b"").is_success());
  }

  #[test]
  fn is_redirect_true_for_3xx() {
    assert!(make_response(300, b"").is_redirect());
    assert!(make_response(301, b"").is_redirect());
    assert!(make_response(302, b"").is_redirect());
    assert!(make_response(307, b"").is_redirect());
    assert!(make_response(399, b"").is_redirect());
  }

  #[test]
  fn is_redirect_false_for_non_3xx() {
    assert!(!make_response(299, b"").is_redirect());
    assert!(!make_response(400, b"").is_redirect());
  }

  #[test]
  fn is_client_error_true_for_4xx() {
    assert!(make_response(400, b"").is_client_error());
    assert!(make_response(404, b"").is_client_error());
    assert!(make_response(403, b"").is_client_error());
    assert!(make_response(499, b"").is_client_error());
  }

  #[test]
  fn is_client_error_false_for_non_4xx() {
    assert!(!make_response(399, b"").is_client_error());
    assert!(!make_response(500, b"").is_client_error());
  }

  #[test]
  fn is_server_error_true_for_5xx() {
    assert!(make_response(500, b"").is_server_error());
    assert!(make_response(502, b"").is_server_error());
    assert!(make_response(503, b"").is_server_error());
    assert!(make_response(599, b"").is_server_error());
  }

  #[test]
  fn is_server_error_false_for_non_5xx() {
    assert!(!make_response(499, b"").is_server_error());
    assert!(!make_response(600, b"").is_server_error());
  }

  #[test]
  fn status_returns_status_code() {
    assert_eq!(make_response(200, b"").status(), 200);
    assert_eq!(make_response(404, b"").status(), 404);
    assert_eq!(make_response(500, b"").status(), 500);
  }

  #[test]
  fn cookies_returns_set_cookie_headers() {
    let mut headers = Headers::new();
    headers.insert("Set-Cookie", "session=abc");
    headers.insert("Set-Cookie", "user=john");

    let response = Response {
      status_code: 200,
      reason: String::from("OK"),
      headers,
      body: Body::from_bytes(Vec::new()),
      trailers: Vec::new(),
    };

    let cookies = response.cookies();
    assert_eq!(cookies.len(), 2);
    assert!(cookies.contains(&"session=abc"));
    assert!(cookies.contains(&"user=john"));
  }

  #[test]
  fn text_converts_utf8_body() {
    let response = make_response(200, b"Hello, World!");
    assert_eq!(response.text().unwrap(), "Hello, World!");
  }

  #[test]
  fn bytes_returns_body_slice() {
    let response = make_response(200, b"test data");
    assert_eq!(response.bytes(), b"test data");
  }

  #[test]
  fn into_bytes_consumes_and_returns_body() {
    let response = make_response(200, b"data");
    let bytes = response.into_bytes();
    assert_eq!(bytes, b"data");
  }

  #[test]
  fn body_read_strategy_joins_multiple_te_headers() {
    let mut headers = Headers::new();
    headers.insert("Transfer-Encoding", "gzip");
    headers.insert("Transfer-Encoding", "chunked");
    assert_eq!(
      Response::body_read_strategy(&headers, 200).unwrap(),
      BodyReadStrategy::Chunked
    );

    let mut bad = Headers::new();
    bad.insert("Transfer-Encoding", "chunked");
    bad.insert("Transfer-Encoding", "gzip");
    assert!(matches!(
      Response::body_read_strategy(&bad, 200),
      Err(ParseError::ChunkedNotFinal)
    ));
  }

  #[test]
  fn unsupported_content_encoding_errors() {
    let input = b"HTTP/1.1 200 OK\r\nContent-Encoding: br\r\nContent-Length: 4\r\n\r\ndata";
    assert!(matches!(
      Response::parse(input),
      Err(ParseError::DecompressionFailed)
    ));

    let identity = b"HTTP/1.1 200 OK\r\nContent-Encoding: identity\r\nContent-Length: 4\r\n\r\ndata";
    let ok = Response::parse(identity).unwrap();
    assert_eq!(ok.bytes(), b"data");
  }
}
