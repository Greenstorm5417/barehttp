/// DNS resolution errors
pub mod dns;
/// HTTP-specific errors
pub mod http;
/// HTTP message parsing errors
pub mod parse;
/// Socket operation errors
pub mod socket;

pub use dns::DnsError;
pub use parse::ParseError;
pub use socket::SocketError;

/// Main error type for HTTP operations
///
/// Encompasses all possible errors that can occur during HTTP requests,
/// including parsing, DNS resolution, socket operations, and protocol errors.
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
  /// IP addresses are not supported in this context
  IpAddressNotSupported,
  /// Maximum redirect limit exceeded
  TooManyRedirects,
  /// Redirect response missing Location header
  MissingRedirectLocation,
  /// Invalid or malformed redirect location
  InvalidRedirectLocation,
  /// Circular redirect detected
  RedirectLoop,
  /// HTTP error status code (4xx or 5xx)
  HttpStatus(u16),
  /// HTTPS required but HTTP URL provided
  HttpsRequired,
  /// Response headers exceed maximum allowed size
  ResponseHeaderTooLarge,
  /// UTF-8 decoding error
  Utf8Error,
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
