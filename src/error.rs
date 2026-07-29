/// HTTP/1.1 message parse failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
  /// Invalid HTTP version format
  InvalidHttpVersion,
  /// Invalid HTTP status code
  InvalidStatusCode,
  /// Invalid reason phrase in status line
  InvalidReasonPhrase,
  /// Invalid header field name
  InvalidHeaderName,
  /// Invalid header field value
  InvalidHeaderValue,
  /// Invalid URI format
  InvalidUri,
  /// Missing required CRLF (\r\n) sequence
  MissingCrlf,
  /// Bare carriage return without line feed
  BareCarriageReturn,
  /// Unexpected end of input while parsing
  UnexpectedEndOfInput,
  /// Invalid whitespace in message
  InvalidWhitespace,
  /// Invalid chunk size in chunked transfer encoding
  InvalidChunkSize,
  /// Invalid Content-Length header value
  InvalidContentLength,
  /// Both Transfer-Encoding and Content-Length present (RFC 9112 Section 6.3)
  ConflictingFraming,
  /// Transfer-Encoding present but chunked is not the final encoding (RFC 9112 Section 6.3)
  ChunkedNotFinal,
  /// Whitespace found between start-line and first header field (RFC 9112 Section 2.2)
  WhitespaceBeforeHeaders,
  /// Extra data found after complete response body (RFC 9112 Section 6.3)
  ExtraDataAfterResponse,
  /// Host header is required in HTTP/1.1 requests (RFC 9112 Section 3.2)
  MissingHostHeader,
  /// Header value contains obsolete line folding (RFC 9112 Section 5.2)
  ObsoleteFoldInHeader,
  /// Transfer-Encoding in responses that must not have it (RFC 9112 Section 6.1)
  InvalidTransferEncodingForStatus,
  /// TE header contains "chunked" which is forbidden (RFC 9112 Section 7.4)
  ChunkedInTeHeader,
  /// TE header present but Connection header missing "TE" (RFC 9112 Section 7.4)
  TeHeaderMissingConnection,
  /// Multiple Host headers present (RFC 9112 Section 3.2)
  MultipleHostHeaders,
  /// Invalid Host header value format (RFC 9112 Section 3.2)
  InvalidHostHeaderValue,
  /// Transfer-Encoding used with HTTP version < 1.1 (RFC 9112 Section 6.1)
  TransferEncodingRequiresHttp11,
  /// Chunked appears multiple times in Transfer-Encoding (RFC 9112 Section 6.1)
  ChunkedAppliedMultipleTimes,
  /// Failed to decompress response body (gzip/deflate)
  DecompressionFailed,
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
    }
  }
}

impl core::fmt::Display for ParseError {
  fn fmt(
    &self,
    f: &mut core::fmt::Formatter<'_>,
  ) -> core::fmt::Result {
    f.write_str(self.as_str())
  }
}

impl core::error::Error for ParseError {}

/// DNS lookup failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DnsError {
  /// DNS resolution failed with error code
  ResolutionFailed(i32),
  /// No IP addresses found for hostname
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
  /// Socket is not connected
  NotConnected,
  /// Connection refused by remote host
  ConnectionRefused,
  /// Operation timed out
  TimedOut,
  /// Operation was interrupted
  Interrupted,
  /// Invalid socket address
  InvalidAddress,
  /// Operation not supported
  Unsupported,
  /// Operating system error with code
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

/// Error from [`crate::HttpClient`] and the free `get` / `post` helpers.
#[derive(Debug)]
pub enum Error {
  /// HTTP message parsing error
  Parse(ParseError),
  /// DNS resolution error
  Dns(DnsError),
  /// Socket operation error
  Socket(SocketError),
  /// Invalid or malformed URL
  InvalidUrl,
  /// Maximum redirect limit exceeded
  TooManyRedirects,
  /// Redirect response missing Location header
  MissingRedirectLocation,
  /// Circular redirect detected
  RedirectLoop,
  /// HTTP error status code (4xx or 5xx)
  HttpStatus(u16),
  /// HTTPS/TLS not available on this socket (or HTTPS-only policy rejected HTTP)
  HttpsRequired,
  /// Response headers exceed maximum allowed size
  ResponseHeaderTooLarge,
  /// UTF-8 decoding error
  Utf8Error,
  /// Invalid request (illegal cookie, form and body both set, …)
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
      Self::HttpStatus(_) => "HTTP status",
      Self::HttpsRequired => "HTTPS/TLS not available on this socket; use a TLS-capable BlockingSocket or http://",
      Self::ResponseHeaderTooLarge => "response headers too large",
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
      Self::HttpStatus(code) => write!(f, "HTTP status {code}"),
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
