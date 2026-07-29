//! Parsed HTTP/1.1 responses and body-length strategy.

extern crate alloc;
use crate::error::{DecompressError, IntoStringError, ParseError};
use crate::headers::Headers;
use crate::parser::chunked::ChunkedDecoder;
use crate::parser::headers::{HeaderRef, materialize_headers, parse_header_fields, scan_header_fields};
use crate::parser::version::{Version, parse_status_line};
use alloc::string::String;
use alloc::vec::Vec;
use bytes::Bytes;

#[cfg(feature = "zstd")]
use ruzstd::decoding::StreamingDecoder;

/// Parsed HTTP response.
///
/// # Examples
///
/// ```
/// use barehttp::Response;
///
/// let raw = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok";
/// let response = Response::parse(raw).map_err(barehttp::Error::from)?;
/// assert_eq!(response.status_code(), 200);
/// assert_eq!(response.header("content-length"), Some("2"));
/// assert_eq!(response.to_text()?, "ok");
/// # Ok::<(), barehttp::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Response {
  status_code: u16,
  reason: String,
  headers: Headers,
  body: Bytes,
  trailers: Headers,
}

impl Response {
  /// Build a response from parts (crate-internal / tests).
  #[must_use]
  pub(crate) fn from_parts(
    status_code: u16,
    reason: String,
    headers: Headers,
    body: impl Into<Bytes>,
    trailers: Headers,
  ) -> Self {
    Self {
      status_code,
      reason,
      headers,
      body: body.into(),
      trailers,
    }
  }

  /// Status code (e.g. 200).
  ///
  /// Primary name; [`Self::status`] is a deprecated compatibility alias.
  #[must_use]
  pub const fn status_code(&self) -> u16 {
    self.status_code
  }

  /// Reason phrase from the status line.
  #[must_use]
  pub fn reason(&self) -> &str {
    &self.reason
  }

  /// Response headers.
  #[must_use]
  pub const fn headers(&self) -> &Headers {
    &self.headers
  }

  /// Response body bytes.
  ///
  /// Primary name; [`Self::as_bytes`] is a deprecated compatibility alias.
  #[must_use]
  pub fn body(&self) -> &[u8] {
    &self.body
  }

  /// Trailer fields from chunked responses (RFC 9112 §7.1.2).
  ///
  /// Same type as [`Self::headers`] — case-insensitive lookup, ordered iteration.
  #[must_use]
  pub const fn trailers(&self) -> &Headers {
    &self.trailers
  }

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
    body_bytes: Bytes,
    max_body: usize,
  ) -> Result<Bytes, ParseError> {
    if body_bytes.len() > max_body {
      return Err(ParseError::BodyExceedsLimit(max_body));
    }

    // RFC 9110: comma-separated codings, applied in listed order → decompress reverse.
    // Typical responses have a single coding — avoid heap for that case.
    // One pass: detect Content-Encoding and collect codings (no empty pre-scan).
    let mut single: Option<&str> = None;
    let mut multi: Vec<&str> = Vec::new();
    let mut saw_ce = false;
    for (name, value) in &*headers {
      if !name.eq_ignore_ascii_case(Headers::CONTENT_ENCODING) {
        continue;
      }
      saw_ce = true;
      for part in value.split(',') {
        let t = part.trim();
        if t.is_empty() || t.eq_ignore_ascii_case("identity") {
          continue;
        }
        if multi.is_empty() {
          if let Some(first) = single {
            multi.push(first);
            multi.push(t);
            single = None;
          } else {
            single = Some(t);
          }
        } else {
          multi.push(t);
        }
      }
    }
    if !saw_ce {
      return Ok(body_bytes);
    }

    let unsupported = if let Some(c) = single {
      !coding_is_supported(c)
    } else if multi.is_empty() {
      return Ok(body_bytes);
    } else {
      multi.iter().any(|c| !coding_is_supported(c))
    };
    // Unknown / unsupported coding: leave body compressed (do not fail the response).
    if unsupported {
      return Ok(body_bytes);
    }

    let mut decoded: Vec<u8> = body_bytes.into();
    if let Some(coding) = single {
      decoded = decompress_coding(coding, decoded, max_body)?;
      if decoded.len() > max_body {
        return Err(ParseError::BodyExceedsLimit(max_body));
      }
    } else {
      for coding in multi.iter().rev() {
        decoded = decompress_coding(coding, decoded, max_body)?;
        if decoded.len() > max_body {
          return Err(ParseError::BodyExceedsLimit(max_body));
        }
      }
    }

    headers.remove(Headers::CONTENT_ENCODING);
    headers.remove(Headers::CONTENT_LENGTH);
    Ok(Bytes::from(decoded))
  }

  /// Look up a header by name (case-insensitive).
  #[must_use]
  pub fn header(
    &self,
    name: &str,
  ) -> Option<&str> {
    self.headers.get(name)
  }

  /// Promote borrowed header refs to owned [`Headers`].
  #[must_use]
  pub(crate) fn headers_from_refs(refs: &[HeaderRef<'_>]) -> Headers {
    materialize_headers(refs)
  }

  /// Own a reason-phrase byte slice.
  #[must_use]
  pub(crate) fn reason_owned(reason_bytes: &[u8]) -> String {
    reason_to_string(reason_bytes)
  }

  /// Parse the status line and headers only (two-phase reading).
  ///
  /// Returns `(status_code, reason, headers, version, remainder_after_headers)`.
  ///
  /// # Errors
  /// [`ParseError`] when the status line or header section is illegal.
  pub(crate) fn parse_headers_only(input: &[u8]) -> Result<(u16, String, Headers, Version, &[u8]), ParseError> {
    // One-pass owned headers (no intermediate `HeaderRef` vec).
    let data = skip_leading_crlf(input);
    let (version, status, reason_bytes, after_status) = parse_status_line(data)?;
    let (headers, remaining) = parse_header_fields(after_status)?;
    Ok((status, reason_to_string(reason_bytes), headers, version, remaining))
  }

  /// Zero-copy status line + header scan (no name/value `String`s yet).
  ///
  /// # Errors
  /// [`ParseError`] when the status line or header section is illegal.
  pub(crate) fn scan_headers_only(
    input: &[u8]
  ) -> Result<(u16, &[u8], Vec<HeaderRef<'_>>, Version, &[u8]), ParseError> {
    let data = skip_leading_crlf(input);
    let (version, status, reason_bytes, after_status) = parse_status_line(data)?;
    let (headers, remaining) = scan_header_fields(after_status)?;
    Ok((status, reason_bytes, headers, version, remaining))
  }

  /// Choose how to read the entity body from framing headers.
  ///
  /// # Errors
  /// Framing [`ParseError`]s when headers conflict or are illegal for the status/version.
  pub(crate) fn body_read_strategy(
    headers: &Headers,
    status_code: u16,
    version: Version,
  ) -> Result<BodyReadStrategy, ParseError> {
    Self::body_read_strategy_pairs(
      headers.iter().map(|(n, v)| (n.as_bytes(), v.as_bytes())),
      status_code,
      version,
    )
  }

  /// Framing strategy from borrowed header refs (before materializing [`Headers`]).
  ///
  /// # Errors
  /// Framing [`ParseError`]s when headers conflict or are illegal for the status/version.
  pub(crate) fn body_read_strategy_refs(
    headers: &[HeaderRef<'_>],
    status_code: u16,
    version: Version,
  ) -> Result<BodyReadStrategy, ParseError> {
    Self::body_read_strategy_pairs(headers.iter().map(|h| (h.name, h.value)), status_code, version)
  }

  fn body_read_strategy_pairs<'a, I>(
    headers: I,
    status_code: u16,
    version: Version,
  ) -> Result<BodyReadStrategy, ParseError>
  where
    I: IntoIterator<Item = (&'a [u8], &'a [u8])>,
  {
    if (100..200).contains(&status_code) || status_code == 204 || status_code == 304 {
      // Still reject illegal TE on these statuses (desync risk).
      for (name, _) in headers {
        if name.eq_ignore_ascii_case(b"transfer-encoding") {
          return Err(ParseError::InvalidTransferEncodingForStatus);
        }
      }
      return Ok(BodyReadStrategy::NoBody);
    }

    let mut has_transfer_encoding = false;
    let mut chunked_count = 0usize;
    let mut last_is_chunked = false;
    let mut resolved_cl: Option<usize> = None;
    let mut seen_cl = false;

    // Framing only needs TE / Content-Length; byte-compare each field (no PHF).
    for (name, value) in headers {
      if name.eq_ignore_ascii_case(b"transfer-encoding") {
        has_transfer_encoding = true;
        for part in value.split(|&b| b == b',') {
          let token = te_coding_token(part);
          if token.is_empty() {
            continue;
          }
          let is_chunked = token.eq_ignore_ascii_case(b"chunked");
          if is_chunked {
            chunked_count = chunked_count.saturating_add(1);
          }
          last_is_chunked = is_chunked;
        }
      } else if name.eq_ignore_ascii_case(b"content-length") {
        seen_cl = true;
        let len = parse_content_length_bytes(value).ok_or(ParseError::InvalidContentLength)?;
        match resolved_cl {
          None => resolved_cl = Some(len),
          Some(prev) if prev != len => return Err(ParseError::InvalidContentLength),
          _ => {},
        }
      }
    }

    if has_transfer_encoding && version != Version::HTTP_11 {
      return Err(ParseError::TransferEncodingRequiresHttp11);
    }

    if has_transfer_encoding {
      if seen_cl {
        return Err(ParseError::ConflictingFraming);
      }
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

    if seen_cl {
      return Ok(BodyReadStrategy::ContentLength(
        resolved_cl.ok_or(ParseError::InvalidContentLength)?,
      ));
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
  pub(crate) fn parse_body_from_bytes(
    body_bytes: &[u8],
    headers: &mut Headers,
    status_code: u16,
    version: Version,
    max_body: usize,
  ) -> Result<(Bytes, Headers), ParseError> {
    let strategy = Self::body_read_strategy(headers, status_code, version)?;
    let (body_vec, trailer_bytes) = decode_body_bytes(body_bytes, strategy)?;
    finish_body(headers, body_vec, trailer_bytes, max_body)
  }

  /// Owned-buffer variant of [`Self::parse_body_from_bytes`].
  /// Skips a copy for `Content-Length` / until-close bodies the transport already holds.
  ///
  /// # Errors
  /// [`ParseError`] when framing is illegal or the body cannot be decoded / decompressed.
  pub(crate) fn parse_body_from_owned(
    body_bytes: Bytes,
    headers: &mut Headers,
    status_code: u16,
    version: Version,
    max_body: usize,
  ) -> Result<(Bytes, Headers), ParseError> {
    let strategy = Self::body_read_strategy(headers, status_code, version)?;
    let (body_vec, trailer_bytes) = decode_body_owned(body_bytes, strategy)?;
    finish_body(headers, body_vec, trailer_bytes, max_body)
  }

  /// Deprecated alias of [`Self::status_code`].
  #[deprecated(note = "use `status_code`")]
  #[must_use]
  #[inline]
  pub const fn status(&self) -> u16 {
    self.status_code()
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

  /// Deprecated alias of [`Self::body`].
  #[deprecated(note = "use `body`")]
  #[must_use]
  pub fn as_bytes(&self) -> &[u8] {
    self.body()
  }

  /// Body as UTF-8 text (borrowed).
  ///
  /// Prefer this over [`Self::into_string`] when you still need status/headers after
  /// a UTF-8 check — failure does not consume `self`.
  ///
  /// Returns [`core::str::Utf8Error`] (not [`crate::Error`]); that type converts with
  /// [`From`] / `?` into [`crate::Error::Utf8Error`] when desired.
  ///
  /// # Errors
  /// [`core::str::Utf8Error`] if the body is not valid UTF-8.
  pub fn to_text(&self) -> Result<&str, core::str::Utf8Error> {
    core::str::from_utf8(self.body())
  }

  /// Consume the response; return the body as a UTF-8 [`String`].
  ///
  /// On failure, [`IntoStringError`] preserves the full [`Response`] (status, headers,
  /// body) so callers can recover without losing the message. That type also
  /// implements [`From`] into [`crate::Error`] (UTF-8 cause only — recover first if needed).
  ///
  /// On success, converts the body to a [`String`] without copying when the
  /// underlying buffer is uniquely owned.
  ///
  /// # Errors
  /// [`IntoStringError`] if the body is not valid UTF-8.
  ///
  /// # Examples
  ///
  /// ```
  /// use barehttp::Response;
  ///
  /// let bad = Response::parse(b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\n\r\n\xff")
  ///   .map_err(barehttp::Error::from)?;
  /// if let Err(err) = bad.into_string() {
  ///   assert_eq!(err.response().status_code(), 200);
  ///   assert_eq!(err.into_response().body(), &[0xff]);
  /// }
  /// # Ok::<(), barehttp::Error>(())
  /// ```
  pub fn into_string(self) -> Result<String, IntoStringError> {
    if let Err(error) = core::str::from_utf8(self.body()) {
      return Err(IntoStringError::new(self, error));
    }
    let Self { body, .. } = self;
    // UTF-8 validated above; reclaim unique ownership without a copy when possible.
    let vec = Vec::from(body);
    // SAFETY: `from_utf8` succeeded on these exact bytes before the move.
    Ok(unsafe { String::from_utf8_unchecked(vec) })
  }

  /// Consume the response; return body bytes as a [`Vec<u8>`].
  ///
  /// Prefer [`Self::body`] when a borrowed slice is enough. Reclaims the
  /// allocation without a copy when the internal buffer is uniquely owned.
  #[must_use]
  pub fn into_bytes(self) -> Vec<u8> {
    Vec::from(self.body)
  }
}

impl AsRef<[u8]> for Response {
  fn as_ref(&self) -> &[u8] {
    self.body()
  }
}

/// How the client should read the response entity body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
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
  #[cfg(feature = "gzip")]
  if coding.eq_ignore_ascii_case("gzip") || coding.eq_ignore_ascii_case("deflate") {
    return true;
  }
  #[cfg(feature = "zstd")]
  if coding.eq_ignore_ascii_case("zstd") {
    return true;
  }
  let _ = coding;
  false
}

#[cfg(feature = "gzip")]
const fn map_decompress_error(
  err: DecompressError,
  max_body: usize,
) -> ParseError {
  match err {
    DecompressError::LimitExceeded => ParseError::BodyExceedsLimit(max_body),
    DecompressError::InvalidInput => ParseError::Decompression(DecompressError::InvalidInput),
  }
}

fn decompress_coding(
  coding: &str,
  body_bytes: Vec<u8>,
  max_body: usize,
) -> Result<Vec<u8>, ParseError> {
  #[cfg(feature = "gzip")]
  if coding.eq_ignore_ascii_case("gzip") {
    return crate::gzip::decompress_gzip(&body_bytes, max_body).map_err(|e| map_decompress_error(e, max_body));
  }

  #[cfg(feature = "gzip")]
  if coding.eq_ignore_ascii_case("deflate") {
    return crate::gzip::decompress_http_deflate(&body_bytes, max_body).map_err(|e| map_decompress_error(e, max_body));
  }

  #[cfg(feature = "zstd")]
  if coding.eq_ignore_ascii_case("zstd") {
    use ruzstd::io_nostd::Read;
    let mut decoder =
      StreamingDecoder::new(&body_bytes[..]).map_err(|_| ParseError::Decompression(DecompressError::InvalidInput))?;
    let mut decompressed = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
      let n = decoder
        .read(&mut buf)
        .map_err(|_| ParseError::Decompression(DecompressError::InvalidInput))?;
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
  Err(ParseError::Decompression(DecompressError::InvalidInput))
}

/// Transfer-coding token: OWS-trimmed, parameter suffix (`;…`) stripped.
fn te_coding_token(part: &[u8]) -> &[u8] {
  let mut t = trim_ows(part);
  if let Some(semi) = t.iter().position(|&b| b == b';') {
    t = trim_ows(t.get(..semi).unwrap_or(&[]));
  }
  t
}

/// Parse a `Content-Length` field value from raw bytes (OWS / duplicate-list rules).
fn parse_content_length_bytes(value: &[u8]) -> Option<usize> {
  let trimmed = trim_ows(value);
  if trimmed.is_empty() {
    return None;
  }

  if trimmed.contains(&b',') {
    let mut first: Option<usize> = None;
    for part in trimmed.split(|&b| b == b',') {
      let n = parse_decimal_usize(trim_ows(part))?;
      match first {
        None => first = Some(n),
        Some(prev) if prev != n => return None,
        _ => {},
      }
    }
    return first;
  }

  parse_decimal_usize(trimmed)
}

fn trim_ows(value: &[u8]) -> &[u8] {
  let mut s = value;
  while matches!(s.first().copied(), Some(b' ' | b'\t')) {
    s = s.get(1..).unwrap_or(&[]);
  }
  while matches!(s.last().copied(), Some(b' ' | b'\t')) {
    s = s.get(..s.len().saturating_sub(1)).unwrap_or(&[]);
  }
  s
}

fn parse_decimal_usize(digits: &[u8]) -> Option<usize> {
  if digits.is_empty() {
    return None;
  }
  let mut n = 0usize;
  for &b in digits {
    if !b.is_ascii_digit() {
      return None;
    }
    n = n.checked_mul(10)?.checked_add(usize::from(b - b'0'))?;
  }
  Some(n)
}

fn skip_leading_crlf(input: &[u8]) -> &[u8] {
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
  data
}

fn reason_to_string(reason_bytes: &[u8]) -> String {
  if reason_bytes.is_ascii() {
    // SAFETY: `is_ascii` guarantees valid UTF-8.
    return String::from(unsafe { core::str::from_utf8_unchecked(reason_bytes) });
  }
  String::from_utf8_lossy(reason_bytes).into_owned()
}

fn finish_body(
  headers: &mut Headers,
  body_vec: Bytes,
  trailers: Headers,
  max_body: usize,
) -> Result<(Bytes, Headers), ParseError> {
  let decompressed_body = Response::decompress_body_if_needed(headers, body_vec, max_body)?;
  Ok((decompressed_body, trailers))
}

fn decode_body_bytes(
  input: &[u8],
  strategy: BodyReadStrategy,
) -> Result<(Bytes, Headers), ParseError> {
  match strategy {
    BodyReadStrategy::NoBody => Ok((Bytes::new(), Headers::new())),
    BodyReadStrategy::ContentLength(len) => {
      if input.len() < len {
        return Err(ParseError::UnexpectedEndOfInput);
      }
      let body_data = input.get(..len).ok_or(ParseError::UnexpectedEndOfInput)?;
      if input.len() > len {
        return Err(ParseError::ExtraDataAfterResponse);
      }
      Ok((Bytes::copy_from_slice(body_data), Headers::new()))
    },
    BodyReadStrategy::Chunked => decode_chunked(input),
    BodyReadStrategy::UntilClose => Ok((Bytes::copy_from_slice(input), Headers::new())),
  }
}

fn decode_body_owned(
  input: Bytes,
  strategy: BodyReadStrategy,
) -> Result<(Bytes, Headers), ParseError> {
  match strategy {
    BodyReadStrategy::NoBody => Ok((Bytes::new(), Headers::new())),
    BodyReadStrategy::ContentLength(len) => {
      if input.len() < len {
        return Err(ParseError::UnexpectedEndOfInput);
      }
      if input.len() > len {
        return Err(ParseError::ExtraDataAfterResponse);
      }
      // Transport already owns exact CL bytes — no second allocation.
      Ok((input, Headers::new()))
    },
    BodyReadStrategy::Chunked => decode_chunked(&input),
    BodyReadStrategy::UntilClose => {
      // Already the full body buffer from the transport.
      Ok((input, Headers::new()))
    },
  }
}

fn decode_chunked(input: &[u8]) -> Result<(Bytes, Headers), ParseError> {
  let mut decoder = ChunkedDecoder::new();
  let mut output = Vec::new();
  let remaining = decoder.decode_chunk(input, &mut output)?;
  if !remaining.is_empty() {
    return Err(ParseError::ExtraDataAfterResponse);
  }
  Ok((Bytes::from(output), decoder.take_trailers()))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod response_helpers_tests {
  use super::*;

  #[test]
  fn status_class_helpers() {
    let mk = |code| Response::from_parts(code, String::new(), Headers::new(), Vec::new(), Headers::new());
    assert!(mk(200).is_success());
    assert!(mk(301).is_redirect());
    assert!(mk(404).is_client_error());
    assert!(mk(500).is_server_error());
    assert!(!mk(199).is_success());
  }

  #[test]
  fn text_and_into_bytes() {
    let r = Response::from_parts(200, String::new(), Headers::new(), b"hi".to_vec(), Headers::new());
    assert_eq!(r.status_code(), 200);
    assert_eq!(r.body(), b"hi");
    assert_eq!(r.to_text().unwrap(), "hi");
    let r2 = Response::from_parts(200, String::new(), Headers::new(), b"data".to_vec(), Headers::new());
    assert_eq!(r2.into_string().unwrap(), "data");
    let r3 = Response::from_parts(200, String::new(), Headers::new(), b"raw".to_vec(), Headers::new());
    let owned: alloc::vec::Vec<u8> = r3.into_bytes();
    assert_eq!(owned, b"raw");
  }

  #[test]
  fn into_string_preserves_response_on_utf8_error() {
    let r = Response::from_parts(
      201,
      String::from("Created"),
      Headers::new(),
      Vec::from([0xffu8]),
      Headers::new(),
    );
    let err = r.into_string().unwrap_err();
    assert_eq!(err.response().status_code(), 201);
    assert_eq!(err.response().reason(), "Created");
    assert_eq!(err.response().body(), &[0xff]);
  }

  #[test]
  fn into_string_reclaims_unique_body_allocation() {
    let payload = b"unique-owned-body-payload".to_vec();
    let ptr = payload.as_ptr();
    let r = Response::from_parts(200, String::new(), Headers::new(), payload, Headers::new());
    let text = r.into_string().unwrap();
    // Unique body buffer → `Vec` → `String` must reclaim, not copy.
    assert_eq!(text.as_ptr(), ptr);
    assert_eq!(text, "unique-owned-body-payload");
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
    assert_eq!(resp.body(), b"data");
    assert_eq!(resp.header("content-encoding"), Some("br"));
    assert_eq!(resp.header("content-length"), Some("4"));

    let identity = b"HTTP/1.1 200 OK\r\nContent-Encoding: identity\r\nContent-Length: 4\r\n\r\ndata";
    assert_eq!(Response::parse(identity).unwrap().body(), b"data");
  }

  #[cfg(feature = "gzip")]
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
    assert_eq!(resp.body(), b"hi");
    assert!(resp.header("content-encoding").is_none());
    assert!(resp.header("content-length").is_none());
  }

  #[cfg(feature = "gzip")]
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

  #[cfg(feature = "gzip")]
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
    assert_eq!(resp.body(), b"hi");
    assert!(resp.header("content-encoding").is_none());
  }

  #[cfg(feature = "zstd")]
  #[test]
  fn zstd_decompress_strips_content_encoding_and_length() {
    use alloc::string::ToString;
    // `zstd -c` of `hi` (15-byte frame)
    let zstd_body: &[u8] = &[
      0x28, 0xb5, 0x2f, 0xfd, 0x04, 0x58, 0x11, 0x00, 0x00, 0x68, 0x69, 0xfa, 0x38, 0x26, 0xea,
    ];
    let mut msg = Vec::from(&b"HTTP/1.1 200 OK\r\nContent-Encoding: zstd\r\nContent-Length: "[..]);
    msg.extend_from_slice(zstd_body.len().to_string().as_bytes());
    msg.extend_from_slice(b"\r\n\r\n");
    msg.extend_from_slice(zstd_body);

    let resp = Response::parse(&msg).unwrap();
    assert_eq!(resp.body(), b"hi");
    assert!(resp.header("content-encoding").is_none());
    assert!(resp.header("content-length").is_none());
  }
}
