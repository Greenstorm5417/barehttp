//! Parsed HTTP/1.1 responses and body-read strategy.

extern crate alloc;
use crate::error::ParseError;
use crate::headers::Headers;
use crate::parser::chunked::ChunkedDecoder;
use crate::parser::headers::parse_header_fields;
use crate::parser::version::{Version, parse_status_line};
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
  pub body: Vec<u8>,
  /// Trailer fields from chunked responses (RFC 9112 §7.1.2).
  pub trailers: Vec<(String, String)>,
}

impl Response {
  /// Parse a complete buffered HTTP/1.1 response.
  ///
  /// # Errors
  /// Returns [`ParseError`] if the message is malformed.
  pub fn parse(input: &[u8]) -> Result<Self, ParseError> {
    let (status_code, reason, headers, version, rest) = Self::parse_headers_only(input)?;
    let (body, trailers) = Self::parse_body_from_bytes(rest, &headers, status_code, version)?;
    Ok(Self {
      status_code,
      reason,
      headers,
      body,
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
      if data.len() >= 2 && data.first().copied() == Some(b'\r') && data.get(1).copied() == Some(b'\n') {
        data = data.get(2..).unwrap_or(&[]);
        continue;
      }
      if data.first().copied() == Some(b'\n') {
        data = data.get(1..).unwrap_or(&[]);
        continue;
      }
      break;
    }

    let (version, status, reason, after_status) = parse_status_line(data)?;
    let (headers_bytes, remaining) = parse_header_fields(after_status)?;

    let mut headers = Vec::new();
    for (name_bytes, value_bytes) in &headers_bytes {
      headers.push((
        String::from_utf8_lossy(name_bytes).into_owned(),
        String::from_utf8_lossy(value_bytes).into_owned(),
      ));
    }

    Ok((
      status,
      String::from_utf8_lossy(reason).into_owned(),
      Headers::from_vec(headers),
      version,
      remaining,
    ))
  }

  /// Determine how many bytes to read for the response body.
  ///
  /// # Errors
  /// Returns framing [`ParseError`]s when headers are malformed or illegal for the status/version.
  pub fn body_read_strategy(
    headers: &Headers,
    status_code: u16,
    version: Version,
  ) -> Result<BodyReadStrategy, ParseError> {
    let has_transfer_encoding = headers
      .iter()
      .any(|(name, _)| name.eq_ignore_ascii_case(Headers::TRANSFER_ENCODING));

    if has_transfer_encoding && version != Version::HTTP_11 {
      return Err(ParseError::TransferEncodingRequiresHttp11);
    }

    // RFC 9112 §6.1: MUST NOT send TE on 1xx/204; reject 304 TE too (desync risk).
    if has_transfer_encoding && ((100..200).contains(&status_code) || status_code == 204 || status_code == 304) {
      return Err(ParseError::InvalidTransferEncodingForStatus);
    }

    if (100..200).contains(&status_code) || status_code == 204 || status_code == 304 {
      return Ok(BodyReadStrategy::NoBody);
    }

    // RFC 9112: Transfer-Encoding overrides Content-Length — both present is an error
    if has_transfer_encoding {
      if headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case(Headers::CONTENT_LENGTH))
      {
        return Err(ParseError::ConflictingFraming);
      }

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

    if let Some(len) = resolve_content_length(headers)? {
      return Ok(BodyReadStrategy::ContentLength(len));
    }

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
  ) -> Result<(Vec<u8>, Vec<(String, String)>), ParseError> {
    let strategy = Self::body_read_strategy(headers, status_code, version)?;
    let (body_vec, trailer_bytes) = decode_body_bytes(body_bytes, strategy)?;

    let trailers = trailer_bytes
      .into_iter()
      .map(|(name, value)| {
        (
          String::from_utf8_lossy(&name).into_owned(),
          String::from_utf8_lossy(&value).into_owned(),
        )
      })
      .collect();

    let decompressed_body = Self::decompress_body_if_needed(headers, body_vec)?;
    Ok((decompressed_body, trailers))
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

  /// Body as UTF-8 string.
  ///
  /// # Errors
  /// Returns [`crate::Error::Utf8Error`] if the body is not valid UTF-8.
  pub fn text(&self) -> Result<String, crate::Error> {
    String::from_utf8(self.body.clone()).map_err(Into::into)
  }

  /// Consume the response and return body bytes.
  #[must_use]
  pub fn into_bytes(self) -> Vec<u8> {
    self.body
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
    let mut decoder = StreamingDecoder::new(&body_bytes[..]).map_err(|_| ParseError::DecompressionFailed)?;
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
  if flags & 0x04 != 0 {
    let b0 = data.get(i).copied()?;
    let b1 = data.get(i + 1).copied()?;
    let xlen = usize::from(u16::from_le_bytes([b0, b1]));
    i = i.checked_add(2)?.checked_add(xlen)?;
  }
  if flags & 0x08 != 0 {
    while *data.get(i)? != 0 {
      i = i.checked_add(1)?;
    }
    i = i.checked_add(1)?;
  }
  if flags & 0x10 != 0 {
    while *data.get(i)? != 0 {
      i = i.checked_add(1)?;
    }
    i = i.checked_add(1)?;
  }
  if flags & 0x02 != 0 {
    i = i.checked_add(2)?;
  }
  let end = data.len().checked_sub(8)?;
  if end < i {
    return None;
  }
  data.get(i..end)
}

fn parse_content_length_str(value: &str) -> Option<usize> {
  let trimmed = value.trim();
  if trimmed.is_empty() {
    return None;
  }

  if trimmed.contains(',') {
    let parts: Vec<&str> = trimmed.split(',').map(str::trim).collect();
    let first = parts.first()?.parse::<usize>().ok()?;
    if parts.iter().all(|p| p.parse::<usize>().ok() == Some(first)) {
      return Some(first);
    }
    return None;
  }

  if !trimmed.chars().all(|c| c.is_ascii_digit()) {
    return None;
  }

  trimmed.parse().ok()
}

fn resolve_content_length(headers: &Headers) -> Result<Option<usize>, ParseError> {
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

fn decode_body_bytes(
  input: &[u8],
  strategy: BodyReadStrategy,
) -> Result<(Vec<u8>, Vec<(Vec<u8>, Vec<u8>)>), ParseError> {
  match strategy {
    BodyReadStrategy::NoBody => Ok((Vec::new(), Vec::new())),
    BodyReadStrategy::ContentLength(len) => {
      if input.len() < len {
        return Err(ParseError::UnexpectedEndOfInput);
      }
      let body_data = input.get(..len).ok_or(ParseError::UnexpectedEndOfInput)?;
      if input.len() > len {
        return Err(ParseError::ExtraDataAfterResponse);
      }
      Ok((body_data.to_vec(), Vec::new()))
    },
    BodyReadStrategy::Chunked => {
      let mut decoder = ChunkedDecoder::new();
      let mut output = Vec::new();
      let remaining = decoder.decode_chunk(input, &mut output)?;
      if !remaining.is_empty() {
        return Err(ParseError::ExtraDataAfterResponse);
      }
      Ok((output, decoder.trailers().to_vec()))
    },
    BodyReadStrategy::UntilClose => Ok((input.to_vec(), Vec::new())),
  }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod response_helpers_tests {
  use super::*;

  #[test]
  fn status_class_helpers() {
    let mk = |code| Response {
      status_code: code,
      reason: String::new(),
      headers: Headers::new(),
      body: Vec::new(),
      trailers: Vec::new(),
    };
    assert!(mk(200).is_success());
    assert!(mk(301).is_redirect());
    assert!(mk(404).is_client_error());
    assert!(mk(500).is_server_error());
    assert!(!mk(199).is_success());
  }

  #[test]
  fn text_and_into_bytes() {
    let r = Response {
      status_code: 200,
      reason: String::new(),
      headers: Headers::new(),
      body: b"hi".to_vec(),
      trailers: Vec::new(),
    };
    assert_eq!(r.text().unwrap(), "hi");
    let r2 = Response {
      status_code: 200,
      reason: String::new(),
      headers: Headers::new(),
      body: b"data".to_vec(),
      trailers: Vec::new(),
    };
    assert_eq!(r2.into_bytes(), b"data");
  }

  #[test]
  fn body_read_strategy_joins_multiple_te_headers() {
    let mut headers = Headers::new();
    headers.insert("Transfer-Encoding", "gzip");
    headers.insert("Transfer-Encoding", "chunked");
    assert_eq!(
      Response::body_read_strategy(&headers, 200, Version::HTTP_11).unwrap(),
      BodyReadStrategy::Chunked
    );

    let mut bad = Headers::new();
    bad.insert("Transfer-Encoding", "chunked");
    bad.insert("Transfer-Encoding", "gzip");
    assert!(matches!(
      Response::body_read_strategy(&bad, 200, Version::HTTP_11),
      Err(ParseError::ChunkedNotFinal)
    ));
  }

  #[test]
  fn unsupported_content_encoding_errors() {
    let input = b"HTTP/1.1 200 OK\r\nContent-Encoding: br\r\nContent-Length: 4\r\n\r\ndata";
    assert!(matches!(Response::parse(input), Err(ParseError::DecompressionFailed)));

    let identity = b"HTTP/1.1 200 OK\r\nContent-Encoding: identity\r\nContent-Length: 4\r\n\r\ndata";
    assert_eq!(Response::parse(identity).unwrap().body.as_slice(), b"data");
  }
}
