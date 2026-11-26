use crate::error::ParseError;
use crate::headers::Headers;
use crate::parser::headers::{expect_crlf, parse_header_fields};
use alloc::vec::Vec;
use bytes::Bytes;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkedDecoder {
  state: DecodeState,
  trailers: Headers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DecodeState {
  ChunkSize,
  /// `left` payload bytes still needed for the current chunk.
  ChunkData(usize),
  ChunkDataCrlf,
  TrailerSection,
  Complete,
}

/// One framing step of [`ChunkedDecoder::step`] (offsets relative to its input).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
  /// Payload bytes at `input[start..start + len]`; resume at `start + len`.
  Data { start: usize, len: usize },
  /// Need more input; the first `consumed` bytes are fully processed.
  NeedMore { consumed: usize },
  /// Message complete; the message ends at `input[..end]`.
  Done { end: usize },
}

/// Outcome of [`ChunkedDecoder::feed`].
#[derive(Debug, PartialEq, Eq)]
pub enum FeedResult<'a> {
  /// Need more input. `consumed` bytes at the front of this feed are fully processed
  /// and must not be presented again.
  NeedMore { consumed: usize },
  /// Message complete. `rest` is any bytes after the chunked message.
  Done { rest: &'a [u8] },
}

impl ChunkedDecoder {
  pub const fn new() -> Self {
    Self {
      state: DecodeState::ChunkSize,
      trailers: Headers::new(),
    }
  }

  /// Trailer fields after the last chunk (RFC 9112 §7.1.2).
  #[must_use]
  #[allow(dead_code)]
  pub const fn trailers(&self) -> &Headers {
    &self.trailers
  }

  /// Move trailer fields out of the decoder after a successful decode.
  #[must_use]
  pub fn take_trailers(&mut self) -> Headers {
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
  /// Framing-only (`feed` with no output buffer): does not allocate or copy chunk payload.
  ///
  /// # Errors
  /// [`ParseError`] on illegal framing.
  pub fn message_len_if_complete(input: &[u8]) -> Result<Option<usize>, ParseError> {
    let mut decoder = Self::new();
    match decoder.feed(input, None)? {
      FeedResult::Done { rest } => Ok(Some(input.len().saturating_sub(rest.len()))),
      FeedResult::NeedMore { .. } => Ok(None),
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

  /// Incremental feed: processes `input` from the start, never re-scans already-consumed
  /// prefix across calls when the caller advances by `NeedMore.consumed`.
  ///
  /// When `output` is `Some`, chunk payload is appended; when `None`, framing is
  /// validated only (payload skipped).
  ///
  /// # Errors
  /// [`ParseError`] on illegal framing.
  pub fn feed<'a>(
    &mut self,
    input: &'a [u8],
    mut output: Option<&mut alloc::vec::Vec<u8>>,
  ) -> Result<FeedResult<'a>, ParseError> {
    let mut base = 0usize;
    loop {
      match self.step(input.get(base..).unwrap_or(&[]))? {
        Step::Data { start, len } => {
          let at = base.saturating_add(start);
          if let Some(out) = output.as_deref_mut() {
            out.extend_from_slice(input.get(at..at.saturating_add(len)).unwrap_or(&[]));
          }
          base = at.saturating_add(len);
        },
        Step::NeedMore { consumed } => {
          return Ok(FeedResult::NeedMore {
            consumed: base.saturating_add(consumed),
          });
        },
        Step::Done { end } => {
          return Ok(FeedResult::Done {
            rest: input.get(base.saturating_add(end)..).unwrap_or(&[]),
          });
        },
      }
    }
  }

  /// Payload bytes still owed by the current chunk (0 outside chunk data).
  #[must_use]
  pub(crate) const fn pending_data(&self) -> usize {
    match self.state {
      DecodeState::ChunkData(left) => left,
      _ => 0,
    }
  }

  /// Advance framing over `input` until payload, more input, or completion.
  ///
  /// Never copies: payload is reported as an offset range so callers can move
  /// it wherever they like (e.g. compact in place over consumed framing).
  ///
  /// # Errors
  /// [`ParseError`] on illegal framing.
  pub(crate) fn step(
    &mut self,
    input: &[u8],
  ) -> Result<Step, ParseError> {
    let mut pos = 0usize;
    loop {
      let remaining = input.get(pos..).unwrap_or(&[]);
      match self.state {
        DecodeState::ChunkSize => {
          if remaining.is_empty() {
            return Ok(Step::NeedMore { consumed: pos });
          }
          match Self::parse_chunk_size(remaining) {
            Ok((size, rest)) => {
              pos = input.len().saturating_sub(rest.len());
              self.state = if size == 0 {
                DecodeState::TrailerSection
              } else {
                DecodeState::ChunkData(size)
              };
            },
            Err(ParseError::UnexpectedEndOfInput | ParseError::MissingCrlf) => {
              return Ok(Step::NeedMore { consumed: pos });
            },
            Err(e) => return Err(e),
          }
        },
        DecodeState::ChunkData(left) => {
          if remaining.is_empty() {
            return Ok(Step::NeedMore { consumed: pos });
          }
          let take = left.min(remaining.len());
          let next = left.saturating_sub(take);
          self.state = if next > 0 {
            DecodeState::ChunkData(next)
          } else {
            DecodeState::ChunkDataCrlf
          };
          return Ok(Step::Data { start: pos, len: take });
        },
        DecodeState::ChunkDataCrlf => match expect_crlf(remaining) {
          Ok(rest) => {
            pos = input.len().saturating_sub(rest.len());
            self.state = DecodeState::ChunkSize;
          },
          Err(ParseError::MissingCrlf) => {
            return Ok(Step::NeedMore { consumed: pos });
          },
          Err(e) => return Err(e),
        },
        DecodeState::TrailerSection => {
          // The whole trailer section (through its blank line) must be present
          // before it is parsed — trailers are not streamed field-by-field.
          if !trailer_section_looks_complete(remaining) {
            return Ok(Step::NeedMore { consumed: pos });
          }
          let (trailers, rest) = parse_header_fields(remaining)?;
          self.trailers = trailers;
          self.state = DecodeState::Complete;
          return Ok(Step::Done {
            end: input.len().saturating_sub(rest.len()),
          });
        },
        DecodeState::Complete => return Ok(Step::Done { end: pos }),
      }
    }
  }

  /// Decode a complete chunked message from `input` into `output`.
  ///
  /// Prefer [`Self::decode_buffered`] for owned wire buffers (contiguous reuse).
  /// Incremental recv uses [`Self::feed`].
  ///
  /// # Errors
  /// [`ParseError`] when the message is incomplete or framing is illegal.
  #[allow(dead_code)] // exercised in unit tests; buffered parse uses `decode_buffered`
  pub fn decode_chunk<'a>(
    &'a mut self,
    input: &'a [u8],
    output: &mut alloc::vec::Vec<u8>,
  ) -> Result<&'a [u8], ParseError> {
    match self.feed(input, Some(output))? {
      FeedResult::Done { rest } => Ok(rest),
      FeedResult::NeedMore { .. } => Err(ParseError::UnexpectedEndOfInput),
    }
  }

  /// Decode a complete buffered chunked body from borrowed wire bytes.
  ///
  /// Two framing walks (no copies, no span list): count the payload, then copy
  /// it into one exact-capacity buffer. Trailers are parsed once.
  ///
  /// # Errors
  /// [`ParseError`] on incomplete or illegal framing, or bytes after the message.
  pub fn decode_buffered(input: &[u8]) -> Result<(Bytes, Headers), ParseError> {
    let (payload_len, trailer_at) = Self::walk_buffered(input, |_, _| {})?;
    let trailer_section = input
      .get(trailer_at..)
      .ok_or(ParseError::UnexpectedEndOfInput)?;
    let (trailers, rest) = parse_header_fields(trailer_section)?;
    if !rest.is_empty() {
      return Err(ParseError::ExtraDataAfterResponse);
    }
    if payload_len == 0 {
      return Ok((Bytes::new(), trailers));
    }
    let mut out = Vec::with_capacity(payload_len);
    Self::walk_buffered(input, |start, end| {
      out.extend_from_slice(input.get(start..end).unwrap_or(&[]));
    })?;
    debug_assert_eq!(out.len(), payload_len);
    Ok((Bytes::from(out), trailers))
  }

  /// Payload length of a complete buffered chunked message (framing walk only).
  ///
  /// # Errors
  /// [`ParseError`] on incomplete or illegal framing.
  pub(crate) fn buffered_payload_len(input: &[u8]) -> Result<usize, ParseError> {
    Ok(Self::walk_buffered(input, |_, _| {})?.0)
  }

  /// Decode a complete buffered chunked message, appending the payload to `out`
  /// (capacity reserved by the caller). Returns the trailers.
  ///
  /// # Errors
  /// [`ParseError`] on incomplete or illegal framing, or bytes after the message.
  pub(crate) fn decode_buffered_into(
    input: &[u8],
    out: &mut Vec<u8>,
  ) -> Result<Headers, ParseError> {
    let (_, trailer_at) = Self::walk_buffered(input, |start, end| {
      out.extend_from_slice(input.get(start..end).unwrap_or(&[]));
    })?;
    let (trailers, rest) = parse_header_fields(input.get(trailer_at..).unwrap_or(&[]))?;
    if !rest.is_empty() {
      return Err(ParseError::ExtraDataAfterResponse);
    }
    Ok(trailers)
  }

  /// Walk chunk framing once; `on_span(start, end)` per data chunk. Returns the
  /// payload length and the trailer-section offset.
  fn walk_buffered<F: FnMut(usize, usize)>(
    input: &[u8],
    mut on_span: F,
  ) -> Result<(usize, usize), ParseError> {
    let mut remaining = input;
    let mut payload_len = 0usize;
    loop {
      let (size, after_size) = match Self::parse_chunk_size(remaining) {
        Ok(v) => v,
        Err(ParseError::UnexpectedEndOfInput | ParseError::MissingCrlf) => {
          return Err(ParseError::UnexpectedEndOfInput);
        },
        Err(e) => return Err(e),
      };

      if size == 0 {
        if !trailer_section_looks_complete(after_size) {
          return Err(ParseError::UnexpectedEndOfInput);
        }
        return Ok((payload_len, input.len().saturating_sub(after_size.len())));
      }

      let after_payload = after_size
        .get(size..)
        .ok_or(ParseError::UnexpectedEndOfInput)?;
      let start = input.len().saturating_sub(after_size.len());
      match expect_crlf(after_payload) {
        Ok(rest) => remaining = rest,
        Err(ParseError::MissingCrlf) => return Err(ParseError::UnexpectedEndOfInput),
        Err(e) => return Err(e),
      }
      payload_len = payload_len
        .checked_add(size)
        .ok_or(ParseError::InvalidChunkSize)?;
      on_span(start, start.saturating_add(size));
    }
  }

  /// Parse a chunk-size hex line (pub(crate) for Kani / internal tests).
  pub(crate) fn parse_chunk_size(input: &[u8]) -> Result<(usize, &[u8]), ParseError> {
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
  // Single scan for `\r\n\r\n` or `\n\n`.
  let mut i = 0usize;
  while i < data.len() {
    match data.get(i).copied() {
      Some(b'\r')
        if data.get(i.saturating_add(1)).copied() == Some(b'\n')
          && data.get(i.saturating_add(2)).copied() == Some(b'\r')
          && data.get(i.saturating_add(3)).copied() == Some(b'\n') =>
      {
        return true;
      },
      Some(b'\n') if data.get(i.saturating_add(1)).copied() == Some(b'\n') => {
        return true;
      },
      _ => {},
    }
    i = i.saturating_add(1);
  }
  false
}

#[cfg(test)]
mod tests {
  #![allow(clippy::unwrap_used, clippy::expect_used, clippy::shadow_reuse, clippy::panic)]

  use super::{ChunkedDecoder, FeedResult, Step};
  use crate::error::ParseError;
  use alloc::vec::Vec;

  #[test]
  fn feed_byte_at_a_time_decodes() {
    let wire = b"5\r\nHello\r\n6\r\n World\r\n0\r\n\r\n";
    let mut decoder = ChunkedDecoder::new();
    let mut out = Vec::new();
    let mut pending = Vec::new();
    for &b in wire {
      pending.push(b);
      match decoder.feed(&pending, Some(&mut out)).unwrap() {
        FeedResult::NeedMore { consumed } => {
          pending.drain(..consumed);
        },
        FeedResult::Done { rest } => {
          assert_eq!(rest, []);
          assert_eq!(out, b"Hello World");
          return;
        },
      }
    }
    panic!("expected Done");
  }

  #[test]
  fn feed_consumed_sum_equals_message_len() {
    // Each wire byte reported as consumed exactly once across incremental feeds.
    let payload = alloc::vec![b'x'; 128];
    let mut wire = Vec::new();
    wire.extend_from_slice(b"80\r\n");
    wire.extend_from_slice(&payload);
    wire.extend_from_slice(b"\r\n0\r\n\r\n");

    let mut decoder = ChunkedDecoder::new();
    let mut offset = 0usize;
    let mut consumed_sum = 0usize;
    for end in 1..=wire.len() {
      match decoder
        .feed(wire.get(offset..end).unwrap_or(&[]), None)
        .unwrap()
      {
        FeedResult::NeedMore { consumed } => {
          consumed_sum = consumed_sum.saturating_add(consumed);
          offset += consumed;
        },
        FeedResult::Done { rest } => {
          let framed = end.saturating_sub(offset).saturating_sub(rest.len());
          consumed_sum = consumed_sum.saturating_add(framed);
          offset += framed;
          break;
        },
      }
    }
    assert_eq!(offset, wire.len());
    assert_eq!(consumed_sum, wire.len());
  }

  #[test]
  fn feed_advances_past_payload_without_output() {
    // Growing buffer + offset cursor: already-validated bytes are never re-fed.
    let wire = b"5\r\nHello\r\n0\r\n\r\n";
    let mut decoder = ChunkedDecoder::new();
    let mut buf = Vec::new();
    let mut offset = 0usize;
    for &b in wire {
      buf.push(b);
      match decoder
        .feed(buf.get(offset..).unwrap_or(&[]), None)
        .unwrap()
      {
        FeedResult::Done { rest } => {
          assert_eq!(rest, []);
          assert_eq!(offset + (buf.len() - offset), wire.len());
          return;
        },
        FeedResult::NeedMore { consumed } => {
          offset += consumed;
        },
      }
    }
    panic!("expected Done");
  }

  #[test]
  fn message_len_matches_feed() {
    let wire = b"A\r\n0123456789\r\n0\r\nX-T: v\r\n\r\n";
    assert_eq!(ChunkedDecoder::message_len_if_complete(wire).unwrap(), Some(wire.len()));
    assert_eq!(
      ChunkedDecoder::message_len_if_complete(wire.get(..wire.len() - 1).unwrap()).unwrap(),
      None
    );
  }

  #[test]
  fn decode_buffered_single_chunk_exact_alloc() {
    let (body, trailers) = ChunkedDecoder::decode_buffered(b"5\r\nHello\r\n0\r\n\r\n").unwrap();
    assert_eq!(body.as_ref(), b"Hello");
    assert!(trailers.is_empty());
  }

  #[test]
  fn step_reports_spans_without_copying() {
    let wire = b"3\r\nabc\r\n2\r\nde\r\n0\r\n\r\n";
    let mut d = ChunkedDecoder::new();
    let mut pos = 0usize;
    let mut got = Vec::new();
    loop {
      match d.step(wire.get(pos..).unwrap()).unwrap() {
        Step::Data { start, len } => {
          got.extend_from_slice(wire.get(pos + start..pos + start + len).unwrap());
          pos += start + len;
        },
        Step::NeedMore { .. } => panic!("complete input"),
        Step::Done { end } => {
          assert_eq!(pos + end, wire.len());
          break;
        },
      }
    }
    assert_eq!(got, b"abcde");
  }

  #[test]
  fn decode_buffered_multi_chunk_exact_concat() {
    let wire = b"5\r\nHello\r\n6\r\n World\r\n0\r\nX-T: v\r\n\r\n";
    let (body, trailers) = ChunkedDecoder::decode_buffered(wire).unwrap();
    assert_eq!(body.as_ref(), b"Hello World");
    assert_eq!(trailers.get("x-t"), Some("v"));
  }

  #[test]
  fn decode_buffered_matches_decode_chunk() {
    let wire = b"5\r\nHello\r\n6\r\n World\r\n0\r\nX-T: v\r\n\r\n";
    let (body, trailers) = ChunkedDecoder::decode_buffered(wire).unwrap();
    let mut decoder = ChunkedDecoder::new();
    let mut out = Vec::new();
    let rest = decoder.decode_chunk(wire, &mut out).unwrap();
    assert_eq!(rest, []);
    assert_eq!(body.as_ref(), out.as_slice());
    assert_eq!(trailers.get("x-t"), decoder.take_trailers().get("x-t"));
  }

  #[test]
  fn decode_buffered_empty_and_rejects_extra() {
    let (body, _) = ChunkedDecoder::decode_buffered(b"0\r\n\r\n").unwrap();
    assert!(body.is_empty());

    // Extra framed data after the terminating blank line (same shape as RFC smuggling cases).
    let extra = b"0\r\n\r\n5\r\nHello\r\n0\r\n\r\n";
    assert!(matches!(
      ChunkedDecoder::decode_buffered(extra),
      Err(ParseError::ExtraDataAfterResponse)
    ));
  }
}

#[cfg(kani)]
mod kani_chunk_proofs {
  use super::ChunkedDecoder;
  use crate::error::ParseError;

  #[kani::proof]
  fn empty_chunk_size_line_errs() {
    assert!(matches!(
      ChunkedDecoder::parse_chunk_size(b""),
      Err(ParseError::InvalidChunkSize | ParseError::UnexpectedEndOfInput)
    ));
  }

  #[kani::proof]
  fn single_hex_digit_parses() {
    let (size, rest) = ChunkedDecoder::parse_chunk_size(b"a\r\n").unwrap();
    assert_eq!(size, 10);
    assert!(rest.is_empty());
  }

  /// Bounded hex length: overflow via checked_mul returns InvalidChunkSize (no panic).
  #[kani::proof]
  // Concrete 34-byte line; unwind must cover both digit + trailer loops.
  #[kani::unwind(64)]
  fn long_hex_does_not_panic() {
    // 32 hex digits — may overflow usize on 64-bit; must Err, not panic.
    let input = b"ffffffffffffffffffffffffffffffff\r\n";
    let _ = ChunkedDecoder::parse_chunk_size(input);
  }

  /// Panic-freedom on a fixed 6-byte `HHHH\r\n` line (4 symbolic hex digits).
  ///
  /// Explicit unwind: without it, CBMC can unwind `parse_chunk_size`'s loops
  /// hundreds of times on a 6-byte buffer (observed 700+ iters / 19+ min in GHA).
  #[kani::proof]
  #[kani::unwind(16)]
  fn symbolic_short_hex() {
    let mut digits = [0u8; 4];
    for d in &mut digits {
      *d = kani::any();
      kani::assume(d.is_ascii_hexdigit());
    }
    let mut line = [0u8; 6];
    line[..4].copy_from_slice(&digits);
    line[4] = b'\r';
    line[5] = b'\n';
    let _ = ChunkedDecoder::parse_chunk_size(&line);
  }
}
