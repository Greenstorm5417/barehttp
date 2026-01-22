use crate::error::ParseError;

/// Detects HTTP framing boundaries in byte streams
///
/// RFC 9112 Section 2: Message Format
/// HTTP messages consist of a start-line, header fields, and optional message body.
/// The end of header section is indicated by CRLF CRLF sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FramingDetector;

impl FramingDetector {
  /// Search for the end of HTTP headers (CRLF CRLF sequence)
  ///
  /// RFC 9112 Section 2.1: Message Format
  /// Returns the position after the CRLF CRLF if found, None otherwise
  #[allow(dead_code)]
  pub fn find_header_end(data: &[u8]) -> Option<usize> {
    // RFC 9112 Section 2: header section ends with CRLF CRLF
    data
      .windows(4)
      .position(|w| w == b"\r\n\r\n")
      .map(|pos| pos + 4)
  }

  /// Check if buffer contains complete HTTP headers
  ///
  /// This is more efficient than `find_header_end` when you only need
  /// to know if headers are complete, not where they end.
  #[inline]
  pub fn has_complete_headers(data: &[u8]) -> bool {
    data.windows(4).any(|w| w == b"\r\n\r\n")
  }

  /// Search for the terminating chunk in chunked transfer encoding
  ///
  /// RFC 9112 Section 7.1: Chunked Transfer Coding
  /// The chunked encoding ends with a chunk of size 0 followed by trailer section
  /// Minimal form: "0\r\n\r\n"
  ///
  /// Returns true if the terminating chunk sequence is found
  pub fn has_chunked_terminator(data: &[u8]) -> bool {
    // RFC 9112 Section 7.1: last chunk is "0" followed by CRLF and trailer
    // Most minimal form is "0\r\n\r\n" (5 bytes)
    data.windows(5).any(|w| w == b"0\r\n\r\n")
  }

  /// Parse Content-Length header value
  ///
  /// RFC 9112 Section 6.2: Content-Length
  /// The Content-Length field value consists of one or more digits
  #[allow(dead_code)]
  pub fn parse_content_length(value: &[u8]) -> Result<usize, ParseError> {
    let s = core::str::from_utf8(value).map_err(|_| ParseError::InvalidContentLength)?;
    s.trim()
      .parse()
      .map_err(|_| ParseError::InvalidContentLength)
  }

  /// Extract headers section from response buffer
  ///
  /// Returns the header bytes (including status line, excluding final CRLF CRLF)
  /// and remaining bytes after headers
  #[allow(dead_code)]
  pub fn split_headers(data: &[u8]) -> Result<(&[u8], &[u8]), ParseError> {
    let end_pos = Self::find_header_end(data).ok_or(ParseError::UnexpectedEndOfInput)?;

    // Headers include everything up to (but not including) the final CRLF CRLF
    let headers = data
      .get(..end_pos.saturating_sub(4))
      .ok_or(ParseError::UnexpectedEndOfInput)?;
    let remaining = data
      .get(end_pos..)
      .ok_or(ParseError::UnexpectedEndOfInput)?;

    Ok((headers, remaining))
  }
}
