//! Parsed HTTP/1.1 responses and body-length strategy.

extern crate alloc;
use crate::error::ParseError;
use crate::headers::Headers;
use crate::parser::chunked::ChunkedDecoder;
use crate::parser::headers::parse_header_fields;
use crate::parser::version::{Version, parse_status_line};
use alloc::string::String;
use alloc::vec::Vec;

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
  /// [`ParseError`] when the status line, headers, framing, or body are illegal.
  pub fn parse(input: &[u8]) -> Result<Self, ParseError> {
    let (status_code, reason, mut headers, version, rest) = Self::parse_headers_only(input)?;
    let (body, trailers) = Self::parse_body_from_bytes(rest, &mut headers, status_code, version, usize::MAX)?;
    Ok(Self {
      status_code,
      reason,
      headers,
      body,
      trailers,
    })
  }

  fn decompress_body_if_needed(
    headers: &mut Headers,
    body_bytes: Vec<u8>,
    max_body: usize,
  ) -> Result<Vec<u8>, ParseError> {
    if body_bytes.len() > max_body {
      return Err(ParseError::BodyExceedsLimit(max_body));
    }

    let encodings_empty = !headers
      .iter()
      .any(|(n, _)| n.eq_ignore_ascii_case("content-encoding"));
    if encodings_empty {
      return Ok(body_bytes);
    }

    // RFC 9110: comma-separated codings, applied in listed order → decompress reverse.
    let mut tokens = Vec::new();
    for (name, value) in headers.iter() {
      if !name.eq_ignore_ascii_case("content-encoding") {
        continue;
      }
      for part in value.split(',') {
        let t = part.trim();
        if !t.is_empty() && !t.eq_ignore_ascii_case("identity") {
          tokens.push(String::from(t));
        }
      }
    }

    if tokens.is_empty() {
      return Ok(body_bytes);
    }

    // Unknown / unsupported coding: leave body compressed (do not fail the response).
    if tokens.iter().any(|c| !coding_is_supported(c)) {
      return Ok(body_bytes);
    }

    let mut decoded = body_bytes;
    for coding in tokens.iter().rev() {
      decoded = decompress_coding(coding, decoded, max_body)?;
      if decoded.len() > max_body {
        return Err(ParseError::BodyExceedsLimit(max_body));
      }
    }

    headers.remove("content-encoding");
    headers.remove(Headers::CONTENT_LENGTH);
    Ok(decoded)
  }

  /// Look up a header by name (case-insensitive).
  #[must_use]
  pub fn get_header(
    &self,
    name: &str,
  ) -> Option<&str> {
    self.headers.get(name)
  }

  /// Parse the status line and headers only (two-phase reading).
  ///
  /// Returns `(status_code, reason, headers, version, remainder_after_headers)`.
  ///
  /// # Errors
  /// [`ParseError`] when the status line or header section is illegal.
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

    let mut headers = Headers::new();
    for (name_bytes, value_bytes) in &headers_bytes {
      headers.insert(
        String::from_utf8_lossy(name_bytes).into_owned(),
        String::from_utf8_lossy(value_bytes).into_owned(),
      );
    }

    Ok((
      status,
      String::from_utf8_lossy(reason).into_owned(),
      headers,
      version,
      remaining,
    ))
  }

  /// Choose how to read the entity body from framing headers.
  ///
  /// # Errors
  /// Framing [`ParseError`]s when headers conflict or are illegal for the status/version.
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

      let mut chunked_count = 0usize;
      let mut last_is_chunked = false;
      for (name, value) in headers.iter() {
        if !name.eq_ignore_ascii_case(Headers::TRANSFER_ENCODING) {
          continue;
        }
        for part in value.split(',') {
          let token = part.split(';').next().unwrap_or(part).trim();
          if token.is_empty() {
            continue;
          }
          let is_chunked = token.eq_ignore_ascii_case("chunked");
          if is_chunked {
            chunked_count = chunked_count.saturating_add(1);
          }
          last_is_chunked = is_chunked;
        }
      }

      // RFC 9112 §6.1: MUST NOT apply chunked more than once
      if chunked_count > 1 {
        return Err(ParseError::ChunkedAppliedMultipleTimes);
      }

      if last_is_chunked {
        return Ok(BodyReadStrategy::Chunked);
      }
      if chunked_count > 0 {
        return Err(ParseError::ChunkedNotFinal);
      }
      return Ok(BodyReadStrategy::UntilClose);
    }

    if let Some(len) = resolve_content_length(headers)? {
      return Ok(BodyReadStrategy::ContentLength(len));
    }

    Ok(BodyReadStrategy::UntilClose)
  }

  /// Decode the body from bytes after the header section (two-phase reading).
  ///
  /// Returns the (possibly decompressed) body and any chunked trailer fields.
  /// `max_body` caps wire-decoded and decompressed size ([`ParseError::BodyExceedsLimit`]).
  ///
  /// # Errors
  /// [`ParseError`] when framing is illegal or the body cannot be decoded / decompressed.
  pub fn parse_body_from_bytes(
    body_bytes: &[u8],
    headers: &mut Headers,
    status_code: u16,
    version: Version,
    max_body: usize,
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

    let decompressed_body = Self::decompress_body_if_needed(headers, body_vec, max_body)?;
    Ok((decompressed_body, trailers))
  }

  /// Status code (alias of [`Self::status_code`]).
  #[must_use]
  pub const fn status(&self) -> u16 {
    self.status_code
  }

  /// Status is 2xx.
  #[must_use]
  pub const fn is_success(&self) -> bool {
    matches!(self.status_code, 200..300)
  }

  /// Status is 3xx.
  #[must_use]
  pub const fn is_redirect(&self) -> bool {
    matches!(self.status_code, 300..400)
  }

  /// Status is 4xx.
  #[must_use]
  pub const fn is_client_error(&self) -> bool {
    matches!(self.status_code, 400..500)
  }

  /// Status is 5xx.
  #[must_use]
  pub const fn is_server_error(&self) -> bool {
    matches!(self.status_code, 500..600)
  }

  /// Body bytes.
  #[must_use]
  pub fn as_bytes(&self) -> &[u8] {
    &self.body
  }

  /// Body as UTF-8.
  ///
  /// # Errors
  /// [`crate::Error::Utf8Error`] if the body is not valid UTF-8.
  pub fn text(&self) -> Result<String, crate::Error> {
    String::from_utf8(self.body.clone()).map_err(Into::into)
  }

  /// Consume the response; return the body as UTF-8.
  ///
  /// # Errors
  /// [`crate::Error::Utf8Error`] if the body is not valid UTF-8.
  pub fn into_string(self) -> Result<String, crate::Error> {
    String::from_utf8(self.body).map_err(Into::into)
  }

  /// Consume the response; return body bytes.
  #[must_use]
  pub fn into_bytes(self) -> Vec<u8> {
    self.body
  }
}

/// How the client should read the response entity body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyReadStrategy {
  /// No entity body (1xx, 204, 304, …).
  NoBody,
  /// Exactly `n` octets (`Content-Length`).
  ContentLength(usize),
  /// Chunked transfer coding until the final chunk.
  Chunked,
  /// Read until the peer closes the connection.
  UntilClose,
}

const fn coding_is_supported(coding: &str) -> bool {
  #[cfg(feature = "gzip-decompression")]
  if coding.eq_ignore_ascii_case("gzip") || coding.eq_ignore_ascii_case("deflate") {
    return true;
  }
  #[cfg(feature = "zstd-decompression")]
  if coding.eq_ignore_ascii_case("zstd") {
    return true;
  }
  let _ = coding;
  false
}

fn decompress_coding(
  coding: &str,
  body_bytes: Vec<u8>,
  max_body: usize,
) -> Result<Vec<u8>, ParseError> {
  #[cfg(feature = "gzip-decompression")]
  if coding.eq_ignore_ascii_case("gzip") {
    return match crate::gzip::decompress_gzip(&body_bytes, max_body) {
      Ok(v) => Ok(v),
      Err(crate::gzip::DecompressError::LimitExceeded) => Err(ParseError::BodyExceedsLimit(max_body)),
      Err(crate::gzip::DecompressError::InvalidInput) => Err(ParseError::DecompressionFailed),
    };
  }

  #[cfg(feature = "gzip-decompression")]
  if coding.eq_ignore_ascii_case("deflate") {
    return match crate::gzip::decompress_http_deflate(&body_bytes, max_body) {
      Ok(v) => Ok(v),
      Err(crate::gzip::DecompressError::LimitExceeded) => Err(ParseError::BodyExceedsLimit(max_body)),
      Err(crate::gzip::DecompressError::InvalidInput) => Err(ParseError::DecompressionFailed),
    };
  }

  #[cfg(feature = "zstd-decompression")]
  if coding.eq_ignore_ascii_case("zstd") {
    use ruzstd::io_nostd::Read;
    let mut decoder = StreamingDecoder::new(&body_bytes[..]).map_err(|_| ParseError::DecompressionFailed)?;
    let mut decompressed = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
      let n = decoder
        .read(&mut buf)
        .map_err(|_| ParseError::DecompressionFailed)?;
      if n == 0 {
        break;
      }
      if decompressed.len().saturating_add(n) > max_body {
        return Err(ParseError::BodyExceedsLimit(max_body));
      }
      if let Some(slice) = buf.get(..n) {
        decompressed.extend_from_slice(slice);
      }
    }
    return Ok(decompressed);
  }

  let _ = (coding, body_bytes, max_body);
  Err(ParseError::DecompressionFailed)
}

fn parse_content_length_str(value: &str) -> Option<usize> {
  let trimmed = value.trim();
  if trimmed.is_empty() {
    return None;
  }

  if trimmed.contains(',') {
    let mut first: Option<usize> = None;
    for part in trimmed.split(',') {
      let p = part.trim();
      let n = p.parse::<usize>().ok()?;
      match first {
        None => first = Some(n),
        Some(prev) if prev != n => return None,
        _ => {},
      }
    }
    return first;
  }

  if !trimmed.bytes().all(|b| b.is_ascii_digit()) {
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
    assert_eq!(r.status(), 200);
    assert_eq!(r.as_bytes(), b"hi");
    assert_eq!(r.text().unwrap(), "hi");
    let r2 = Response {
      status_code: 200,
      reason: String::new(),
      headers: Headers::new(),
      body: b"data".to_vec(),
      trailers: Vec::new(),
    };
    assert_eq!(r2.into_string().unwrap(), "data");
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
  fn body_read_strategy_rejects_duplicate_chunked_in_one_line() {
    let mut headers = Headers::new();
    headers.insert("Transfer-Encoding", "chunked, chunked");
    assert!(matches!(
      Response::body_read_strategy(&headers, 200, Version::HTTP_11),
      Err(ParseError::ChunkedAppliedMultipleTimes)
    ));
  }

  #[test]
  fn body_read_strategy_comma_list_with_params() {
    let mut headers = Headers::new();
    headers.insert("Transfer-Encoding", "gzip;q=1.0, chunked");
    assert_eq!(
      Response::body_read_strategy(&headers, 200, Version::HTTP_11).unwrap(),
      BodyReadStrategy::Chunked
    );
  }

  #[test]
  fn parse_headers_only_builds_headers() {
    let input = b"HTTP/1.1 200 OK\r\nHost: a\r\nX-A: 1\r\n\r\nbody";
    let (code, reason, headers, ver, rest) = Response::parse_headers_only(input).unwrap();
    assert_eq!(code, 200);
    assert_eq!(reason, "OK");
    assert_eq!(ver, Version::HTTP_11);
    assert_eq!(headers.get("host"), Some("a"));
    assert_eq!(headers.get("x-a"), Some("1"));
    assert_eq!(rest, b"body");
  }

  #[test]
  fn unsupported_content_encoding_left_as_is() {
    let input = b"HTTP/1.1 200 OK\r\nContent-Encoding: br\r\nContent-Length: 4\r\n\r\ndata";
    let resp = Response::parse(input).unwrap();
    assert_eq!(resp.body.as_slice(), b"data");
    assert_eq!(resp.get_header("content-encoding"), Some("br"));
    assert_eq!(resp.get_header("content-length"), Some("4"));

    let identity = b"HTTP/1.1 200 OK\r\nContent-Encoding: identity\r\nContent-Length: 4\r\n\r\ndata";
    assert_eq!(Response::parse(identity).unwrap().body.as_slice(), b"data");
  }

  #[cfg(feature = "gzip-decompression")]
  #[test]
  fn gzip_decompress_strips_content_encoding_and_length() {
    use alloc::string::ToString;
    // gzip.compress(b"hi")
    let gzipped: &[u8] = &[
      0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0xff, 0xcb, 0xc8, 0x04, 0x00, 0xac, 0x2a, 0x93, 0xd8, 0x02,
      0x00, 0x00, 0x00,
    ];
    let mut msg = Vec::from(&b"HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: "[..]);
    msg.extend_from_slice(gzipped.len().to_string().as_bytes());
    msg.extend_from_slice(b"\r\n\r\n");
    msg.extend_from_slice(gzipped);

    let resp = Response::parse(&msg).unwrap();
    assert_eq!(resp.body.as_slice(), b"hi");
    assert!(resp.get_header("content-encoding").is_none());
    assert!(resp.get_header("content-length").is_none());
  }

  #[cfg(feature = "gzip-decompression")]
  #[test]
  fn gzip_decompress_exceeds_limit() {
    use alloc::string::ToString;
    let gzipped: &[u8] = &[
      0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0xff, 0xcb, 0xc8, 0x04, 0x00, 0xac, 0x2a, 0x93, 0xd8, 0x02,
      0x00, 0x00, 0x00,
    ];
    let mut headers = Headers::new();
    headers.insert("Content-Encoding", "gzip");
    headers.insert("Content-Length", gzipped.len().to_string());
    let err = Response::parse_body_from_bytes(gzipped, &mut headers, 200, Version::HTTP_11, 1).unwrap_err();
    assert_eq!(err, ParseError::BodyExceedsLimit(1));
  }

  #[cfg(feature = "gzip-decompression")]
  #[test]
  fn raw_deflate_accepted_after_zlib_fails() {
    use alloc::string::ToString;
    // raw DEFLATE of b"hi" (RFC 1951; not zlib-wrapped)
    let deflated: &[u8] = &[0xcb, 0xc8, 0x04, 0x00];
    let mut msg = Vec::from(&b"HTTP/1.1 200 OK\r\nContent-Encoding: deflate\r\nContent-Length: "[..]);
    msg.extend_from_slice(deflated.len().to_string().as_bytes());
    msg.extend_from_slice(b"\r\n\r\n");
    msg.extend_from_slice(deflated);

    let resp = Response::parse(&msg).unwrap();
    assert_eq!(resp.body.as_slice(), b"hi");
    assert!(resp.get_header("content-encoding").is_none());
  }
}
