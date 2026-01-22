extern crate alloc;
use crate::body::Body;
use crate::error::ParseError;
use crate::headers::Headers;
use crate::parser::framing::FramingDetector;
use crate::parser::message::{BodyReadStrategy, Response};
use alloc::vec::Vec;

/// Orchestrates incremental HTTP response reading and parsing
///
/// This separates the concerns of:
/// - Network I/O (caller's responsibility)
/// - Framing detection (finding boundaries)
/// - Message parsing (interpreting bytes as HTTP)
#[allow(dead_code)]
pub struct ResponseReader {
  state: ReaderState,
  buffer: Vec<u8>,
  max_header_size: usize,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReaderState {
  ReadingHeaders,
  ReadingBody {
    status_code: u16,
    strategy: BodyReadStrategy,
  },
}

#[allow(dead_code)]
impl ResponseReader {
  /// Create a new response reader with default limits
  pub const fn new() -> Self {
    Self::with_max_header_size(8192)
  }

  /// Create a new response reader with custom header size limit
  ///
  /// RFC 9112 Section 2.1: implementations ought to be careful about
  /// the size of message header fields
  pub const fn with_max_header_size(max_header_size: usize) -> Self {
    Self {
      state: ReaderState::ReadingHeaders,
      buffer: Vec::new(),
      max_header_size,
    }
  }

  /// Add more data to the reader's buffer
  ///
  /// Returns an error if header size limit is exceeded
  pub fn feed(&mut self, data: &[u8]) -> Result<(), ParseError> {
    self.buffer.extend_from_slice(data);

    if matches!(self.state, ReaderState::ReadingHeaders)
      && self.buffer.len() > self.max_header_size
    {
      return Err(ParseError::HeaderTooLarge);
    }

    Ok(())
  }

  /// Check if headers are complete and ready to be parsed
  pub fn has_complete_headers(&self) -> bool {
    matches!(self.state, ReaderState::ReadingHeaders)
      && FramingDetector::has_complete_headers(&self.buffer)
  }

  /// Parse headers and transition to body reading state
  ///
  /// Returns (status_code, reason, headers, body_strategy)
  ///
  /// Must only be called when `has_complete_headers()` returns true
  pub fn parse_headers(
    &mut self,
  ) -> Result<(u16, alloc::string::String, Headers, BodyReadStrategy), ParseError> {
    if !matches!(self.state, ReaderState::ReadingHeaders) {
      return Err(ParseError::InvalidState);
    }

    let (status_code, reason, headers, remaining) =
      Response::parse_headers_only(&self.buffer)?;

    let strategy = Response::body_read_strategy(&headers, status_code);

    // Replace buffer with only the body bytes (clear headers)
    self.buffer = remaining.to_vec();

    self.state = ReaderState::ReadingBody {
      status_code,
      strategy,
    };

    Ok((status_code, reason, headers, strategy))
  }

  /// Check if body reading is complete based on the strategy
  ///
  /// For HEAD requests, this should always return true immediately
  pub fn is_body_complete(&self) -> bool {
    if let ReaderState::ReadingBody { strategy, .. } = self.state {
      match strategy {
        BodyReadStrategy::NoBody => true,
        BodyReadStrategy::ContentLength(expected) => self.buffer.len() >= expected,
        BodyReadStrategy::Chunked => {
          FramingDetector::has_chunked_terminator(&self.buffer)
        }
        // UntilClose requires explicit signal from caller
        BodyReadStrategy::UntilClose => false,
      }
    } else {
      false
    }
  }

  /// Get the bytes needed to complete the body read
  ///
  /// Returns None if reading until close or if body is already complete
  pub const fn bytes_needed(&self) -> Option<usize> {
    if let ReaderState::ReadingBody {
      strategy: BodyReadStrategy::ContentLength(expected),
      ..
    } = self.state
    {
      Some(expected.saturating_sub(self.buffer.len()))
    } else {
      None
    }
  }

  /// Parse the complete response
  ///
  /// Must only be called when body is complete
  pub fn finish(self, headers: &Headers, status_code: u16) -> Result<Body, ParseError> {
    if !matches!(
      self.state,
      ReaderState::ReadingHeaders | ReaderState::ReadingBody { .. }
    ) {
      return Err(ParseError::InvalidState);
    }

    Response::parse_body_from_bytes(&self.buffer, headers, status_code)
  }
}

impl Default for ResponseReader {
  fn default() -> Self {
    Self::new()
  }
}
