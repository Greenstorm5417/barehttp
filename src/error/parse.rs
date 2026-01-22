/// Errors that can occur while parsing HTTP messages
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
  /// Invalid HTTP version format
  InvalidHttpVersion,
  /// Invalid HTTP method
  InvalidMethod,
  /// Invalid request target/path
  InvalidRequestTarget,
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
  /// Line exceeds maximum allowed length
  LineTooLong,
  /// Invalid chunk size in chunked transfer encoding
  InvalidChunkSize,
  /// Invalid Content-Length header value
  InvalidContentLength,
  /// Response header section exceeds size limit
  HeaderTooLarge,
  /// Invalid state transition in response reader
  InvalidState,
}

impl core::fmt::Display for ParseError {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    match self {
      Self::InvalidHttpVersion => write!(f, "invalid HTTP version"),
      Self::InvalidMethod => write!(f, "invalid method"),
      Self::InvalidRequestTarget => write!(f, "invalid request target"),
      Self::InvalidStatusCode => write!(f, "invalid status code"),
      Self::InvalidReasonPhrase => write!(f, "invalid reason phrase"),
      Self::InvalidHeaderName => write!(f, "invalid header name"),
      Self::InvalidHeaderValue => write!(f, "invalid header value"),
      Self::InvalidUri => write!(f, "invalid URI"),
      Self::MissingCrlf => write!(f, "missing CRLF"),
      Self::BareCarriageReturn => write!(f, "bare CR not allowed"),
      Self::UnexpectedEndOfInput => write!(f, "unexpected end of input"),
      Self::InvalidWhitespace => write!(f, "invalid whitespace"),
      Self::LineTooLong => write!(f, "line too long"),
      Self::InvalidChunkSize => write!(f, "invalid chunk size"),
      Self::InvalidContentLength => write!(f, "invalid Content-Length value"),
      Self::HeaderTooLarge => write!(f, "response header too large"),
      Self::InvalidState => write!(f, "invalid parser state"),
    }
  }
}
