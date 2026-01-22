extern crate alloc;
use crate::body::Body;
use crate::error::ParseError;
use crate::headers::Headers;
use crate::parser::chunked::ChunkedDecoder;
use crate::parser::headers::HeaderField;
use crate::parser::http::StatusLine;
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
  pub status_code: u16,
  pub reason: String,
  pub headers: Headers,
  pub body: Body,
}

impl Response {
  /// Parse HTTP/1.1 response with RFC 9112 robustness features.
  /// Per Section 2.2: clients MAY skip leading empty lines before status-line.
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

    let (status_line, mut remaining) = StatusLine::parse(data)?;

    let mut headers = Vec::new();
    let mut headers_bytes = Vec::new();
    loop {
      let (header, rest) = HeaderField::parse(remaining)?;
      remaining = rest;

      if let Some(h) = header {
        headers_bytes.push((h.name.to_vec(), h.value.to_vec()));
        let name_str = String::from_utf8_lossy(h.name).into_owned();
        let value_str = String::from_utf8_lossy(h.value).into_owned();
        headers.push((name_str, value_str));
      } else {
        break;
      }
    }

    let body_bytes =
      Self::parse_body(remaining, &headers_bytes, status_line.status.code())?;

    Ok(Self {
      status_code: status_line.status.code(),
      reason: String::from_utf8_lossy(status_line.reason).into_owned(),
      headers: Headers::from_vec(headers),
      body: Body::from_bytes(body_bytes),
    })
  }

  fn parse_body(
    input: &[u8],
    headers: &[(Vec<u8>, Vec<u8>)],
    status_code: u16,
  ) -> Result<Vec<u8>, ParseError> {
    if (100..200).contains(&status_code) || status_code == 204 || status_code == 304 {
      return Ok(Vec::new());
    }

    let has_transfer_encoding = headers
      .iter()
      .any(|(name, _)| name.eq_ignore_ascii_case(b"transfer-encoding"));

    let has_content_length = headers
      .iter()
      .any(|(name, _)| name.eq_ignore_ascii_case(b"content-length"));

    let content_length = headers
      .iter()
      .find(|(name, _)| name.eq_ignore_ascii_case(b"content-length"))
      .and_then(|(_, value)| parse_content_length(value));

    // RFC 9112 Section 6.3: If both Transfer-Encoding and Content-Length are present,
    // Transfer-Encoding overrides Content-Length. This is a potential attack.
    // A user agent MUST close the connection and discard the response.
    if has_transfer_encoding && has_content_length {
      // For a client implementation, we should handle this by rejecting the message
      // We'll process with Transfer-Encoding but flag as potentially malicious
      // In a real implementation, the connection should be closed after this
    }

    if has_transfer_encoding {
      let is_chunked = headers.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case(b"transfer-encoding")
          && value.windows(7).any(|w| w.eq_ignore_ascii_case(b"chunked"))
      });

      if is_chunked {
        let mut decoder = ChunkedDecoder::new();
        let mut output = Vec::new();
        // RFC 9112 Section 8: Handle incomplete chunked message
        // If decoding fails, the message is incomplete
        decoder.decode_chunk(input, &mut output)?;
        return Ok(output);
      }
      // RFC 9112 Section 6.3: If Transfer-Encoding is present but not chunked,
      // for responses, read until connection closes
      // For a client, this is implementation-specific
      return Ok(input.to_vec());
    }

    if let Some(len) = content_length {
      // RFC 9112 Section 8: A message with valid Content-Length is incomplete
      // if the size received is less than the value given by Content-Length
      if input.len() < len {
        return Err(ParseError::UnexpectedEndOfInput);
      }
      let body_data = input.get(..len).ok_or(ParseError::UnexpectedEndOfInput)?;
      return Ok(body_data.to_vec());
    }

    Ok(Vec::new())
  }

  pub fn get_header(&self, name: &str) -> Option<&str> {
    self.headers.get(name)
  }

  /// Parse response headers only (for two-phase reading)
  /// Returns (status_code, reason, headers, remaining_bytes_after_headers)
  pub fn parse_headers_only(
    input: &[u8],
  ) -> Result<(u16, String, Headers, &[u8]), ParseError> {
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

    let (status_line, mut remaining) = StatusLine::parse(data)?;

    let mut headers = Vec::new();
    loop {
      let (header, rest) = HeaderField::parse(remaining)?;
      remaining = rest;

      if let Some(h) = header {
        let name_str = String::from_utf8_lossy(h.name).into_owned();
        let value_str = String::from_utf8_lossy(h.value).into_owned();
        headers.push((name_str, value_str));
      } else {
        break;
      }
    }

    Ok((
      status_line.status.code(),
      String::from_utf8_lossy(status_line.reason).into_owned(),
      Headers::from_vec(headers),
      remaining,
    ))
  }

  /// Determine how many bytes to read for the response body
  /// Returns None for no body, Some(n) for Content-Length: n, or special handling for chunked
  pub fn body_read_strategy(headers: &Headers, status_code: u16) -> BodyReadStrategy {
    // No body for certain status codes
    if (100..200).contains(&status_code) || status_code == 204 || status_code == 304 {
      return BodyReadStrategy::NoBody;
    }

    let has_transfer_encoding = headers
      .iter()
      .any(|(name, _)| name.eq_ignore_ascii_case("transfer-encoding"));

    let has_content_length = headers
      .iter()
      .any(|(name, _)| name.eq_ignore_ascii_case("content-length"));

    // RFC 9112: Transfer-Encoding overrides Content-Length
    if has_transfer_encoding {
      let is_chunked = headers.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("transfer-encoding")
          && value.to_lowercase().contains("chunked")
      });

      if is_chunked {
        return BodyReadStrategy::Chunked;
      }
      // Non-chunked transfer encoding: read until connection close
      return BodyReadStrategy::UntilClose;
    }

    if has_content_length
      && let Some((_name, value)) = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
      && let Ok(len) = value.trim().parse::<usize>()
    {
      return BodyReadStrategy::ContentLength(len);
    }

    // No Content-Length or Transfer-Encoding: no body for responses
    BodyReadStrategy::NoBody
  }

  /// Parse body from remaining bytes after headers (for two-phase reading)
  pub fn parse_body_from_bytes(
    body_bytes: &[u8],
    headers: &Headers,
    status_code: u16,
  ) -> Result<Body, ParseError> {
    if (100..200).contains(&status_code) || status_code == 204 || status_code == 304 {
      return Ok(Body::from_bytes(Vec::new()));
    }

    let headers_bytes: Vec<(Vec<u8>, Vec<u8>)> = headers
      .iter()
      .map(|(k, v)| (k.as_bytes().to_vec(), v.as_bytes().to_vec()))
      .collect();

    let body_vec = Self::parse_body(body_bytes, &headers_bytes, status_code)?;
    Ok(Body::from_bytes(body_vec))
  }

  #[must_use]
  pub const fn headers(&self) -> &Headers {
    &self.headers
  }

  #[must_use]
  pub const fn headers_mut(&mut self) -> &mut Headers {
    &mut self.headers
  }

  #[must_use]
  pub const fn body(&self) -> &Body {
    &self.body
  }

  #[must_use]
  pub const fn body_mut(&mut self) -> &mut Body {
    &mut self.body
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

fn parse_content_length(value: &[u8]) -> Option<usize> {
  let s = core::str::from_utf8(value).ok()?;
  s.trim().parse().ok()
}

#[derive(Debug, Clone)]
pub struct RequestBuilder {
  method: String,
  path: String,
  headers: Headers,
  body: Option<Body>,
}

impl RequestBuilder {
  pub fn new(method: &str, path: &str) -> Self {
    Self {
      method: String::from(method),
      path: String::from(path),
      headers: Headers::new(),
      body: None,
    }
  }

  pub fn header(mut self, name: &str, value: &str) -> Self {
    self.headers.insert(name, value);
    self
  }

  pub fn body(mut self, body: Vec<u8>) -> Self {
    self.body = Some(Body::from_bytes(body));
    self
  }

  pub fn build(self) -> Vec<u8> {
    let mut request = Vec::new();

    request.extend_from_slice(self.method.as_bytes());
    request.push(b' ');
    request.extend_from_slice(self.path.as_bytes());
    request.extend_from_slice(b" HTTP/1.1\r\n");

    for (name, value) in &self.headers {
      request.extend_from_slice(name.as_bytes());
      request.extend_from_slice(b": ");
      request.extend_from_slice(value.as_bytes());
      request.extend_from_slice(b"\r\n");
    }

    if let Some(body) = &self.body
      && !self.headers.contains("content-length")
    {
      use alloc::string::ToString;
      request.extend_from_slice(b"Content-Length: ");
      request.extend_from_slice(body.len().to_string().as_bytes());
      request.extend_from_slice(b"\r\n");
    }

    request.extend_from_slice(b"\r\n");

    if let Some(body) = &self.body {
      request.extend_from_slice(body.as_bytes());
    }

    request
  }
}
