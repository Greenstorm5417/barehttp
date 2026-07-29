/// HTTP/1.1 message parse failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
  /// gzip / deflate / zstd decompression failed.
  DecompressionFailed,
  /// Decompressed body larger than the configured size limit.
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
      Self::DecompressionFailed => "failed to decompress response body",
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
      other => f.write_str(other.as_str()),
    }
  }
}

impl core::error::Error for ParseError {}

/// DNS lookup failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// Error from [`crate::HttpClient`] and the free `get` / `post` / … functions.
#[derive(Debug)]
pub enum Error {
  /// Wire / framing parse failure.
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
  HttpStatus(u16, crate::parser::Response),
  /// Non-HTTPS URL rejected by [`crate::config::Config::https_only`].
  HttpsOnly,
  /// `https://` without TLS, or `assume_tls_socket` with cleartext [`crate::OsBlockingSocket`].
  TlsNotConfigured,
  /// Response header section larger than [`crate::config::Config::max_response_header_size`].
  ResponseHeaderTooLarge,
  /// Body larger than [`crate::config::Config::max_response_body_size`].
  BodyExceedsLimit(usize),
  /// Response body is not valid UTF-8.
  Utf8Error,
  /// Bad request construction (illegal cookie octets, form fields plus an explicit body, …).
  InvalidRequest,
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
      Self::Utf8Error => "invalid UTF-8",
      Self::InvalidRequest => "invalid request",
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
      _ => None,
    }
  }
}

impl From<alloc::string::FromUtf8Error> for Error {
  fn from(_: alloc::string::FromUtf8Error) -> Self {
    Self::Utf8Error
  }
}
