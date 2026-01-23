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
  /// Header value contains bare CR without LF (RFC 9112 Section 2.2)
  BareCarriageReturnInHeader,
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
  /// Request-target URI exceeds maximum allowed length (RFC 9112 Section 3)
  UriTooLong,
  /// Transfer-Encoding used with HTTP version < 1.1 (RFC 9112 Section 6.1)
  TransferEncodingRequiresHttp11,
  /// Chunked appears multiple times in Transfer-Encoding (RFC 9112 Section 6.1)
  ChunkedAppliedMultipleTimes,
}

impl ParseError {
  /// Returns true if this error represents an unrecoverable framing error
  /// that requires the connection to be closed per RFC 9112 Section 6.3.
  ///
  /// These errors indicate potential request smuggling attacks or ambiguous
  /// message framing that makes it unsafe to reuse the connection.
  #[must_use]
  pub const fn requires_connection_closure(self) -> bool {
    matches!(
      self,
      Self::ConflictingFraming
        | Self::ChunkedNotFinal
        | Self::InvalidContentLength
        | Self::UnexpectedEndOfInput
        | Self::ExtraDataAfterResponse
        | Self::WhitespaceBeforeHeaders
        | Self::InvalidTransferEncodingForStatus
    )
  }
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
      Self::ConflictingFraming => {
        write!(f, "both Transfer-Encoding and Content-Length present")
      }
      Self::ChunkedNotFinal => write!(f, "chunked must be the final Transfer-Encoding"),
      Self::WhitespaceBeforeHeaders => {
        write!(f, "whitespace found between start-line and first header")
      }
      Self::ExtraDataAfterResponse => {
        write!(f, "extra data found after complete response")
      }
      Self::MissingHostHeader => write!(f, "Host header required for HTTP/1.1 requests"),
      Self::BareCarriageReturnInHeader => {
        write!(f, "header value contains bare CR (not allowed)")
      }
      Self::ObsoleteFoldInHeader => {
        write!(f, "header value contains obs-fold (not allowed)")
      }
      Self::InvalidTransferEncodingForStatus => {
        write!(f, "Transfer-Encoding not allowed for this status code")
      }
      Self::ChunkedInTeHeader => write!(f, "TE header must not contain 'chunked'"),
      Self::TeHeaderMissingConnection => {
        write!(f, "TE header requires 'TE' in Connection header")
      }
      Self::MultipleHostHeaders => write!(f, "multiple Host headers present"),
      Self::InvalidHostHeaderValue => write!(f, "invalid Host header value format"),
      Self::UriTooLong => write!(f, "request-target URI exceeds maximum allowed length"),
      Self::TransferEncodingRequiresHttp11 => {
        write!(f, "Transfer-Encoding requires HTTP/1.1 or higher")
      }
      Self::ChunkedAppliedMultipleTimes => {
        write!(f, "chunked transfer coding applied multiple times")
      }
    }
  }
}
