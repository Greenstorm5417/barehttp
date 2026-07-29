//! Crate error types.
//!
//! # Body size limits
//!
//! Wire and decompress code may emit [`ParseError::BodyExceedsLimit`] while buffering a
//! response. [`From<ParseError> for Error`] maps that variant to
//! [`Error::BodyExceedsLimit`], so `Error::Parse` never carries it. Match
//! [`Error::BodyExceedsLimit`] at the client boundary (a nested `Parse` form would have
//! the same `Display` text).

extern crate alloc;

/// Errors from gzip/deflate content-coding decompression.
///
/// Always at the crate root: [`ParseError::Decompression`] can appear even when the
/// `gzip` feature (and [`crate::gzip`] helpers) is off. With `gzip` enabled,
/// [`crate::gzip`] re-exports this same type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecompressError {
  /// Invalid or truncated input.
  InvalidInput,
  /// Uncompressed output would exceed the configured limit.
  LimitExceeded,
}

impl core::fmt::Display for DecompressError {
  fn fmt(
    &self,
    f: &mut core::fmt::Formatter<'_>,
  ) -> core::fmt::Result {
    match self {
      Self::InvalidInput => f.write_str("invalid gzip/deflate input"),
      Self::LimitExceeded => f.write_str("decompressed output exceeds size limit"),
    }
  }
}

impl core::error::Error for DecompressError {}

/// HTTP/1.1 message parse failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParseError {
  /// Status-line version token is not `HTTP/x.y`.
  InvalidHttpVersion,
  /// Status code is missing or not three DIGIT.
  InvalidStatusCode,
  /// Reason phrase bytes are illegal.
  InvalidReasonPhrase,
  /// Header field name is not a token.
  InvalidHeaderName,
  /// Header field value contains illegal octets.
  InvalidHeaderValue,
  /// URI / absolute-form could not be parsed.
  InvalidUri,
  /// Required CRLF (`\r\n`) missing.
  MissingCrlf,
  /// Bare CR without LF.
  BareCarriageReturn,
  /// Input ended before a complete message.
  UnexpectedEndOfInput,
  /// Illegal whitespace in the message.
  InvalidWhitespace,
  /// Chunk-size hex is malformed.
  InvalidChunkSize,
  /// `Content-Length` value is not a valid length.
  InvalidContentLength,
  /// Both `Transfer-Encoding` and `Content-Length` present (RFC 9112 §6.3).
  ConflictingFraming,
  /// `chunked` is present but not the final coding (RFC 9112 §6.3).
  ChunkedNotFinal,
  /// Whitespace between start-line and first header field (RFC 9112 §2.2).
  WhitespaceBeforeHeaders,
  /// Bytes after a complete response body (RFC 9112 §6.3).
  ExtraDataAfterResponse,
  /// HTTP/1.1 request missing `Host` (RFC 9112 §3.2).
  MissingHostHeader,
  /// Obs-fold in a header value (RFC 9112 §5.2).
  ObsoleteFoldInHeader,
  /// `Transfer-Encoding` on a status that must not carry it (RFC 9112 §6.1).
  InvalidTransferEncodingForStatus,
  /// `TE` lists `chunked` (forbidden; RFC 9112 §7.4).
  ChunkedInTeHeader,
  /// `TE` present without `TE` in `Connection` (RFC 9112 §7.4).
  TeHeaderMissingConnection,
  /// More than one `Host` header (RFC 9112 §3.2).
  MultipleHostHeaders,
  /// `Host` value is not a valid authority (RFC 9112 §3.2).
  InvalidHostHeaderValue,
  /// `Transfer-Encoding` on HTTP/1.0 or earlier (RFC 9112 §6.1).
  TransferEncodingRequiresHttp11,
  /// `chunked` listed more than once (RFC 9112 §6.1).
  ChunkedAppliedMultipleTimes,
  /// Outbound request used `Transfer-Encoding`; this client frames bodies with
  /// `Content-Length` only (RFC 9112 §6.3).
  RequestTransferEncodingUnsupported,
  /// Content-coding decompression failed (preserves [`DecompressError`] as `source`).
  ///
  /// Limit failures are not stored here: they become [`ParseError::BodyExceedsLimit`]
  /// (and [`Error::BodyExceedsLimit`] at the client boundary).
  Decompression(DecompressError),
  /// Decompressed or wire body larger than the configured size limit.
  ///
  /// When converted with [`From<ParseError> for Error`], this becomes
  /// [`Error::BodyExceedsLimit`] (not nested under [`Error::Parse`]).
  BodyExceedsLimit(usize),
}

impl ParseError {
  const fn as_str(self) -> &'static str {
    match self {
      Self::InvalidHttpVersion => "invalid HTTP version",
      Self::InvalidStatusCode => "invalid status code",
      Self::InvalidReasonPhrase => "invalid reason phrase",
      Self::InvalidHeaderName => "invalid header name",
      Self::InvalidHeaderValue => "invalid header value",
      Self::InvalidUri => "invalid URI",
      Self::MissingCrlf => "missing CRLF",
      Self::BareCarriageReturn => "bare CR not allowed",
      Self::UnexpectedEndOfInput => "unexpected end of input",
      Self::InvalidWhitespace => "invalid whitespace",
      Self::InvalidChunkSize => "invalid chunk size",
      Self::InvalidContentLength => "invalid Content-Length value",
      Self::ConflictingFraming => "both Transfer-Encoding and Content-Length present",
      Self::ChunkedNotFinal => "chunked must be the final Transfer-Encoding",
      Self::WhitespaceBeforeHeaders => "whitespace found between start-line and first header",
      Self::ExtraDataAfterResponse => "extra data found after complete response",
      Self::MissingHostHeader => "Host header required for HTTP/1.1 requests",
      Self::ObsoleteFoldInHeader => "header value contains obs-fold (not allowed)",
      Self::InvalidTransferEncodingForStatus => "Transfer-Encoding not allowed for this status code",
      Self::ChunkedInTeHeader => "TE header must not contain 'chunked'",
      Self::TeHeaderMissingConnection => "TE header requires 'TE' in Connection header",
      Self::MultipleHostHeaders => "multiple Host headers present",
      Self::InvalidHostHeaderValue => "invalid Host header value format",
      Self::TransferEncodingRequiresHttp11 => "Transfer-Encoding requires HTTP/1.1 or higher",
      Self::ChunkedAppliedMultipleTimes => "chunked transfer coding applied multiple times",
      Self::RequestTransferEncodingUnsupported => "Transfer-Encoding on requests is unsupported; use Content-Length",
      Self::Decompression(_) => "failed to decompress response body",
      Self::BodyExceedsLimit(_) => "response body exceeds size limit",
    }
  }
}

impl core::fmt::Display for ParseError {
  fn fmt(
    &self,
    f: &mut core::fmt::Formatter<'_>,
  ) -> core::fmt::Result {
    match self {
      Self::BodyExceedsLimit(limit) => write!(f, "response body exceeds limit of {limit} bytes"),
      Self::Decompression(e) => write!(f, "failed to decompress response body: {e}"),
      other => f.write_str(other.as_str()),
    }
  }
}

impl core::error::Error for ParseError {
  fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
    match self {
      Self::Decompression(e) => Some(e),
      _ => None,
    }
  }
}

/// DNS lookup failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DnsError {
  /// OS resolver failed; payload is the platform error code.
  ResolutionFailed(i32),
  /// Hostname resolved to an empty address list.
  NoAddressesFound,
}

impl DnsError {
  const fn as_str(self) -> &'static str {
    match self {
      Self::ResolutionFailed(_) => "DNS resolution failed",
      Self::NoAddressesFound => "no addresses found for hostname",
    }
  }
}

impl core::fmt::Display for DnsError {
  fn fmt(
    &self,
    f: &mut core::fmt::Formatter<'_>,
  ) -> core::fmt::Result {
    match self {
      Self::ResolutionFailed(code) => write!(f, "DNS resolution failed: {code}"),
      Self::NoAddressesFound => f.write_str(self.as_str()),
    }
  }
}

impl core::error::Error for DnsError {}

/// Socket I/O failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SocketError {
  /// Socket is not connected.
  NotConnected,
  /// Peer refused the connection.
  ConnectionRefused,
  /// Deadline elapsed.
  TimedOut,
  /// Call interrupted (e.g. signal); may be retried.
  Interrupted,
  /// Address is unusable.
  InvalidAddress,
  /// Operation unsupported on this socket / platform.
  Unsupported,
  /// OS error; payload is the platform errno / WSA code.
  OsError(i32),
}

impl SocketError {
  const fn as_str(self) -> &'static str {
    match self {
      Self::NotConnected => "socket not connected",
      Self::ConnectionRefused => "connection refused",
      Self::TimedOut => "operation timed out",
      Self::Interrupted => "operation interrupted",
      Self::InvalidAddress => "invalid address",
      Self::Unsupported => "operation not supported",
      Self::OsError(_) => "OS error",
    }
  }
}

impl core::fmt::Display for SocketError {
  fn fmt(
    &self,
    f: &mut core::fmt::Formatter<'_>,
  ) -> core::fmt::Result {
    match self {
      Self::OsError(code) => write!(f, "OS error: {code}"),
      other => f.write_str(other.as_str()),
    }
  }
}

impl core::error::Error for SocketError {}

/// Bad request construction (illegal cookie octets, form fields plus an explicit body, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum InvalidRequest {
  /// Both form fields and an explicit body were set.
  FormAndBody,
  /// Cookie name or value contained `;` or a control octet.
  CookieOctet,
}

impl InvalidRequest {
  const fn as_str(self) -> &'static str {
    match self {
      Self::FormAndBody => "cannot set both form fields and an explicit body",
      Self::CookieOctet => "cookie name or value contains illegal octets",
    }
  }
}

impl core::fmt::Display for InvalidRequest {
  fn fmt(
    &self,
    f: &mut core::fmt::Formatter<'_>,
  ) -> core::fmt::Result {
    f.write_str(self.as_str())
  }
}

impl core::error::Error for InvalidRequest {}

/// [`crate::Response::into_string`] failed; the full response is preserved for recovery.
///
/// Implements [`From`] into [`Error`] as [`Error::Utf8Error`] (UTF-8 cause only).
/// Use [`Self::into_response`] / [`Self::response`] when you still need status/headers.
///
/// # Examples
///
/// ```
/// use barehttp::Response;
///
/// let bad = Response::parse(b"HTTP/1.1 201 Created\r\nContent-Length: 1\r\n\r\n\xff")
///   .map_err(barehttp::Error::from)?;
/// if let Err(err) = bad.into_string() {
///   assert_eq!(err.response().status_code(), 201);
///   assert_eq!(err.into_response().body(), &[0xff]);
/// }
/// # Ok::<(), barehttp::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntoStringError {
  // Boxed so `Result<T, IntoStringError>` stays small (C-GOOD-ERR / clippy::result_large_err).
  response: alloc::boxed::Box<crate::parser::Response>,
  error: core::str::Utf8Error,
}

impl IntoStringError {
  pub(crate) fn new(
    response: crate::parser::Response,
    error: core::str::Utf8Error,
  ) -> Self {
    Self {
      response: alloc::boxed::Box::new(response),
      error,
    }
  }

  /// Borrow the response (status, headers, body intact).
  #[must_use]
  pub fn response(&self) -> &crate::parser::Response {
    &self.response
  }

  /// Recover the response.
  #[must_use]
  pub fn into_response(self) -> crate::parser::Response {
    *self.response
  }

  /// The UTF-8 error that caused the failure.
  #[must_use]
  pub const fn utf8_error(&self) -> core::str::Utf8Error {
    self.error
  }
}

impl core::fmt::Display for IntoStringError {
  fn fmt(
    &self,
    f: &mut core::fmt::Formatter<'_>,
  ) -> core::fmt::Result {
    write!(f, "response body is not valid UTF-8: {}", self.error)
  }
}

impl core::error::Error for IntoStringError {
  fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
    Some(&self.error)
  }
}

/// Error from [`crate::HttpClient`] and the free `get` / `post` / ... functions.
///
/// # Examples
///
/// Recover a 4xx/5xx response when status-as-error is enabled:
///
/// ```
/// use barehttp::{Error, Response};
///
/// fn take_http_status(err: Error) -> Option<Response> {
///   match err {
///     Error::HttpStatus(_code, resp) => Some(*resp),
///     _ => None,
///   }
/// }
///
/// let resp = Response::parse(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
///   .map_err(Error::from)?;
/// let err = Error::HttpStatus(404, Box::new(resp));
/// let recovered = take_http_status(err).ok_or(Error::InvalidUrl)?;
/// assert_eq!(recovered.status_code(), 404);
/// # Ok::<(), Error>(())
/// ```
///
/// Body size limit maps to [`Error::BodyExceedsLimit`], not [`Error::Parse`]:
///
/// ```
/// use barehttp::{Error, ParseError};
///
/// let err: Error = ParseError::BodyExceedsLimit(1024).into();
/// assert!(matches!(err, Error::BodyExceedsLimit(1024)));
/// ```
///
/// UTF-8 body recovery keeps the full response on [`IntoStringError`] (not on
/// [`Error::Utf8Error`]); see that type's docs.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
  /// Wire / framing parse failure.
  ///
  /// Does not carry [`ParseError::BodyExceedsLimit`]; [`From`] maps it to
  /// [`Error::BodyExceedsLimit`].
  Parse(ParseError),
  /// DNS lookup failure.
  Dns(DnsError),
  /// Socket I/O failure.
  Socket(SocketError),
  /// URL could not be parsed or is unusable for this request.
  InvalidUrl,
  /// Redirect hops exceeded [`crate::config::Config::max_redirects`].
  TooManyRedirects,
  /// Redirect response had no `Location` header.
  MissingRedirectLocation,
  /// Redirect target already visited in this request.
  RedirectLoop,
  /// Redirect cannot be followed (e.g. 307/308 with a request body).
  RedirectFailed,
  /// 4xx/5xx when [`crate::config::Config::http_status_as_error`] is set.
  ///
  /// `(status, response)`, same shape as ureq 2.x `Status(code, Response)`.
  /// Response is boxed so `Error` stays small enough for `Result` in call sites.
  HttpStatus(u16, alloc::boxed::Box<crate::parser::Response>),
  /// Non-HTTPS URL rejected by [`crate::config::Config::https_only`].
  HttpsOnly,
  /// `https://` without TLS, or `assume_tls_socket` with cleartext [`crate::OsBlockingSocket`].
  TlsNotConfigured,
  /// Response header section larger than [`crate::config::Config::max_response_header_size`].
  ResponseHeaderTooLarge,
  /// Body larger than [`crate::config::Config::max_response_body_size`].
  ///
  /// Single public recovery path for body limits (transport and parse/decompress).
  BodyExceedsLimit(usize),
  /// Response body is not valid UTF-8.
  ///
  /// Produced when [`crate::Response::to_text`] (returns [`core::str::Utf8Error`])
  /// or [`IntoStringError`] is converted with `?` / [`From`] into [`Error`]. Prefer
  /// matching the specialized error type when you need the recoverable
  /// [`crate::Response`] from [`crate::Response::into_string`].
  Utf8Error(core::str::Utf8Error),
  /// Bad request construction (see [`InvalidRequest`]).
  InvalidRequest(InvalidRequest),
}

impl Error {
  const fn as_str(&self) -> &'static str {
    match self {
      Self::Parse(_) => "parse error",
      Self::Dns(_) => "DNS error",
      Self::Socket(_) => "socket error",
      Self::InvalidUrl => "invalid URL",
      Self::TooManyRedirects => "too many redirects",
      Self::MissingRedirectLocation => "redirect missing Location header",
      Self::RedirectLoop => "redirect loop detected",
      Self::RedirectFailed => "redirect failed",
      Self::HttpStatus(_, _) => "HTTP status",
      Self::HttpsOnly => "HTTPS-only policy rejected non-HTTPS URL",
      Self::TlsNotConfigured => {
        "TLS not configured: use a TLS-capable BlockingSocket with assume_tls_socket, or http://"
      },
      Self::ResponseHeaderTooLarge => "response headers too large",
      Self::BodyExceedsLimit(_) => "response body exceeds size limit",
      Self::Utf8Error(_) => "invalid UTF-8",
      Self::InvalidRequest(_) => "invalid request",
    }
  }
}

impl core::fmt::Display for Error {
  fn fmt(
    &self,
    f: &mut core::fmt::Formatter<'_>,
  ) -> core::fmt::Result {
    match self {
      Self::Parse(e) => write!(f, "parse error: {e}"),
      Self::Dns(e) => write!(f, "DNS error: {e}"),
      Self::Socket(e) => write!(f, "socket error: {e}"),
      Self::HttpStatus(code, _) => write!(f, "HTTP status {code}"),
      Self::BodyExceedsLimit(limit) => write!(f, "response body exceeds limit of {limit} bytes"),
      Self::Utf8Error(e) => write!(f, "invalid UTF-8: {e}"),
      Self::InvalidRequest(e) => write!(f, "invalid request: {e}"),
      other => f.write_str(other.as_str()),
    }
  }
}

impl core::error::Error for Error {
  fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
    match self {
      Self::Parse(e) => Some(e),
      Self::Dns(e) => Some(e),
      Self::Socket(e) => Some(e),
      Self::Utf8Error(e) => Some(e),
      Self::InvalidRequest(e) => Some(e),
      _ => None,
    }
  }
}

impl From<ParseError> for Error {
  fn from(value: ParseError) -> Self {
    match value {
      // Lift so callers never see Display-identical `Parse(BodyExceedsLimit)`.
      ParseError::BodyExceedsLimit(n) => Self::BodyExceedsLimit(n),
      other => Self::Parse(other),
    }
  }
}

impl From<DnsError> for Error {
  fn from(value: DnsError) -> Self {
    Self::Dns(value)
  }
}

impl From<SocketError> for Error {
  fn from(value: SocketError) -> Self {
    Self::Socket(value)
  }
}

impl From<InvalidRequest> for Error {
  fn from(value: InvalidRequest) -> Self {
    Self::InvalidRequest(value)
  }
}

impl From<core::str::Utf8Error> for Error {
  fn from(value: core::str::Utf8Error) -> Self {
    Self::Utf8Error(value)
  }
}

impl From<IntoStringError> for Error {
  /// Maps to [`Error::Utf8Error`]. The response is dropped; call
  /// [`IntoStringError::into_response`] first if you need it.
  fn from(value: IntoStringError) -> Self {
    Self::Utf8Error(value.utf8_error())
  }
}
