use crate::error::ParseError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkedDecoder {
  state: DecodeState,
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
    }
  }

  pub fn decode_chunk<'a>(
    &mut self,
    input: &'a [u8],
    output: &mut alloc::vec::Vec<u8>,
  ) -> Result<&'a [u8], ParseError> {
    let mut remaining = input;

    loop {
      match self.state {
        DecodeState::ChunkSize => {
          let (size, rest) = Self::parse_chunk_size(remaining)?;
          remaining = rest;

          if size == 0 {
            self.state = DecodeState::TrailerSection;
          } else {
            self.state = DecodeState::ChunkData(size);
          }
        }
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
        }
        DecodeState::ChunkDataCrlf => {
          remaining = Self::expect_crlf(remaining)?;
          self.state = DecodeState::ChunkSize;
        }
        DecodeState::TrailerSection => {
          let (found_end, rest) = Self::parse_trailer_section(remaining)?;
          remaining = rest;

          if found_end {
            self.state = DecodeState::Complete;
            return Ok(remaining);
          }
        }
        DecodeState::Complete => {
          return Ok(remaining);
        }
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

    let mut rest = input.get(i..).ok_or(ParseError::InvalidChunkSize)?;

    while !rest.is_empty() {
      let b = *rest.first().ok_or(ParseError::InvalidChunkSize)?;
      if b == b'\r' || b == b'\n' {
        break;
      }
      rest = rest.get(1..).ok_or(ParseError::InvalidChunkSize)?;
    }

    rest = Self::expect_crlf(rest)?;

    Ok((size, rest))
  }

  fn parse_trailer_section(input: &[u8]) -> Result<(bool, &[u8]), ParseError> {
    if input.len() >= 2 {
      let byte0 = input.first().copied();
      let byte1 = input.get(1).copied();

      if byte0 == Some(b'\r') && byte1 == Some(b'\n') {
        let rest = input.get(2..).ok_or(ParseError::MissingCrlf)?;
        return Ok((true, rest));
      }
    }

    if !input.is_empty() && input.first().copied() == Some(b'\n') {
      let rest = input.get(1..).ok_or(ParseError::MissingCrlf)?;
      return Ok((true, rest));
    }

    let mut i = 0;
    while i < input.len() {
      let b = input.get(i).copied();
      if b == Some(b'\r') || b == Some(b'\n') {
        break;
      }
      i += 1;
    }

    if i == 0 {
      return Err(ParseError::UnexpectedEndOfInput);
    }

    let after_line = input.get(i..).ok_or(ParseError::UnexpectedEndOfInput)?;
    let final_rest = Self::expect_crlf(after_line)?;

    Ok((false, final_rest))
  }

  fn expect_crlf(input: &[u8]) -> Result<&[u8], ParseError> {
    if input.len() < 2 {
      return Err(ParseError::MissingCrlf);
    }

    let byte0 = input.first().copied();
    let byte1 = input.get(1).copied();

    if byte0 == Some(b'\r') && byte1 == Some(b'\n') {
      return input.get(2..).ok_or(ParseError::MissingCrlf);
    }

    if byte0 == Some(b'\n') {
      return input.get(1..).ok_or(ParseError::MissingCrlf);
    }

    if byte0 == Some(b'\r') {
      return Err(ParseError::BareCarriageReturn);
    }

    Err(ParseError::MissingCrlf)
  }
}
