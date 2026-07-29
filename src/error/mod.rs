/// DNS resolution errors
pub mod dns;
/// HTTP message parsing errors
pub mod parse;
/// Socket operation errors
pub mod socket;

pub use dns::DnsError;
pub use parse::ParseError;
pub use socket::SocketError;

/// Main error type for HTTP operations.
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
  /// DNS resolution returned no addresses
  NoAddresses,
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
      Self::InvalidUrl => write!(f, "invalid URL"),
      Self::NoAddresses => write!(f, "no addresses resolved"),
      Self::TooManyRedirects => write!(f, "too many redirects"),
      Self::MissingRedirectLocation => write!(f, "redirect missing Location header"),
      Self::RedirectLoop => write!(f, "redirect loop detected"),
      Self::HttpStatus(code) => write!(f, "HTTP status {code}"),
      Self::HttpsRequired => write!(
        f,
        "HTTPS/TLS not available on this socket; use a TLS-capable BlockingSocket or http://"
      ),
      Self::ResponseHeaderTooLarge => write!(f, "response headers too large"),
      Self::Utf8Error => write!(f, "invalid UTF-8"),
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

impl From<ParseError> for Error {
  fn from(e: ParseError) -> Self {
    Self::Parse(e)
  }
}

impl From<DnsError> for Error {
  fn from(e: DnsError) -> Self {
    Self::Dns(e)
  }
}

impl From<SocketError> for Error {
  fn from(e: SocketError) -> Self {
    Self::Socket(e)
  }
}

impl From<alloc::string::FromUtf8Error> for Error {
  fn from(_: alloc::string::FromUtf8Error) -> Self {
    Self::Utf8Error
  }
}
