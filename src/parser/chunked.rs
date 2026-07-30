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

/// Framing layout of a complete buffered chunked message (no payload copy).
struct BufferedLayout {
  /// Decoded payload length (sum of chunk sizes).
  payload_len: usize,
  /// When the whole payload is one contiguous span in the wire buffer.
  contiguous: Option<(usize, usize)>,
  /// Non-contiguous payload spans (empty when `contiguous` is `Some` or body empty).
  ranges: Vec<(usize, usize)>,
  /// Byte offset of the trailer section (after the final `0…\r\n` size line).
  trailer_at: usize,
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
    let mut remaining = input;

    loop {
      match self.state {
        DecodeState::ChunkSize => {
          if remaining.is_empty() {
            return Ok(FeedResult::NeedMore {
              consumed: input.len().saturating_sub(remaining.len()),
            });
          }
          match Self::parse_chunk_size(remaining) {
            Ok((size, rest)) => {
              remaining = rest;
              self.state = if size == 0 {
                DecodeState::TrailerSection
              } else {
                // Cap reserve so a forged huge chunk size cannot OOM before framing checks.
                if let Some(out) = output.as_deref_mut() {
                  const RESERVE_CAP: usize = 64 * 1024;
                  out.reserve(size.min(RESERVE_CAP));
                }
                DecodeState::ChunkData(size)
              };
            },
            Err(ParseError::UnexpectedEndOfInput | ParseError::MissingCrlf) => {
              return Ok(FeedResult::NeedMore {
                consumed: input.len().saturating_sub(remaining.len()),
              });
            },
            Err(e) => return Err(e),
          }
        },
        DecodeState::ChunkData(left) => {
          if remaining.is_empty() {
            return Ok(FeedResult::NeedMore {
              consumed: input.len().saturating_sub(remaining.len()),
            });
          }
          let take = left.min(remaining.len());
          let data = remaining
            .get(..take)
            .ok_or(ParseError::UnexpectedEndOfInput)?;
          if let Some(out) = output.as_deref_mut() {
            out.extend_from_slice(data);
          }
          remaining = remaining
            .get(take..)
            .ok_or(ParseError::UnexpectedEndOfInput)?;
          let next = left.saturating_sub(take);
          if next > 0 {
            self.state = DecodeState::ChunkData(next);
            return Ok(FeedResult::NeedMore {
              consumed: input.len().saturating_sub(remaining.len()),
            });
          }
          self.state = DecodeState::ChunkDataCrlf;
        },
        DecodeState::ChunkDataCrlf => match expect_crlf(remaining) {
          Ok(rest) => {
            remaining = rest;
            self.state = DecodeState::ChunkSize;
          },
          Err(ParseError::MissingCrlf) => {
            return Ok(FeedResult::NeedMore {
              consumed: input.len().saturating_sub(remaining.len()),
            });
          },
          Err(e) => return Err(e),
        },
        DecodeState::TrailerSection => {
          // Incomplete edge: the whole trailer section (through the terminating blank
          // line) must be present before `parse_header_fields` runs — trailers are not
          // streamed field-by-field. Payload chunks above are single-pass.
          if !trailer_section_looks_complete(remaining) {
            return Ok(FeedResult::NeedMore {
              consumed: input.len().saturating_sub(remaining.len()),
            });
          }
          let (trailers, rest) = parse_header_fields(remaining)?;
          self.trailers = trailers;
          self.state = DecodeState::Complete;
          return Ok(FeedResult::Done { rest });
        },
        DecodeState::Complete => {
          return Ok(FeedResult::Done { rest: remaining });
        },
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

  /// Decode a complete buffered chunked body from owned wire bytes.
  ///
  /// Single-chunk (and empty) payloads reuse `input` via [`Bytes::slice`] — no
  /// body-sized `Vec`. Multi-chunk copies once into an exact-capacity buffer.
  /// Trailers are parsed in the same framing walk.
  ///
  /// # Errors
  /// [`ParseError`] on incomplete or illegal framing, or bytes after the message.
  #[allow(clippy::needless_pass_by_value)] // owned `Bytes` so contiguous payloads can `slice`
  pub fn decode_buffered(input: Bytes) -> Result<(Bytes, Headers), ParseError> {
    let layout = Self::buffered_layout(input.as_ref())?;
    let trailer_section = input
      .get(layout.trailer_at..)
      .ok_or(ParseError::UnexpectedEndOfInput)?;
    let (trailers, rest) = parse_header_fields(trailer_section)?;
    if !rest.is_empty() {
      return Err(ParseError::ExtraDataAfterResponse);
    }

    let body = if layout.payload_len == 0 {
      Bytes::new()
    } else if let Some((start, end)) = layout.contiguous {
      input.slice(start..end)
    } else {
      let mut out = Vec::with_capacity(layout.payload_len);
      for (start, end) in layout.ranges {
        let span = input
          .get(start..end)
          .ok_or(ParseError::UnexpectedEndOfInput)?;
        out.extend_from_slice(span);
      }
      debug_assert_eq!(out.len(), layout.payload_len);
      Bytes::from(out)
    };
    Ok((body, trailers))
  }

  /// Walk chunk framing once: sizes, payload spans, trailer offset (no copy).
  fn buffered_layout(input: &[u8]) -> Result<BufferedLayout, ParseError> {
    let mut remaining = input;
    let mut payload_len = 0usize;
    let mut contiguous: Option<(usize, usize)> = None;
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut multi = false;

    loop {
      let (size, after_size) = match Self::parse_chunk_size(remaining) {
        Ok(v) => v,
        Err(ParseError::UnexpectedEndOfInput | ParseError::MissingCrlf) => {
          return Err(ParseError::UnexpectedEndOfInput);
        },
        Err(e) => return Err(e),
      };

      if size == 0 {
        let trailer_at = input.len().saturating_sub(after_size.len());
        if !trailer_section_looks_complete(after_size) {
          return Err(ParseError::UnexpectedEndOfInput);
        }
        return Ok(BufferedLayout {
          payload_len,
          contiguous: if multi {
            None
          } else {
            contiguous
          },
          ranges: if multi {
            ranges
          } else {
            Vec::new()
          },
          trailer_at,
        });
      }

      if after_size.len() < size {
        return Err(ParseError::UnexpectedEndOfInput);
      }
      let after_payload = after_size
        .get(size..)
        .ok_or(ParseError::UnexpectedEndOfInput)?;
      let start = input.len().saturating_sub(after_size.len());
      let end = start.saturating_add(size);

      match expect_crlf(after_payload) {
        Ok(rest) => remaining = rest,
        Err(ParseError::MissingCrlf) => return Err(ParseError::UnexpectedEndOfInput),
        Err(e) => return Err(e),
      }

      payload_len = payload_len
        .checked_add(size)
        .ok_or(ParseError::InvalidChunkSize)?;

      if multi {
        ranges.push((start, end));
      } else if contiguous.is_none() {
        contiguous = Some((start, end));
      } else {
        // Second data chunk: promote first span into `ranges` and continue scattered.
        multi = true;
        if let Some((s, e)) = contiguous.take() {
          ranges.push((s, e));
        }
        ranges.push((start, end));
      }
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

  use super::{ChunkedDecoder, FeedResult};
  use crate::error::ParseError;
  use alloc::vec::Vec;
  use bytes::Bytes;

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
          assert!(rest.is_empty());
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
          assert!(rest.is_empty());
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
  fn decode_buffered_single_chunk_reuses_bytes_storage() {
    let wire = Bytes::from(Vec::from(&b"5\r\nHello\r\n0\r\n\r\n"[..]));
    let parent = wire.clone();
    let (body, trailers) = ChunkedDecoder::decode_buffered(wire).unwrap();
    assert_eq!(body.as_ref(), b"Hello");
    assert!(trailers.is_empty());
    // Contiguous payload → `Bytes::slice` into the same allocation (no Vec rebuild).
    let p0 = parent.as_ptr() as usize;
    let p1 = p0.saturating_add(parent.len());
    let c0 = body.as_ptr() as usize;
    let c1 = c0.saturating_add(body.len());
    assert!(c0 >= p0 && c1 <= p1);
  }

  #[test]
  fn decode_buffered_multi_chunk_exact_concat() {
    let wire = Bytes::from(Vec::from(&b"5\r\nHello\r\n6\r\n World\r\n0\r\nX-T: v\r\n\r\n"[..]));
    let (body, trailers) = ChunkedDecoder::decode_buffered(wire).unwrap();
    assert_eq!(body.as_ref(), b"Hello World");
    assert_eq!(trailers.get("x-t"), Some("v"));
  }

  #[test]
  fn decode_buffered_matches_decode_chunk() {
    let wire = b"5\r\nHello\r\n6\r\n World\r\n0\r\nX-T: v\r\n\r\n";
    let (body, trailers) = ChunkedDecoder::decode_buffered(Bytes::copy_from_slice(wire)).unwrap();
    let mut decoder = ChunkedDecoder::new();
    let mut out = Vec::new();
    let rest = decoder.decode_chunk(wire, &mut out).unwrap();
    assert!(rest.is_empty());
    assert_eq!(body.as_ref(), out.as_slice());
    assert_eq!(trailers.get("x-t"), decoder.take_trailers().get("x-t"));
  }

  #[test]
  fn decode_buffered_empty_and_rejects_extra() {
    let empty = Bytes::from(Vec::from(&b"0\r\n\r\n"[..]));
    let (body, _) = ChunkedDecoder::decode_buffered(empty).unwrap();
    assert!(body.is_empty());

    // Extra framed data after the terminating blank line (same shape as RFC smuggling cases).
    let extra = Bytes::from(Vec::from(&b"0\r\n\r\n5\r\nHello\r\n0\r\n\r\n"[..]));
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
