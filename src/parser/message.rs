extern crate alloc;
use crate::body::Body;
use crate::error::ParseError;
use crate::headers::{HeaderName, Headers};
use crate::parser::chunked::ChunkedDecoder;
use crate::parser::headers::HeaderField;
use crate::parser::http::StatusLine;
use crate::parser::version::Version;
use alloc::string::String;
use alloc::vec::Vec;

#[cfg(feature = "gzip-decompression")]
use miniz_oxide::inflate::{decompress_to_vec, decompress_to_vec_zlib};

#[cfg(feature = "zstd-decompression")]
use ruzstd::decoding::StreamingDecoder;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
  pub status_code: u16,
  pub reason: String,
  pub headers: Headers,
  pub body: Body,
  /// Trailer fields from chunked responses (RFC 9112 Section 7.1.2)
  /// Stored separately as they appear after the body in chunked encoding
  pub trailers: Vec<(String, String)>,
}

impl Response {
  /// Parse HTTP/1.1 response with RFC 9112 robustness features.
  /// Per Section 2.2: clients MAY skip leading empty lines before status-line.
  /// Per Section 5.2: clients MUST handle obsolete line folding (obs-fold).
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
      status_line.status.code(),
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
      status_code: status_line.status.code(),
      reason: String::from_utf8_lossy(status_line.reason).into_owned(),
      headers: Headers::from_vec(headers),
      body: Body::from_bytes(body),
      trailers,
    })
  }

  #[allow(clippy::unnecessary_wraps, unused_variables)]
  fn decompress_body_if_needed(
    headers: &Headers,
    body_bytes: Vec<u8>,
  ) -> Result<Vec<u8>, ParseError> {
    if let Some(encoding) = headers.get("content-encoding") {
      let encoding_lower = encoding.to_lowercase();

      // Try gzip/deflate decompression
      #[cfg(feature = "gzip-decompression")]
      if encoding_lower.contains("gzip") {
        // Gzip format: strip 10-byte header and 8-byte footer, decompress the middle
        if body_bytes.len() < 18 {
          return Err(ParseError::DecompressionFailed);
        }
        // Skip gzip header (10 bytes minimum) and footer (8 bytes)
        // The actual compressed data is in between
        let end_pos = body_bytes.len().saturating_sub(8);
        let deflate_data = body_bytes
          .get(10..end_pos)
          .ok_or(ParseError::DecompressionFailed)?;
        return decompress_to_vec(deflate_data).map_err(|_| ParseError::DecompressionFailed);
      }

      #[cfg(feature = "gzip-decompression")]
      if encoding_lower.contains("deflate") {
        return decompress_to_vec_zlib(&body_bytes).map_err(|_| ParseError::DecompressionFailed);
      }

      // Try zstd decompression
      #[cfg(feature = "zstd-decompression")]
      if encoding_lower.contains("zstd") {
        use ruzstd::io_nostd::Read;
        let mut decoder = StreamingDecoder::new(&body_bytes[..]).map_err(|_| ParseError::DecompressionFailed)?;
        let mut decompressed = Vec::new();
        decoder
          .read_to_end(&mut decompressed)
          .map_err(|_| ParseError::DecompressionFailed)?;
        return Ok(decompressed);
      }
    }
    Ok(body_bytes)
  }

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
      .any(|(name, _)| name.eq_ignore_ascii_case(HeaderName::TRANSFER_ENCODING.as_bytes()));

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

    let has_content_length = headers
      .iter()
      .any(|(name, _)| name.eq_ignore_ascii_case(HeaderName::CONTENT_LENGTH.as_bytes()));

    let content_length = headers
      .iter()
      .find(|(name, _)| name.eq_ignore_ascii_case(HeaderName::CONTENT_LENGTH.as_bytes()))
      .and_then(|(_, value)| parse_content_length(value));

    // RFC 9112 Section 6.3: If both Transfer-Encoding and Content-Length are present,
    // this is a potential request smuggling attack. Client MUST close connection
    // and discard the response.
    if has_transfer_encoding && has_content_length {
      return Err(ParseError::ConflictingFraming);
    }

    if has_transfer_encoding {
      // RFC 9112 Section 6.3: chunked MUST be the final transfer coding
      let te_value = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(HeaderName::TRANSFER_ENCODING.as_bytes()))
        .map(|(_, value)| value);

      if let Some(te_bytes) = te_value {
        let te_str = core::str::from_utf8(te_bytes)
          .unwrap_or("")
          .trim()
          .to_lowercase();

        // Check if chunked is the final encoding
        let is_chunked_final = te_str.ends_with("chunked") || te_str == "chunked";

        if !is_chunked_final && te_str.contains("chunked") {
          // chunked exists but is not final - this is a smuggling vector
          return Err(ParseError::ChunkedNotFinal);
        }

        if is_chunked_final {
          let mut decoder = ChunkedDecoder::new();
          let mut output = Vec::new();
          // RFC 9112 Section 8: Handle incomplete chunked message
          // If decoding fails, the message is incomplete
          let remaining = decoder.decode_chunk(input, &mut output)?;

          // RFC 9112 Section 6.3: Client MUST NOT process/cache/forward extra data
          // as a separate response
          if !remaining.is_empty() {
            return Err(ParseError::ExtraDataAfterResponse);
          }

          // RFC 9112 Section 7.1.2: Retrieve trailer fields from chunked response
          // Store them separately (merging only allowed if header definition permits)
          let trailer_fields = decoder.trailers();

          return Ok((output, trailer_fields.to_vec()));
        }
      }
      // RFC 9112 Section 6.3: If Transfer-Encoding is present but not chunked,
      // for responses, read until connection closes
      // For a client, this is implementation-specific
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

    Ok((Vec::new(), Vec::new()))
  }

  pub fn get_header(
    &self,
    name: &str,
  ) -> Option<&str> {
    self.headers.get(name)
  }

  /// Parse response headers only (for two-phase reading)
  /// Returns (`status_code`, reason, headers, `remaining_bytes_after_headers`)
  pub fn parse_headers_only(input: &[u8]) -> Result<(u16, String, Headers, &[u8]), ParseError> {
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
      status_line.status.code(),
      String::from_utf8_lossy(status_line.reason).into_owned(),
      Headers::from_vec(headers),
      remaining,
    ))
  }

  /// Determine how many bytes to read for the response body
  /// Returns None for no body, Some(n) for Content-Length: n, or special handling for chunked
  pub fn body_read_strategy(
    headers: &Headers,
    status_code: u16,
  ) -> BodyReadStrategy {
    // No body for certain status codes
    if (100..200).contains(&status_code) || status_code == 204 || status_code == 304 {
      return BodyReadStrategy::NoBody;
    }

    let has_transfer_encoding = headers
      .iter()
      .any(|(name, _)| name.eq_ignore_ascii_case(HeaderName::TRANSFER_ENCODING));

    let has_content_length = headers
      .iter()
      .any(|(name, _)| name.eq_ignore_ascii_case(HeaderName::CONTENT_LENGTH));

    // RFC 9112: Transfer-Encoding overrides Content-Length
    if has_transfer_encoding {
      let is_chunked = headers.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case(HeaderName::TRANSFER_ENCODING) && value.to_lowercase().contains("chunked")
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
        .find(|(name, _)| name.eq_ignore_ascii_case(HeaderName::CONTENT_LENGTH))
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

    let (body_vec, _trailers) = Self::parse_body_internal(body_bytes, &headers_bytes, None, status_code, None)?;

    // Decompress if needed
    let decompressed_body = Self::decompress_body_if_needed(headers, body_vec)?;
    Ok(Body::from_bytes(decompressed_body))
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

  /// Check if the server sent Connection: close
  ///
  /// Per RFC 9112 Section 9.6: If server sends "close", client MUST:
  /// - Stop sending further requests on this connection
  /// - Close the connection after reading the response body
  #[must_use]
  pub fn has_connection_close(&self) -> bool {
    self
      .headers
      .get(HeaderName::CONNECTION)
      .is_some_and(|val| val.eq_ignore_ascii_case("close"))
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
  let trimmed = s.trim();

  // RFC 9112 Section 6.3: Check for multiple values (comma-separated)
  if trimmed.contains(',') {
    // RFC 9112 allows comma-separated identical values
    let parts: Vec<&str> = trimmed.split(',').map(str::trim).collect();
    if parts.is_empty() {
      return None;
    }
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

#[derive(Debug, Clone)]
pub struct RequestBuilder {
  method: String,
  path: String,
  headers: Headers,
  body: Option<Body>,
}

impl RequestBuilder {
  pub fn new(
    method: &str,
    path: &str,
  ) -> Self {
    Self {
      method: String::from(method),
      path: String::from(path),
      headers: Headers::new(),
      body: None,
    }
  }

  pub fn header(
    mut self,
    name: &str,
    value: &str,
  ) -> Self {
    self.headers.insert(name, value);
    self
  }

  pub fn body(
    mut self,
    body: Vec<u8>,
  ) -> Self {
    self.body = Some(Body::from_bytes(body));
    self
  }

  pub fn build(self) -> Result<Vec<u8>, ParseError> {
    // RFC 9112 Section 3.2: Client MUST send Host in every HTTP/1.1 request
    if !self.headers.contains(HeaderName::HOST) {
      return Err(ParseError::MissingHostHeader);
    }

    // RFC 9112 Section 3.2: Server responds 400 if multiple Host headers present
    let host_headers = self.headers.get_all(HeaderName::HOST);
    if host_headers.len() > 1 {
      return Err(ParseError::MultipleHostHeaders);
    }

    // RFC 9112 Section 3.2: Validate Host header value format
    if let Some(host_value) = self.headers.get(HeaderName::HOST)
      && !Self::is_valid_host_value(host_value)
    {
      return Err(ParseError::InvalidHostHeaderValue);
    }

    // Validate all header values for RFC 9112 compliance
    for (name, value) in &self.headers {
      // RFC 9112 Section 2.2: Sender MUST NOT generate bare CR
      if value.contains('\r') && !value.contains("\r\n") {
        return Err(ParseError::BareCarriageReturnInHeader);
      }

      // RFC 9112 Section 5.2: Sender MUST NOT generate obs-fold
      if value.contains("\r\n ") || value.contains("\r\n\t") {
        return Err(ParseError::ObsoleteFoldInHeader);
      }

      // RFC 9112 Section 7.4: Client MUST NOT send "chunked" in TE
      if name.eq_ignore_ascii_case(HeaderName::TE) && value.to_lowercase().contains("chunked") {
        return Err(ParseError::ChunkedInTeHeader);
      }

      // RFC 9112 Section 7.4: Sender of TE MUST also send "TE" in Connection
      if name.eq_ignore_ascii_case(HeaderName::TE) {
        if let Some(conn_value) = self.headers.get(HeaderName::CONNECTION) {
          if !conn_value.to_lowercase().contains("te") {
            return Err(ParseError::TeHeaderMissingConnection);
          }
        } else {
          return Err(ParseError::TeHeaderMissingConnection);
        }
      }

      // RFC 9112 Section 6.1: Client MUST NOT send Transfer-Encoding unless server supports HTTP/1.1+
      // Since we always use HTTP/1.1, this is implicitly satisfied, but validate the header
      if name.eq_ignore_ascii_case(HeaderName::TRANSFER_ENCODING) {
        // RFC 9112 Section 6.1: MUST NOT apply chunked more than once
        let te_lower = value.to_lowercase();
        let chunked_count = te_lower.matches("chunked").count();
        if chunked_count > 1 {
          return Err(ParseError::ChunkedAppliedMultipleTimes);
        }
      }
    }

    // RFC 9112 Section 6.2: Sender MUST NOT send CL when TE present
    let has_te = self.headers.contains(HeaderName::TRANSFER_ENCODING);
    let has_cl = self.headers.contains(HeaderName::CONTENT_LENGTH);
    if has_te && has_cl {
      return Err(ParseError::ConflictingFraming);
    }

    let mut request = Vec::new();

    request.extend_from_slice(self.method.as_bytes());
    request.push(b' ');

    // RFC 9112 Section 3.2.1: If origin-form path is empty, send "/"
    let path = if self.path.is_empty() {
      "/"
    } else {
      &self.path
    };
    request.extend_from_slice(path.as_bytes());
    request.extend_from_slice(b" HTTP/1.1\r\n");

    for (name, value) in &self.headers {
      request.extend_from_slice(name.as_bytes());
      request.extend_from_slice(b": ");
      request.extend_from_slice(value.as_bytes());
      request.extend_from_slice(b"\r\n");
    }

    if let Some(body) = &self.body
      && !self.headers.contains(HeaderName::CONTENT_LENGTH)
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

    Ok(request)
  }

  /// Validate Host header value format per RFC 9112 Section 3.2
  /// Host = uri-host [ ":" port ]
  /// uri-host = <host from URI syntax>
  fn is_valid_host_value(host: &str) -> bool {
    if host.is_empty() {
      // Empty Host is valid per RFC 9112 Section 3.2
      return true;
    }

    // Check for invalid characters
    if host.contains(char::is_whitespace) {
      return false;
    }

    // Handle IPv6 literals specially (they contain colons)
    if host.starts_with('[') {
      // IPv6 literal format: [ipv6]:port or [ipv6]
      if let Some(bracket_end) = host.find(']') {
        let ipv6_part = &host[..=bracket_end];
        let after_bracket = &host[bracket_end + 1..];

        if after_bracket.is_empty() {
          // Just [ipv6]
          return Self::is_valid_hostname(ipv6_part);
        } else if let Some(port_str) = after_bracket.strip_prefix(':') {
          // [ipv6]:port
          if port_str.is_empty() || !port_str.chars().all(|c| c.is_ascii_digit()) {
            return false;
          }
          if let Ok(port) = port_str.parse::<u16>() {
            if port == 0 {
              return false;
            }
          } else {
            return false;
          }
          return Self::is_valid_hostname(ipv6_part);
        }
        return false;
      }
      return false;
    }

    // Split host and port if present (for non-IPv6)
    let parts: Vec<&str> = host.rsplitn(2, ':').collect();

    if parts.len() == 2 {
      // Has port - validate it
      let Some(port_str) = parts.first() else {
        return false;
      };
      if port_str.is_empty() || !port_str.chars().all(|c| c.is_ascii_digit()) {
        return false;
      }
      // Check port range
      if let Ok(port) = port_str.parse::<u16>() {
        if port == 0 {
          return false;
        }
      } else {
        return false;
      }

      // Validate hostname part
      let Some(hostname) = parts.get(1) else {
        return false;
      };
      Self::is_valid_hostname(hostname)
    } else {
      // No port, just validate hostname
      Self::is_valid_hostname(host)
    }
  }

  /// Validate hostname format (simplified check for common cases)
  fn is_valid_hostname(hostname: &str) -> bool {
    if hostname.is_empty() {
      return false;
    }

    // Check for IPv6 literal
    if hostname.starts_with('[') && hostname.ends_with(']') {
      // Basic IPv6 validation - just check it has hex digits and colons
      let inner = &hostname[1..hostname.len() - 1];
      return !inner.is_empty() && inner.chars().all(|c| c.is_ascii_hexdigit() || c == ':');
    }

    // Regular hostname or IPv4
    // Allow alphanumeric, dots, hyphens
    hostname
      .chars()
      .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
  }
}
