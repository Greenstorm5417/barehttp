//! Parsed HTTP/1.1 responses and body-length strategy.

extern crate alloc;
use crate::error::{DecompressError, IntoStringError, ParseError};
use crate::headers::Headers;
use crate::parser::chunked::ChunkedDecoder;
use crate::parser::headers::{ScannedFields, pack_fields, pack_fields_with_tail, scan_fields};
use crate::parser::version::{Version, parse_status_line};
use alloc::string::String;
use alloc::vec::Vec;
use bytes::Bytes;
use compact_str::CompactString;

#[cfg(feature = "zstd")]
use ruzstd::decoding::StreamingDecoder;

/// Status line + scanned header fields of a response head (views into the input).
#[derive(Debug)]
pub(crate) struct ScannedHead<'a> {
  pub(crate) version: Version,
  pub(crate) status_code: u16,
  pub(crate) reason: &'a [u8],
  /// Header section bytes the field spans are relative to.
  pub(crate) section: &'a [u8],
  pub(crate) fields: ScannedFields,
  /// Bytes of `input` through the blank line (leading CRLFs included).
  pub(crate) consumed: usize,
}

impl ScannedHead<'_> {
  /// Copy header names/values into one exact arena (the only header copy).
  pub(crate) fn into_headers(self) -> Headers {
    pack_fields(self.section, self.fields)
  }

  /// Like [`Self::into_headers`] with room for a `tail`-byte body after the fields.
  pub(crate) fn into_headers_with_tail(
    self,
    tail: usize,
  ) -> Headers {
    pack_fields_with_tail(self.section, self.fields, tail)
  }

  /// A `Content-Encoding` field is present (the body may be decompressed).
  pub(crate) fn has_content_encoding(&self) -> bool {
    self
      .fields
      .pairs(self.section)
      .any(|(n, _)| n.eq_ignore_ascii_case(b"content-encoding"))
  }
}

/// Largest body stored in the header arena's tail (one allocation per
/// response). Bigger bodies get their own buffer so [`Response::into_bytes`]
/// stays a zero-copy hand-off.
pub(crate) const TAIL_BODY_MAX: usize = 64 * 1024;

/// Where a [`Response`] keeps its body bytes.
#[derive(Debug)]
enum Body {
  /// `headers.arena()[start..start + len]` (small bodies share the header
  /// allocation; explicit length so later arena appends never leak in).
  Tail(u32, u32),
  /// Separate buffer (large / decompressed / caller-built bodies).
  Bytes(Bytes),
}

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
pub struct Response {
  status_code: u16,
  reason: CompactString,
  headers: Headers,
  body: Body,
  trailers: Headers,
}

impl Response {
  /// Build a response from parts (crate-internal / tests).
  #[must_use]
  pub(crate) fn from_parts(
    status_code: u16,
    reason: impl Into<CompactString>,
    headers: Headers,
    body: impl Into<Bytes>,
    trailers: Headers,
  ) -> Self {
    Self {
      status_code,
      reason: reason.into(),
      headers,
      body: Body::Bytes(body.into()),
      trailers,
    }
  }

  /// Response whose body is `headers.arena()[start..start + len]` (single allocation).
  #[must_use]
  pub(crate) const fn from_tail(
    status_code: u16,
    reason: CompactString,
    headers: Headers,
    (start, len): (u32, u32),
    trailers: Headers,
  ) -> Self {
    Self {
      status_code,
      reason,
      headers,
      body: Body::Tail(start, len),
      trailers,
    }
  }

  /// Status code (e.g. 200).
  ///
  /// [`Self::status`] is a deprecated alias.
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
  /// [`Self::as_bytes`] is a deprecated alias.
  #[must_use]
  pub fn body(&self) -> &[u8] {
    match &self.body {
      Body::Tail(start, len) => tail_slice(self.headers.arena(), *start, *len),
      Body::Bytes(b) => b,
    }
  }

  /// Trailer fields from chunked responses (RFC 9112 §7.1.2).
  ///
  /// Same type as [`Self::headers`]: case-insensitive lookup, ordered iteration.
  #[must_use]
  pub const fn trailers(&self) -> &Headers {
    &self.trailers
  }

  /// Parse a complete buffered HTTP/1.1 response.
  ///
  /// The scan is zero-copy over `input`; the result owns exactly two buffers:
  /// a packed name/value arena and an exact-size body (uniquely owned, so
  /// [`Self::into_bytes`] / [`Self::into_string`] reclaim it without copying).
  ///
  /// # Errors
  /// [`ParseError`] when the status line, headers, framing, or body are illegal.
  pub fn parse(input: &[u8]) -> Result<Self, ParseError> {
    let head = Self::scan_head(input)?;
    let strategy = Self::body_read_strategy_pairs(head.fields.pairs(head.section), head.status_code, head.version)?;
    let rest = input.get(head.consumed..).unwrap_or(&[]);
    let status_code = head.status_code;
    let reason = reason_to_compact(head.reason);

    // Small identity-coded bodies share the header allocation.
    if !head.has_content_encoding()
      && let Some(body_len) = tail_body_len(rest, strategy)?
    {
      let mut headers = head.into_headers_with_tail(body_len);
      let start = u32::try_from(headers.arena().len()).map_err(|_| ParseError::BodyExceedsLimit(TAIL_BODY_MAX))?;
      let tail = headers.arena_tail_mut();
      let trailers = if strategy == BodyReadStrategy::Chunked {
        ChunkedDecoder::decode_buffered_into(rest, tail)?
      } else {
        tail.extend_from_slice(rest.get(..body_len).unwrap_or(&[]));
        Headers::new()
      };
      let len = u32::try_from(headers.arena().len().saturating_sub(start as usize)).unwrap_or(0);
      return Ok(Self::from_tail(status_code, reason, headers, (start, len), trailers));
    }

    let (wire_body, wire_trailers) = decode_body_borrowed(rest, strategy)?;
    let mut headers = head.into_headers();
    let (body, trailers) = finish_body(&mut headers, wire_body, wire_trailers, usize::MAX)?;
    Ok(Self {
      status_code,
      reason,
      headers,
      body: Body::Bytes(body),
      trailers,
    })
  }

  /// Status line + header field scan as offsets (no name/value copies yet).
  ///
  /// # Errors
  /// [`ParseError`] when the status line or header section is illegal.
  pub(crate) fn scan_head(input: &[u8]) -> Result<ScannedHead<'_>, ParseError> {
    let data = skip_leading_crlf(input);
    let (version, status_code, reason, section) = parse_status_line(data)?;
    let fields = scan_fields(section)?;
    let consumed = input
      .len()
      .saturating_sub(section.len())
      .saturating_add(fields.consumed);
    Ok(ScannedHead {
      version,
      status_code,
      reason,
      section,
      fields,
      consumed,
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
    // Common case: no Content-Encoding (length-filtered scan, no value parsing).
    if !headers.contains(Headers::CONTENT_ENCODING) {
      return Ok(body_bytes);
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

    // Decompress from the TE/CL buffer as a slice — no Bytes→Vec of the
    // compressed body solely to feed the inflater.
    let mut decoded = if let Some(coding) = single {
      decompress_coding(coding, &body_bytes, max_body)?
    } else {
      let mut out = Vec::new();
      let mut scratch = Vec::new();
      for (i, coding) in multi.iter().rev().enumerate() {
        if i == 0 {
          decompress_coding_into(coding, &body_bytes, max_body, &mut out)?;
        } else {
          // Ping-pong: previous output becomes next input; reuse both capacities.
          core::mem::swap(&mut out, &mut scratch);
          decompress_coding_into(coding, &scratch, max_body, &mut out)?;
        }
        if out.len() > max_body {
          return Err(ParseError::BodyExceedsLimit(max_body));
        }
      }
      out
    };

    if decoded.len() > max_body {
      return Err(ParseError::BodyExceedsLimit(max_body));
    }

    headers.remove(Headers::CONTENT_ENCODING);
    headers.remove(Headers::CONTENT_LENGTH);
    // len == cap: `Bytes::from` adopts the Vec (no shared header, no slack kept).
    if decoded.len() != decoded.capacity() {
      decoded.shrink_to_fit();
    }
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

  /// Own a reason-phrase byte slice.
  #[must_use]
  pub(crate) fn reason_owned(reason_bytes: &[u8]) -> CompactString {
    reason_to_compact(reason_bytes)
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

  /// Framing strategy from `(name, value)` byte pairs (before packing [`Headers`]).
  ///
  /// # Errors
  /// Framing [`ParseError`]s when headers conflict or are illegal for the status/version.
  pub(crate) fn body_read_strategy_pairs<'a, I>(
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

  /// Decode a body already held in an owned buffer (two-phase / transport path).
  ///
  /// For `Content-Length` and until-close, returns that buffer with no copy.
  /// Chunked wire reuses a contiguous payload span from `body_bytes` when possible
  /// ([`ChunkedDecoder::decode_buffered`]); the live transport path uses
  /// [`Self::finish_decoded_body`] after single-pass decode on the wire instead.
  ///
  /// `max_body` caps wire-decoded and decompressed size ([`ParseError::BodyExceedsLimit`]).
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

  /// Finish a body the transport already decoded (chunked single-pass on recv).
  ///
  /// Applies Content-Encoding decompression and the body size cap; does not
  /// re-parse chunked framing.
  ///
  /// # Errors
  /// [`ParseError`] when decompression fails or the body exceeds `max_body`.
  pub(crate) fn finish_decoded_body(
    body_bytes: Bytes,
    headers: &mut Headers,
    trailers: Headers,
    max_body: usize,
  ) -> Result<(Bytes, Headers), ParseError> {
    finish_body(headers, body_bytes, trailers, max_body)
  }

  /// Deprecated alias of [`Self::status_code`].
  #[deprecated(since = "0.1.0", note = "use `status_code`")]
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
  #[deprecated(since = "0.1.0", note = "use `body`")]
  #[must_use]
  pub fn as_bytes(&self) -> &[u8] {
    self.body()
  }

  /// Body as UTF-8 text (borrowed).
  ///
  /// Leaves `self` intact on failure, so status and headers stay available.
  /// Error type is [`core::str::Utf8Error`]; it converts via [`From`] / `?` into
  /// [`crate::Error::Utf8Error`].
  ///
  /// # Errors
  /// [`core::str::Utf8Error`] if the body is not valid UTF-8.
  pub fn to_text(&self) -> Result<&str, core::str::Utf8Error> {
    core::str::from_utf8(self.body())
  }

  /// Consume the response; return the body as a UTF-8 [`String`].
  ///
  /// Non-UTF-8 body: [`IntoStringError`] holds the original [`Response`]
  /// (status, headers, body). Converting that error to [`crate::Error`] via
  /// [`From`] drops the response; recover with [`IntoStringError::into_response`]
  /// or [`IntoStringError::response`] first.
  ///
  /// Unique body buffer: success builds the [`String`] with no copy.
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
    let vec = self.into_bytes();
    // SAFETY: `from_utf8` succeeded on these exact bytes before the move.
    Ok(unsafe { String::from_utf8_unchecked(vec) })
  }

  /// Consume the response; return body bytes as a [`Vec<u8>`].
  ///
  /// For a borrowed view, use [`Self::body`]. Reclaims the allocation without a
  /// copy when the internal buffer is uniquely owned.
  #[must_use]
  pub fn into_bytes(self) -> Vec<u8> {
    match self.body {
      Body::Tail(start, len) => {
        // Reuse the shared allocation: slide the body over the header bytes.
        let mut v = self.headers.into_arena();
        let begin = (start as usize).min(v.len());
        v.truncate(begin.saturating_add(len as usize));
        v.drain(..begin);
        v
      },
      Body::Bytes(b) => Vec::from(b),
    }
  }
}

impl AsRef<[u8]> for Response {
  fn as_ref(&self) -> &[u8] {
    self.body()
  }
}

impl Clone for Response {
  fn clone(&self) -> Self {
    let (headers, body) = match &self.body {
      // The body lives in the arena tail: keep those bytes in the clone.
      Body::Tail(start, len) => (self.headers.clone_raw(), Body::Tail(*start, *len)),
      Body::Bytes(b) => (self.headers.clone(), Body::Bytes(b.clone())),
    };
    Self {
      status_code: self.status_code,
      reason: self.reason.clone(),
      headers,
      body,
      trailers: self.trailers.clone(),
    }
  }
}

impl core::fmt::Debug for Response {
  fn fmt(
    &self,
    f: &mut core::fmt::Formatter<'_>,
  ) -> core::fmt::Result {
    f.debug_struct("Response")
      .field("status_code", &self.status_code)
      .field("reason", &self.reason)
      .field("headers", &self.headers)
      .field("body", &bytes::Bytes::copy_from_slice(self.body()))
      .field("trailers", self.trailers())
      .finish()
  }
}

impl PartialEq for Response {
  fn eq(
    &self,
    other: &Self,
  ) -> bool {
    self.status_code == other.status_code
      && self.reason == other.reason
      && self.headers == other.headers
      && self.body() == other.body()
      && self.trailers() == other.trailers()
  }
}

impl Eq for Response {}

impl core::hash::Hash for Response {
  fn hash<H: core::hash::Hasher>(
    &self,
    state: &mut H,
  ) {
    self.status_code.hash(state);
    self.reason.hash(state);
    self.headers.hash(state);
    self.body().hash(state);
    self.trailers().hash(state);
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

/// Decompress one Content-Encoding layer from a borrowed TE/CL body slice.
fn decompress_coding(
  coding: &str,
  body: &[u8],
  max_body: usize,
) -> Result<Vec<u8>, ParseError> {
  let mut out = Vec::new();
  decompress_coding_into(coding, body, max_body, &mut out)?;
  Ok(out)
}

/// Like [`decompress_coding`], writing into `out` (cleared; capacity reused).
///
/// Not `const`: gzip/zstd paths call non-const decompressors; without those
/// features clippy would otherwise suggest `const fn`.
#[allow(clippy::missing_const_for_fn)]
fn decompress_coding_into(
  coding: &str,
  body: &[u8],
  max_body: usize,
  out: &mut Vec<u8>,
) -> Result<(), ParseError> {
  #[cfg(feature = "gzip")]
  if coding.eq_ignore_ascii_case("gzip") {
    return crate::gzip::decompress_gzip_into(body, max_body, out).map_err(|e| map_decompress_error(e, max_body));
  }

  #[cfg(feature = "gzip")]
  if coding.eq_ignore_ascii_case("deflate") {
    return crate::gzip::decompress_http_deflate_into(body, max_body, out)
      .map_err(|e| map_decompress_error(e, max_body));
  }

  #[cfg(feature = "zstd")]
  if coding.eq_ignore_ascii_case("zstd") {
    return decompress_zstd_into(body, max_body, out);
  }

  let _ = (coding, body, max_body, out);
  Err(ParseError::Decompression(DecompressError::InvalidInput))
}

/// Zstd → `out` (cleared; capacity reused for fused Content-Encoding).
///
/// When the frame header carries a content-size, preallocate exactly and read
/// straight into `out` (no 8KiB scratch / realloc churn). Unknown size keeps a
/// mild heuristic reserve then scratch-extends.
#[cfg(feature = "zstd")]
fn decompress_zstd_into(
  body: &[u8],
  max_body: usize,
  out: &mut Vec<u8>,
) -> Result<(), ParseError> {
  use ruzstd::io_nostd::Read;

  out.clear();
  let mut decoder =
    StreamingDecoder::new(body).map_err(|_| ParseError::Decompression(DecompressError::InvalidInput))?;

  let known = decoder.decoder.content_size();
  if known > 0 {
    let size = usize::try_from(known).map_err(|_| ParseError::Decompression(DecompressError::InvalidInput))?;
    if size > max_body {
      return Err(ParseError::BodyExceedsLimit(max_body));
    }
    // Exact prealloc into fused `out` — decode directly, no scratch copy.
    out.resize(size, 0);
    let mut filled = 0usize;
    while filled < size {
      let dst = out.get_mut(filled..size).unwrap_or(&mut []);
      let n = decoder
        .read(dst)
        .map_err(|_| ParseError::Decompression(DecompressError::InvalidInput))?;
      if n == 0 {
        break;
      }
      filled = filled.saturating_add(n);
    }
    out.truncate(filled);
    if filled != size {
      return Err(ParseError::Decompression(DecompressError::InvalidInput));
    }
    // Declared FCS must be exact; trailing output would bypass the limit check.
    let mut probe = [0u8; 1];
    let extra = decoder
      .read(&mut probe)
      .map_err(|_| ParseError::Decompression(DecompressError::InvalidInput))?;
    if extra != 0 {
      return Err(ParseError::Decompression(DecompressError::InvalidInput));
    }
    return Ok(());
  }

  // No frame content-size: heuristic reserve, then scratch-extend.
  out.reserve(body.len().saturating_mul(2).min(max_body));
  let mut buf = [0u8; 8192];
  loop {
    let n = decoder
      .read(&mut buf)
      .map_err(|_| ParseError::Decompression(DecompressError::InvalidInput))?;
    if n == 0 {
      break;
    }
    if out.len().saturating_add(n) > max_body {
      return Err(ParseError::BodyExceedsLimit(max_body));
    }
    if let Some(slice) = buf.get(..n) {
      out.extend_from_slice(slice);
    }
  }
  Ok(())
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

fn reason_to_compact(reason_bytes: &[u8]) -> CompactString {
  if reason_bytes.is_ascii() {
    // SAFETY: `is_ascii` guarantees valid UTF-8.
    return CompactString::from(unsafe { core::str::from_utf8_unchecked(reason_bytes) });
  }
  CompactString::from(String::from_utf8_lossy(reason_bytes).as_ref())
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
      // Already owns exact CL bytes — no second allocation.
      Ok((input, Headers::new()))
    },
    BodyReadStrategy::Chunked => ChunkedDecoder::decode_buffered(&input),
    BodyReadStrategy::UntilClose => {
      // Already the full body buffer.
      Ok((input, Headers::new()))
    },
  }
}

/// Like [`decode_body_owned`] over a borrowed body: one exact-size copy.
fn decode_body_borrowed(
  input: &[u8],
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
      Ok((owned_bytes(input), Headers::new()))
    },
    BodyReadStrategy::Chunked => ChunkedDecoder::decode_buffered(input),
    BodyReadStrategy::UntilClose => Ok((owned_bytes(input), Headers::new())),
  }
}

/// `arena[start..start + len]`, empty when out of range.
#[inline]
pub(crate) fn tail_slice(
  arena: &[u8],
  start: u32,
  len: u32,
) -> &[u8] {
  let s = start as usize;
  arena.get(s..s.saturating_add(len as usize)).unwrap_or(&[])
}

/// Body length when it may be stored in the header arena tail (validates
/// Content-Length framing like [`decode_body_borrowed`]).
fn tail_body_len(
  rest: &[u8],
  strategy: BodyReadStrategy,
) -> Result<Option<usize>, ParseError> {
  let len = match strategy {
    BodyReadStrategy::NoBody => 0,
    BodyReadStrategy::ContentLength(len) => {
      if rest.len() < len {
        return Err(ParseError::UnexpectedEndOfInput);
      }
      if rest.len() > len {
        return Err(ParseError::ExtraDataAfterResponse);
      }
      len
    },
    BodyReadStrategy::UntilClose => rest.len(),
    BodyReadStrategy::Chunked => ChunkedDecoder::buffered_payload_len(rest)?,
  };
  Ok((len <= TAIL_BODY_MAX).then_some(len))
}

/// Exact-capacity unique copy (`Vec` with `len == cap` → no `Bytes` shared header).
fn owned_bytes(data: &[u8]) -> Bytes {
  if data.is_empty() {
    Bytes::new()
  } else {
    Bytes::from(data.to_vec())
  }
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
  fn small_body_shares_header_allocation() {
    let r = Response::parse(b"HTTP/1.1 200 OK\r\nX-A: 1\r\nContent-Length: 5\r\n\r\nhello").unwrap();
    assert!(matches!(r.body, Body::Tail(_, 5)));
    assert_eq!(r.body(), b"hello");
    let arena = r.headers().arena().as_ptr();
    assert_eq!(arena.wrapping_add(r.headers().arena().len() - 5), r.body().as_ptr());

    // Equality / hashing / clone are representation-independent.
    let built = Response::from_parts(200, "OK", r.headers().clone(), b"hello".to_vec(), Headers::new());
    assert_eq!(r, built);
    let c = r.clone();
    assert_eq!(c.body(), b"hello");
    assert_eq!(c, r);

    // into_bytes reuses the arena allocation (no new buffer).
    let v = r.into_bytes();
    assert_eq!(v, b"hello");
    assert_eq!(v.as_ptr(), arena);
  }

  #[test]
  fn chunked_and_until_close_bodies_use_tail() {
    let r =
      Response::parse(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nab\r\n1\r\nc\r\n0\r\nT: v\r\n\r\n")
        .unwrap();
    assert!(matches!(r.body, Body::Tail(_, 3)));
    assert_eq!(r.body(), b"abc");
    assert_eq!(r.trailers().get("t"), Some("v"));
    assert_eq!(r.into_string().unwrap(), "abc");

    let u = Response::parse(b"HTTP/1.1 200 OK\r\nX: y\r\n\r\nuntil-close").unwrap();
    assert_eq!(u.body(), b"until-close");
    let e = Response::parse(b"HTTP/1.1 204 No Content\r\nX: y\r\n\r\n").unwrap();
    assert_eq!(e.body(), b"");
  }

  #[test]
  fn large_body_is_separate_and_zero_copy() {
    let mut wire = alloc::format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", TAIL_BODY_MAX + 1).into_bytes();
    wire.resize(wire.len() + TAIL_BODY_MAX + 1, b'z');
    let r = Response::parse(&wire).unwrap();
    assert!(matches!(r.body, Body::Bytes(_)));
    let p = r.body().as_ptr();
    let owned = r.into_bytes();
    assert_eq!(owned.as_ptr(), p);
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
  fn scan_head_builds_headers() {
    let input = b"\r\nHTTP/1.1 200 OK\r\nHost: a\r\nX-A: 1\r\n\r\nbody";
    let head = Response::scan_head(input).unwrap();
    let rest = input.get(head.consumed..).unwrap();
    assert_eq!(head.status_code, 200);
    assert_eq!(reason_to_compact(head.reason), "OK");
    assert_eq!(head.version, Version::HTTP_11);
    let headers = head.into_headers();
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
  fn chunked_then_gzip_content_encoding_fused() {
    // gzip.compress(b"hi") — TE chunked wraps CE gzip; decode must feed CE as slice.
    let gzipped: &[u8] = &[
      0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0xff, 0xcb, 0xc8, 0x04, 0x00, 0xac, 0x2a, 0x93, 0xd8, 0x02,
      0x00, 0x00, 0x00,
    ];
    let mut msg = Vec::from(&b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Encoding: gzip\r\n\r\n"[..]);
    msg.extend_from_slice(alloc::format!("{:X}\r\n", gzipped.len()).as_bytes());
    msg.extend_from_slice(gzipped);
    msg.extend_from_slice(b"\r\n0\r\n\r\n");

    let resp = Response::parse(&msg).unwrap();
    assert_eq!(resp.body(), b"hi");
    assert!(resp.header("content-encoding").is_none());
  }

  #[cfg(feature = "gzip")]
  #[test]
  fn chunked_multi_then_gzip_content_encoding() {
    // Same gzip payload split across two chunks — forces exact-capacity copy path.
    let gzipped: &[u8] = &[
      0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0xff, 0xcb, 0xc8, 0x04, 0x00, 0xac, 0x2a, 0x93, 0xd8, 0x02,
      0x00, 0x00, 0x00,
    ];
    let (a, b) = gzipped.split_at(10);
    let mut msg = Vec::from(&b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Encoding: gzip\r\n\r\n"[..]);
    msg.extend_from_slice(alloc::format!("{:X}\r\n", a.len()).as_bytes());
    msg.extend_from_slice(a);
    msg.extend_from_slice(b"\r\n");
    msg.extend_from_slice(alloc::format!("{:X}\r\n", b.len()).as_bytes());
    msg.extend_from_slice(b);
    msg.extend_from_slice(b"\r\n0\r\n\r\n");

    let resp = Response::parse(&msg).unwrap();
    assert_eq!(resp.body(), b"hi");
    assert!(resp.header("content-encoding").is_none());
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
    let err = Response::parse_body_from_owned(Bytes::copy_from_slice(gzipped), &mut headers, 200, Version::HTTP_11, 1)
      .unwrap_err();
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
    // `zstd -c` of `hi` (15-byte frame; no FCS — stdin path)
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

  #[cfg(feature = "zstd")]
  #[test]
  fn zstd_frame_content_size_prealloc_and_limit() {
    use alloc::string::ToString;
    // `zstd -c file` of 100×'a' — single-segment FCS=100, wire ≈21 bytes
    let zstd_body: &[u8] = &[
      0x28, 0xb5, 0x2f, 0xfd, 0x24, 0x64, 0x45, 0x00, 0x00, 0x10, 0x61, 0x61, 0x01, 0x00, 0x3f, 0x01, 0x2c, 0xb3, 0xcf,
      0xde, 0xb1,
    ];

    let mut msg = Vec::from(&b"HTTP/1.1 200 OK\r\nContent-Encoding: zstd\r\nContent-Length: "[..]);
    msg.extend_from_slice(zstd_body.len().to_string().as_bytes());
    msg.extend_from_slice(b"\r\n\r\n");
    msg.extend_from_slice(zstd_body);
    let resp = Response::parse(&msg).unwrap();
    assert_eq!(resp.body(), b"a".repeat(100).as_slice());

    // Wire fits (21 ≤ 50) but FCS=100 → reject before scratch growth / full inflate.
    let mut headers = Headers::new();
    headers.insert("Content-Encoding", "zstd");
    headers.insert("Content-Length", zstd_body.len().to_string());
    let err = Response::parse_body_from_owned(
      Bytes::copy_from_slice(zstd_body),
      &mut headers,
      200,
      Version::HTTP_11,
      50,
    )
    .unwrap_err();
    assert_eq!(err, ParseError::BodyExceedsLimit(50));
  }

  #[cfg(feature = "zstd")]
  #[test]
  fn zstd_into_reuses_output_capacity() {
    // FCS frame of `hi` (size 2); fused CE path must keep pooled capacity.
    let zstd_body: &[u8] = &[
      0x28, 0xb5, 0x2f, 0xfd, 0x24, 0x02, 0x11, 0x00, 0x00, 0x68, 0x69, 0xfa, 0x38, 0x26, 0xea,
    ];
    let mut out = Vec::with_capacity(64);
    decompress_zstd_into(zstd_body, 64, &mut out).unwrap();
    assert_eq!(out, b"hi");
    let cap = out.capacity();
    assert!(cap >= 64);

    decompress_zstd_into(zstd_body, 64, &mut out).unwrap();
    assert_eq!(out, b"hi");
    assert_eq!(out.capacity(), cap, "second zstd decode must keep pooled capacity");
  }
}

#[cfg(kani)]
mod kani_length_proofs {
  use super::parse_decimal_usize;

  #[kani::proof]
  fn empty_digits_none() {
    assert!(parse_decimal_usize(b"").is_none());
  }

  #[kani::proof]
  fn small_decimal_ok() {
    assert_eq!(parse_decimal_usize(b"42"), Some(42));
  }

  /// Overflowing digit strings return `None` (checked_mul/add); does not panic.
  #[kani::proof]
  #[kani::unwind(64)]
  fn overflow_digits_none() {
    // More digits than fit in u64/usize on common targets.
    let digits = b"99999999999999999999999999999999";
    assert!(parse_decimal_usize(digits).is_none());
  }
}
