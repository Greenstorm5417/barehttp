//! Header-section scanner (RFC 9112 §5). Obs-fold is rejected.
//!
//! One pass over the wire bytes produces [`FieldSpan`]s *relative to the
//! scanned input* (no per-field `&[u8]` pairs, no name/value copies). The
//! caller decides framing from those spans while they still point into the
//! receive buffer, then [`pack_fields`] copies only live name/value bytes into
//! one exact-capacity arena.

use crate::error::ParseError;
use crate::headers::{FieldList, FieldSpan, Headers};
use alloc::string::String;
use alloc::vec::Vec;

/// Header fields located in a wire buffer (offsets into the scanned input).
#[derive(Debug)]
pub(crate) struct ScannedFields {
  /// Spans relative to the input passed to [`scan_fields`].
  pub(crate) fields: FieldList,
  /// Sum of name + value lengths (exact packed-arena size for ASCII values).
  pub(crate) live: usize,
  /// Bytes through (and including) the terminating blank line.
  pub(crate) consumed: usize,
}

impl ScannedFields {
  /// `(name, value)` byte views for framing checks before packing.
  #[inline]
  pub(crate) fn pairs<'a>(
    &'a self,
    input: &'a [u8],
  ) -> impl Iterator<Item = (&'a [u8], &'a [u8])> + 'a {
    self
      .fields
      .iter()
      .map(move |s| (s.name_in(input), s.value_in(input)))
  }
}

/// Spans past the inline eight, buffered on the stack before the exact heap list.
const SCAN_STACK: usize = 24;

const ONES: u64 = 0x0101_0101_0101_0101;
const HIGHS: u64 = 0x8080_8080_8080_8080;

/// First byte that is a CTL (`< 0x20`) or DEL.
///
/// SWAR: the lowest flagged lane is exact (borrows only propagate upward);
/// obs-text lanes (`>= 0x80`) never flag because `!w` clears their top bit.
#[inline]
fn find_ctl(hay: &[u8]) -> Option<usize> {
  let (chunks, remainder) = hay.as_chunks::<8>();
  let mut off = 0usize;
  for chunk in chunks {
    let w = u64::from_le_bytes(*chunk);
    let lt = w.wrapping_sub(0x20 * ONES) & !w & HIGHS;
    let del = w ^ (0x7F * ONES);
    let mask = lt | (del.wrapping_sub(ONES) & !del & HIGHS);
    if mask != 0 {
      return Some(off + (mask.trailing_zeros() >> 3) as usize);
    }
    off += 8;
  }
  remainder
    .iter()
    .position(|&b| b < 0x20 || b == 0x7F)
    .map(|i| off + i)
}

/// Single-pass header field scan; obs-fold is rejected (`ObsoleteFoldInHeader`).
///
/// Slice-pattern driven (no per-byte bounds checks); offsets are recovered
/// from `input.len() - rest.len()`.
///
/// # Errors
/// Malformed fields, obs-fold, or whitespace before the first field.
pub(crate) fn scan_fields(input: &[u8]) -> Result<ScannedFields, ParseError> {
  // RFC 9112 Section 2.2: reject whitespace between start-line and first header
  if let [b' ' | b'\t', ..] = input {
    return Err(ParseError::WhitespaceBeforeHeaders);
  }

  let total = input.len();
  // First 8 spans go straight into the final inline list; beyond that, a stack
  // buffer collects them so the heap list is allocated once at its exact size
  // (no spill + regrow churn).
  let mut fields = FieldList::new();
  let mut stack = [const { core::mem::MaybeUninit::<FieldSpan>::uninit() }; SCAN_STACK];
  let mut n = 0usize;
  let mut overflow: Vec<FieldSpan> = Vec::new();
  let mut live = 0usize;
  let mut rest = input;

  loop {
    match rest {
      [b'\r', b'\n', tail @ ..] | [b'\n', tail @ ..] => {
        rest = tail;
        break;
      },
      _ => {},
    }

    // field-name = token, terminated by ':' (token charset excludes SP/HTAB).
    let name_start = total - rest.len();
    let name_len = rest
      .iter()
      .position(|&b| !is_token_char(b))
      .unwrap_or(rest.len());
    rest = match rest.get(name_len..) {
      Some([b':', tail @ ..]) if name_len > 0 => tail,
      _ => return Err(ParseError::InvalidHeaderName),
    };

    while let [b' ' | b'\t', tail @ ..] = rest {
      rest = tail;
    }
    let value_start = total - rest.len();

    // RFC 9110 field-content: HTAB / SP / VCHAR / obs-text. One SWAR pass finds
    // the line end and rejects NUL / VT / other CTLs / DEL on the way.
    let mut scan = 0usize;
    let line_len = loop {
      let Some(rel) = find_ctl(rest.get(scan..).unwrap_or(&[])) else {
        // No terminator in the buffer.
        return Err(ParseError::MissingCrlf);
      };
      let i = scan + rel;
      match rest.get(i) {
        Some(b'\t') => scan = i + 1,
        Some(b'\r' | b'\n') => break i,
        _ => return Err(ParseError::InvalidHeaderValue),
      }
    };
    // Only SP / HTAB can trail here (other CTLs were rejected above).
    let value_len = rest.get(..line_len).unwrap_or(&[]).trim_ascii_end().len();

    rest = match rest.get(line_len..) {
      Some([b'\r', b'\n', tail @ ..] | [b'\n', tail @ ..]) => tail,
      _ => return Err(ParseError::MissingCrlf),
    };
    // Reject obs-fold (CRLF/LF + SP/HTAB).
    if let [b' ' | b'\t', ..] = rest {
      return Err(ParseError::ObsoleteFoldInHeader);
    }

    live += name_len + value_len;
    let span = FieldSpan::from_offsets(
      usize_as_u32(name_start),
      usize_as_u32(name_len),
      usize_as_u32(value_start),
      usize_as_u32(value_len),
    );
    if fields.try_push_inline(span) {
      continue;
    }
    if let Some(slot) = stack.get_mut(n) {
      slot.write(span);
      n += 1;
    } else {
      overflow.push(span);
    }
  }

  // SAFETY: exactly `stack[..n]` was initialized by `slot.write` above
  // (`n <= SCAN_STACK`), and `FieldSpan` is `Copy` (no drop obligations).
  let spilled = unsafe { core::slice::from_raw_parts(stack.as_ptr().cast::<FieldSpan>(), n) };
  if n > 0 {
    fields.spill_exact(spilled, &overflow);
  }
  Ok(ScannedFields {
    fields,
    live,
    consumed: total - rest.len(),
  })
}

/// Copy live name/value bytes of `scanned` (spans into `input`) into one
/// exact-capacity arena and rebase the spans. Empty sections allocate nothing.
#[must_use]
pub(crate) fn pack_fields(
  input: &[u8],
  scanned: ScannedFields,
) -> Headers {
  pack_fields_with_tail(input, scanned, 0)
}

/// Like [`pack_fields`], reserving `tail` extra bytes after the fields so the
/// caller can store a response body in the same allocation.
#[must_use]
pub(crate) fn pack_fields_with_tail(
  input: &[u8],
  scanned: ScannedFields,
  tail: usize,
) -> Headers {
  let ScannedFields { mut fields, live, .. } = scanned;
  if fields.is_empty() && tail == 0 {
    return Headers::new();
  }
  let mut buf: Vec<u8> = Vec::with_capacity(live.saturating_add(tail));
  let mut lossy_seen = false;
  for span in &mut fields {
    let name = span.name_in(input);
    let value = span.value_in(input);
    span.name_start = usize_as_u32(buf.len());
    buf.extend_from_slice(name);
    span.value_start = usize_as_u32(buf.len());
    if value.is_ascii() {
      buf.extend_from_slice(value);
    } else {
      lossy_seen = true;
      // Obs-text → lossy UTF-8 (the arena only holds UTF-8).
      let lossy = String::from_utf8_lossy(value);
      buf.extend_from_slice(lossy.as_bytes());
      span.value_len = usize_as_u32(lossy.len());
    }
  }
  if lossy_seen {
    buf.shrink_to_fit();
  }
  Headers::from_spans(buf, fields)
}

/// One-pass scan + pack (buffered trailers / tests).
///
/// Empty sections (blank line only — common chunked trailers) skip the arena.
///
/// # Errors
/// Malformed fields, obs-fold, or whitespace before the first field.
pub(crate) fn parse_header_fields(input: &[u8]) -> Result<(Headers, &[u8]), ParseError> {
  // Empty trailer / blank header block (the usual chunked ending).
  if let [b'\r', b'\n', rest @ ..] | [b'\n', rest @ ..] = input {
    return Ok((Headers::new(), rest));
  }
  let scanned = scan_fields(input)?;
  let rest = input.get(scanned.consumed..).unwrap_or(&[]);
  Ok((pack_fields(input, scanned), rest))
}

#[inline]
fn usize_as_u32(n: usize) -> u32 {
  u32::try_from(n).unwrap_or(u32::MAX)
}

#[allow(clippy::indexing_slicing)] // 256-entry table fill; indices are token bytes
const fn token_char_lut() -> [u8; 256] {
  let mut t = [0u8; 256];
  t[b'!' as usize] = 1;
  t[b'#' as usize] = 1;
  t[b'$' as usize] = 1;
  t[b'%' as usize] = 1;
  t[b'&' as usize] = 1;
  t[b'\'' as usize] = 1;
  t[b'*' as usize] = 1;
  t[b'+' as usize] = 1;
  t[b'-' as usize] = 1;
  t[b'.' as usize] = 1;
  let mut c = b'0';
  while c <= b'9' {
    t[c as usize] = 1;
    c += 1;
  }
  c = b'A';
  while c <= b'Z' {
    t[c as usize] = 1;
    c += 1;
  }
  t[b'^' as usize] = 1;
  t[b'_' as usize] = 1;
  t[b'`' as usize] = 1;
  c = b'a';
  while c <= b'z' {
    t[c as usize] = 1;
    c += 1;
  }
  t[b'|' as usize] = 1;
  t[b'~' as usize] = 1;
  t
}

const TOKEN_CHAR: [u8; 256] = token_char_lut();

/// RFC 9110 token character (header names).
#[inline]
#[allow(clippy::indexing_slicing)] // `u8` index into a 256-entry table
pub const fn is_token_char(b: u8) -> bool {
  TOKEN_CHAR[b as usize] != 0
}

/// Consume a CRLF (or lone LF). Bare CR is an error.
pub fn expect_crlf(input: &[u8]) -> Result<&[u8], ParseError> {
  match (input.first().copied(), input.get(1).copied()) {
    (Some(b'\r'), Some(b'\n')) => Ok(input.get(2..).unwrap_or(&[])),
    (Some(b'\n'), _) => Ok(input.get(1..).unwrap_or(&[])),
    // Lone CR at end of buffer needs another read (not a framing error yet).
    (Some(b'\r'), Some(_)) => Err(ParseError::BareCarriageReturn),
    _ => Err(ParseError::MissingCrlf),
  }
}

#[cfg(test)]
mod tests {
  #![allow(clippy::unwrap_used, clippy::expect_used)]
  use super::*;

  #[test]
  fn pack_copies_only_live_bytes() {
    let section = b"Host: example.com\r\nX-A: 1\r\n\r\nBODY";
    let scanned = scan_fields(section).unwrap();
    assert_eq!(scanned.consumed, section.len() - 4);
    let live = scanned.live;
    let headers = pack_fields(section, scanned);
    assert_eq!(headers.get("host"), Some("example.com"));
    assert_eq!(headers.get("x-a"), Some("1"));
    assert_eq!(headers.arena_len(), live);
    assert_eq!(live, "Host".len() + "example.com".len() + "X-A".len() + "1".len());
  }

  #[test]
  fn obs_text_is_lossy() {
    let section = b"X-Bin: \xff\xfe\r\n\r\n";
    let scanned = scan_fields(section).unwrap();
    let copied = pack_fields(section, scanned);
    assert_eq!(copied.get("x-bin"), Some("\u{fffd}\u{fffd}"));
  }

  #[test]
  fn trims_ows() {
    let section = b"Host:   example.com \t \r\n\r\n";
    let (headers, rest) = parse_header_fields(section).unwrap();
    assert_eq!(rest, b"");
    assert_eq!(headers.get("host"), Some("example.com"));
  }

  #[test]
  fn tab_inside_value_is_kept() {
    let (headers, _) = parse_header_fields(b"X: a\tb\r\n\r\n").unwrap();
    assert_eq!(headers.get("x"), Some("a\tb"));
  }

  #[test]
  fn long_value_swar_lanes() {
    // Exercise every SWAR lane position for the terminator, obs-text, and CTLs.
    for pad in 0..24 {
      let mk = |tail: &[u8]| {
        let mut v = alloc::vec::Vec::new();
        v.extend_from_slice(b"X: ");
        v.extend(core::iter::repeat_n(b'a', pad));
        v.extend_from_slice(tail);
        v
      };
      let hi = mk(b"\xc3\xa9\r\n\r\n");
      let (h, _) = parse_header_fields(&hi).unwrap();
      assert_eq!(h.get("x").unwrap().len(), pad + 2, "pad {pad}");
      let lo = scan_fields(&mk(b"\r\nY: 1\r\n\r\n")).unwrap();
      assert_eq!(lo.fields.len(), 2, "pad {pad}");
      assert_eq!(lo.live, 1 + pad + 1 + 1);
      assert_eq!(
        scan_fields(&mk(b"\x0b\r\n\r\n")).unwrap_err(),
        ParseError::InvalidHeaderValue
      );
      assert_eq!(
        scan_fields(&mk(b"\x7f\r\n\r\n")).unwrap_err(),
        ParseError::InvalidHeaderValue
      );
      assert_eq!(
        scan_fields(&mk(b"\x00\r\n\r\n")).unwrap_err(),
        ParseError::InvalidHeaderValue
      );
      assert_eq!(scan_fields(&mk(b" \t \r\n\r\n")).unwrap().live, 1 + pad);
    }
  }

  #[test]
  fn error_variants_match_rfc_rules() {
    assert_eq!(scan_fields(b"").unwrap_err(), ParseError::InvalidHeaderName);
    assert_eq!(
      scan_fields(b" X: 1\r\n\r\n").unwrap_err(),
      ParseError::WhitespaceBeforeHeaders
    );
    assert_eq!(
      scan_fields(b"X : 1\r\n\r\n").unwrap_err(),
      ParseError::InvalidHeaderName
    );
    assert_eq!(scan_fields(b": 1\r\n\r\n").unwrap_err(), ParseError::InvalidHeaderName);
    assert_eq!(
      scan_fields(b"X: 1\r\n folded\r\n\r\n").unwrap_err(),
      ParseError::ObsoleteFoldInHeader
    );
    assert_eq!(scan_fields(b"X: 1\rY: 2\r\n\r\n").unwrap_err(), ParseError::MissingCrlf);
    assert_eq!(scan_fields(b"X: 1").unwrap_err(), ParseError::MissingCrlf);
    let lf = scan_fields(b"X: 1\nY:2\n\nrest").unwrap();
    assert_eq!(lf.fields.len(), 2);
    assert_eq!(lf.consumed, 10);
  }
}

#[cfg(test)]
mod scan_capacity_tests {
  #![allow(clippy::unwrap_used)]
  use super::*;

  #[test]
  fn many_fields_cross_stack_buffer() {
    for count in [0usize, 1, 8, 9, 31, 32, 33, 100] {
      let mut wire = alloc::vec::Vec::new();
      for i in 0..count {
        wire.extend_from_slice(alloc::format!("X-{i}: v{i}\r\n").as_bytes());
      }
      wire.extend_from_slice(b"\r\n");
      let (h, rest) = parse_header_fields(&wire).unwrap();
      assert_eq!(rest, b"");
      assert_eq!(h.len(), count);
      for i in 0..count {
        assert_eq!(h.get(&alloc::format!("x-{i}")), Some(alloc::format!("v{i}").as_str()));
      }
    }
  }
}
