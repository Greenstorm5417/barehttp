use crate::error::ParseError;
use crate::parser::headers::{expect_crlf, parse_header_fields};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkedDecoder {
  state: DecodeState,
  trailers: alloc::vec::Vec<(alloc::vec::Vec<u8>, alloc::vec::Vec<u8>)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DecodeState {
  ChunkSize,
  ChunkData(usize),
  ChunkDataCrlf,
  TrailerSection,
  Complete,
}

impl ChunkedDecoder {
  pub const fn new() -> Self {
    Self {
      state: DecodeState::ChunkSize,
      trailers: alloc::vec::Vec::new(),
    }
  }

  /// Trailer fields after the last chunk (RFC 9112 §7.1.2).
  #[must_use]
  #[allow(dead_code)]
  pub fn trailers(&self) -> &[(alloc::vec::Vec<u8>, alloc::vec::Vec<u8>)] {
    &self.trailers
  }

  /// Take trailer fields out of the decoder (avoids cloning after a successful decode).
  #[must_use]
  pub fn take_trailers(&mut self) -> alloc::vec::Vec<(alloc::vec::Vec<u8>, alloc::vec::Vec<u8>)> {
    core::mem::take(&mut self.trailers)
  }

  /// `Ok(true)` if `input` contains a complete chunked message; `Ok(false)` if more
  /// bytes are needed; `Err` on malformed framing.
  #[allow(dead_code)]
  pub fn message_complete(input: &[u8]) -> Result<bool, ParseError> {
    Ok(Self::message_len_if_complete(input)?.is_some())
  }

  /// Bytes consumed by a complete chunked message, or `None` if more input is needed.
  ///
  /// # Errors
  /// [`ParseError`] on illegal framing.
  pub fn message_len_if_complete(input: &[u8]) -> Result<Option<usize>, ParseError> {
    let mut decoder = Self::new();
    // Discard decoded payload; callers that need the body decode separately.
    let mut output = alloc::vec::Vec::new();
    match decoder.decode_chunk(input, &mut output) {
      Ok(rest) => Ok(Some(input.len().saturating_sub(rest.len()))),
      Err(ParseError::UnexpectedEndOfInput | ParseError::MissingCrlf) => Ok(None),
      Err(e) => Err(e),
    }
  }

  /// Bytes consumed by a complete chunked message in `input` (excludes trailing data).
  ///
  /// # Errors
  /// [`ParseError`] when the message is incomplete or the framing is illegal.
  #[allow(dead_code)]
  pub fn message_len(input: &[u8]) -> Result<usize, ParseError> {
    Self::message_len_if_complete(input)?.ok_or(ParseError::UnexpectedEndOfInput)
  }

  pub fn decode_chunk<'a>(
    &'a mut self,
    input: &'a [u8],
    output: &mut alloc::vec::Vec<u8>,
  ) -> Result<&'a [u8], ParseError> {
    let mut remaining = input;

    loop {
      match self.state {
        DecodeState::ChunkSize => {
          if remaining.is_empty() {
            return Err(ParseError::UnexpectedEndOfInput);
          }
          let (size, rest) = Self::parse_chunk_size(remaining)?;
          remaining = rest;

          if size == 0 {
            self.state = DecodeState::TrailerSection;
          } else {
            self.state = DecodeState::ChunkData(size);
          }
        },
        DecodeState::ChunkData(size) => {
          if remaining.len() < size {
            return Err(ParseError::UnexpectedEndOfInput);
          }

          let data = remaining
            .get(..size)
            .ok_or(ParseError::UnexpectedEndOfInput)?;
          output.extend_from_slice(data);

          remaining = remaining
            .get(size..)
            .ok_or(ParseError::UnexpectedEndOfInput)?;
          self.state = DecodeState::ChunkDataCrlf;
        },
        DecodeState::ChunkDataCrlf => {
          remaining = expect_crlf(remaining)?;
          self.state = DecodeState::ChunkSize;
        },
        DecodeState::TrailerSection => {
          if !trailer_section_looks_complete(remaining) {
            return Err(ParseError::UnexpectedEndOfInput);
          }
          let (trailers, rest) = parse_header_fields(remaining)?;
          self.trailers = trailers;
          self.state = DecodeState::Complete;
          return Ok(rest);
        },
        DecodeState::Complete => {
          return Ok(remaining);
        },
      }
    }
  }

  fn parse_chunk_size(input: &[u8]) -> Result<(usize, &[u8]), ParseError> {
    let mut i = 0;
    let mut size = 0usize;

    while i < input.len() {
      let b = *input.get(i).ok_or(ParseError::InvalidChunkSize)?;

      if b == b';' || b == b'\r' || b == b'\n' {
        break;
      }

      let digit = if b.is_ascii_digit() {
        b - b'0'
      } else if (b'a'..=b'f').contains(&b) {
        b - b'a' + 10
      } else if (b'A'..=b'F').contains(&b) {
        b - b'A' + 10
      } else {
        return Err(ParseError::InvalidChunkSize);
      };

      size = size.checked_mul(16).ok_or(ParseError::InvalidChunkSize)?;
      size = size
        .checked_add(digit as usize)
        .ok_or(ParseError::InvalidChunkSize)?;
      i += 1;
    }

    if i == 0 {
      return Err(ParseError::InvalidChunkSize);
    }

    // Hex digits without a size-line terminator yet.
    if i == input.len() {
      return Err(ParseError::UnexpectedEndOfInput);
    }

    let mut rest = input.get(i..).ok_or(ParseError::InvalidChunkSize)?;

    while !rest.is_empty() {
      let b = *rest.first().ok_or(ParseError::InvalidChunkSize)?;
      if b == b'\r' || b == b'\n' {
        break;
      }
      rest = rest.get(1..).ok_or(ParseError::InvalidChunkSize)?;
    }

    if rest.is_empty() {
      return Err(ParseError::UnexpectedEndOfInput);
    }

    rest = expect_crlf(rest)?;

    Ok((size, rest))
  }
}

/// True when `data` contains a blank line ending the trailer section.
fn trailer_section_looks_complete(data: &[u8]) -> bool {
  if data.is_empty() {
    return false;
  }
  if data == b"\r\n" || data == b"\n" {
    return true;
  }
  data.windows(4).any(|w| w == b"\r\n\r\n") || data.windows(2).any(|w| w == b"\n\n")
}
