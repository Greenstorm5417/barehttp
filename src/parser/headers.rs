//! Header-section scanner (RFC 9112 §5). Obs-fold is rejected.
//!
//! Field names and values are `&[u8]` views into the input until
//! [`materialize_headers`], [`parse_header_fields`], or (on the connection path)
//! [`try_wire_spans`] + [`Headers::from_spans`] adopts a frozen wire section.

use crate::error::ParseError;
use crate::headers::{FieldList, FieldSpan, Headers};
use crate::parser::{find_byte, find_cr_or_lf};
use alloc::string::String;
use alloc::vec::Vec;
use bytes::BytesMut;

/// Borrowed header field (views into the input buffer).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeaderRef<'a> {
  /// Field name bytes (token charset; not necessarily lowercase).
  pub name: &'a [u8],
  /// Field value bytes (OWS already trimmed).
  pub value: &'a [u8],
}

/// Scan header fields as borrowed views; obs-fold is rejected (`ObsoleteFoldInHeader`).
///
/// Returns header refs and the remainder after the blank line. No name/value
/// `String` allocations.
///
/// # Errors
/// Malformed fields, obs-fold, or whitespace before the first field.
#[cfg_attr(not(test), allow(dead_code))]
pub fn scan_header_fields(input: &[u8]) -> Result<(Vec<HeaderRef<'_>>, &[u8]), ParseError> {
  let mut headers = Vec::with_capacity(8);
  let remaining = for_each_header_field(input, |h| {
    headers.push(h);
    Ok(())
  })?;
  Ok((headers, remaining))
}

/// Promote borrowed header refs into owned [`Headers`].
///
/// Copies all name/value bytes into one [`Bytes`] arena (offsets per field).
/// Leaves the side-index unset; [`Headers`] builds it lazily on mutation past
/// the index threshold (lookups stay linear until then).
///
/// Prefer [`try_wire_spans`] on the connection path when the receive buffer can
/// be frozen and adopted without copying (ASCII values).
#[must_use]
pub fn materialize_headers(refs: &[HeaderRef<'_>]) -> Headers {
  let mut byte_cap = 0usize;
  for h in refs {
    byte_cap = byte_cap
      .saturating_add(h.name.len())
      .saturating_add(h.value.len());
  }
  let mut buf = BytesMut::with_capacity(byte_cap);
  let mut spans = FieldList::with_capacity(refs.len());
  for &h in refs {
    spans.push(push_span(&mut buf, h));
  }
  Headers::from_spans(buf.freeze(), spans)
}

/// Map borrowed [`HeaderRef`]s to arena offsets into `section` (zero-copy).
///
/// `refs` must be subslices of `section`. Returns [`None`] when any value is
/// non-ASCII (obs-text needs lossy UTF-8 via [`materialize_headers`]) or when a
/// ref is not contained in `section`.
///
/// Side-index stays deferred (same as [`materialize_headers`]).
#[must_use]
pub(crate) fn try_wire_spans(
  section: &[u8],
  refs: &[HeaderRef<'_>],
) -> Option<FieldList> {
  // Names are token charset (ASCII). Values with obs-text are not valid UTF-8;
  // the Headers arena requires UTF-8 for `str` views, so fall back to copy+lossy.
  if refs.iter().any(|h| !h.value.is_ascii()) {
    return None;
  }

  let base = section.as_ptr() as usize;
  let section_len = section.len();
  let mut spans = FieldList::with_capacity(refs.len());

  for h in refs {
    let name_start = subslice_offset(base, section_len, h.name)?;
    let value_start = subslice_offset(base, section_len, h.value)?;
    spans.push(FieldSpan::from_offsets(
      usize_as_u32(name_start),
      usize_as_u32(h.name.len()),
      usize_as_u32(value_start),
      usize_as_u32(h.value.len()),
    ));
  }

  Some(spans)
}

/// Byte offset of `slice` within a parent buffer starting at `base` with `parent_len`.
#[inline]
fn subslice_offset(
  base: usize,
  parent_len: usize,
  slice: &[u8],
) -> Option<usize> {
  let start = slice.as_ptr() as usize;
  if start < base {
    return None;
  }
  let offset = start.saturating_sub(base);
  if offset.saturating_add(slice.len()) > parent_len {
    return None;
  }
  Some(offset)
}

/// One-pass scan + materialize (buffered `Response::parse` / trailers).
///
/// Builds owned [`Headers`] without an intermediate [`HeaderRef`] `Vec`.
/// Side-index is deferred (same as [`materialize_headers`]).
/// Empty sections (blank line only — common chunked trailers) skip the arena.
///
/// # Errors
/// Malformed fields, obs-fold, or whitespace before the first field.
pub fn parse_header_fields(input: &[u8]) -> Result<(Headers, &[u8]), ParseError> {
  // Empty trailer / blank header block: no `BytesMut` arena.
  if input.starts_with(b"\r\n") {
    return Ok((Headers::new(), input.get(2..).unwrap_or(&[])));
  }
  if input.starts_with(b"\n") {
    return Ok((Headers::new(), input.get(1..).unwrap_or(&[])));
  }

  // Start small — typical responses are far under 256 B of name+value bytes.
  // `BytesMut::with_capacity(256)` over-allocated on every parse of a few fields.
  let mut buf = BytesMut::with_capacity(64);
  let mut spans = FieldList::new();
  let remaining = for_each_header_field(input, |h| {
    spans.push(push_span(&mut buf, h));
    Ok(())
  })?;
  if spans.is_empty() {
    return Ok((Headers::new(), remaining));
  }
  Ok((Headers::from_spans(buf.freeze(), spans), remaining))
}

#[inline]
fn usize_as_u32(n: usize) -> u32 {
  u32::try_from(n).unwrap_or(u32::MAX)
}

#[inline]
fn push_span(
  buf: &mut BytesMut,
  h: HeaderRef<'_>,
) -> FieldSpan {
  let name_start = usize_as_u32(buf.len());
  // Token charset was validated in [`for_each_header_field`] → ASCII UTF-8.
  buf.extend_from_slice(h.name);
  let name_len = usize_as_u32(h.name.len());

  let value_start = usize_as_u32(buf.len());
  let value_len = if h.value.is_ascii() {
    buf.extend_from_slice(h.value);
    usize_as_u32(h.value.len())
  } else {
    // Obs-text → lossy UTF-8 (same policy as the former CompactString path).
    let lossy = String::from_utf8_lossy(h.value);
    buf.extend_from_slice(lossy.as_bytes());
    usize_as_u32(lossy.len())
  };
  FieldSpan::from_offsets(name_start, name_len, value_start, value_len)
}

/// Shared field-line scanner. `on_field` runs once per header before the blank line.
pub(crate) fn for_each_header_field<'a, F>(
  input: &'a [u8],
  mut on_field: F,
) -> Result<&'a [u8], ParseError>
where
  F: FnMut(HeaderRef<'a>) -> Result<(), ParseError>,
{
  let mut remaining = input;

  // RFC 9112 Section 2.2: reject whitespace between start-line and first header
  if !remaining.is_empty() {
    let first_byte = remaining.first().copied();
    if first_byte == Some(b' ') || first_byte == Some(b'\t') {
      return Err(ParseError::WhitespaceBeforeHeaders);
    }
  }

  loop {
    let byte0 = remaining.first().copied();
    let byte1 = remaining.get(1).copied();

    if remaining.len() >= 2 && byte0 == Some(b'\r') && byte1 == Some(b'\n') {
      remaining = remaining.get(2..).ok_or(ParseError::MissingCrlf)?;
      break;
    }

    if !remaining.is_empty() && byte0 == Some(b'\n') {
      remaining = remaining.get(1..).ok_or(ParseError::MissingCrlf)?;
      break;
    }

    let Some(colon_pos) = find_byte(remaining, b':') else {
      return Err(ParseError::InvalidHeaderName);
    };

    if colon_pos == 0 {
      return Err(ParseError::InvalidHeaderName);
    }

    let name_bytes = remaining
      .get(..colon_pos)
      .ok_or(ParseError::InvalidHeaderName)?;

    // Token charset excludes SP/HTAB; one pass covers whitespace + RFC 9110 token.
    for &b in name_bytes {
      if !is_token_char(b) {
        return Err(ParseError::InvalidHeaderName);
      }
    }

    remaining = remaining
      .get(colon_pos + 1..)
      .ok_or(ParseError::InvalidHeaderValue)?;

    while !remaining.is_empty() {
      let first_byte = remaining.first().copied();
      if first_byte == Some(b' ') || first_byte == Some(b'\t') {
        remaining = remaining.get(1..).ok_or(ParseError::InvalidHeaderValue)?;
      } else {
        break;
      }
    }

    let line_end = find_cr_or_lf(remaining).unwrap_or(remaining.len());

    let mut value_slice = remaining
      .get(..line_end)
      .ok_or(ParseError::InvalidHeaderValue)?;
    while let Some((&last, rest)) = value_slice.split_last() {
      if last == b' ' || last == b'\t' {
        value_slice = rest;
      } else {
        break;
      }
    }

    // RFC 9110 field-content: HTAB / SP / VCHAR / obs-text only.
    // Reject NUL, CR, LF, VT, and other CTLs (incl. DEL) rather than replace.
    for &b in value_slice {
      if !is_field_vchar_or_ws(b) {
        return Err(ParseError::InvalidHeaderValue);
      }
    }

    remaining = remaining.get(line_end..).ok_or(ParseError::MissingCrlf)?;

    let next_byte0 = remaining.first().copied();
    let next_byte1 = remaining.get(1).copied();
    let next_byte2 = remaining.get(2).copied();

    // Reject obs-fold (CRLF/LF + SP/HTAB)
    if remaining.len() >= 3
      && next_byte0 == Some(b'\r')
      && next_byte1 == Some(b'\n')
      && (next_byte2 == Some(b' ') || next_byte2 == Some(b'\t'))
    {
      return Err(ParseError::ObsoleteFoldInHeader);
    }
    if remaining.len() >= 2 && next_byte0 == Some(b'\n') && (next_byte1 == Some(b' ') || next_byte1 == Some(b'\t')) {
      return Err(ParseError::ObsoleteFoldInHeader);
    }

    if remaining.len() >= 2 && next_byte0 == Some(b'\r') && next_byte1 == Some(b'\n') {
      remaining = remaining.get(2..).ok_or(ParseError::MissingCrlf)?;
    } else if !remaining.is_empty() && next_byte0 == Some(b'\n') {
      remaining = remaining.get(1..).ok_or(ParseError::MissingCrlf)?;
    } else {
      return Err(ParseError::MissingCrlf);
    }

    on_field(HeaderRef {
      name: name_bytes,
      value: value_slice,
    })?;
  }

  Ok(remaining)
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

/// RFC 9110 field-vchar / obs-text, plus HTAB and SP (field-content).
#[inline]
const fn is_field_vchar_or_ws(b: u8) -> bool {
  b == 0x09 || (b >= 0x20 && b != 0x7F)
}

/// Consume a CRLF (or lone LF). Bare CR is an error.
pub fn expect_crlf(input: &[u8]) -> Result<&[u8], ParseError> {
  if input.is_empty() {
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

  // Lone CR at end of buffer: need another read (not a framing error yet).
  if byte0 == Some(b'\r') && byte1.is_none() {
    return Err(ParseError::MissingCrlf);
  }

  if byte0 == Some(b'\r') {
    return Err(ParseError::BareCarriageReturn);
  }

  Err(ParseError::MissingCrlf)
}

#[cfg(test)]
mod tests {
  #![allow(clippy::unwrap_used, clippy::expect_used)]
  use super::*;
  use bytes::Bytes;

  #[test]
  fn try_wire_spans_maps_subslices() {
    let section = b"Host: example.com\r\nX-A: 1\r\n\r\n";
    let (refs, rest) = scan_header_fields(section).unwrap();
    assert!(rest.is_empty());
    let spans = try_wire_spans(section, &refs).expect("ascii");
    let headers = Headers::from_spans(Bytes::copy_from_slice(section), spans);
    assert_eq!(headers.get("host"), Some("example.com"));
    assert_eq!(headers.get("x-a"), Some("1"));
    // Wire section is larger than packed name+value bytes (CRLF / colon / OWS dead).
    assert!(headers.arena_len() > "Host".len() + "example.com".len() + "X-A".len() + "1".len());
  }

  #[test]
  fn try_wire_spans_rejects_obs_text() {
    let section = b"X-Bin: \xff\xfe\r\n\r\n";
    let (refs, _) = scan_header_fields(section).unwrap();
    assert!(try_wire_spans(section, &refs).is_none());
    let copied = materialize_headers(&refs);
    assert_eq!(copied.get("x-bin"), Some("\u{fffd}\u{fffd}"));
  }

  #[test]
  fn try_wire_spans_trims_ows_offsets() {
    let section = b"Host:   example.com  \r\n\r\n";
    let (refs, _) = scan_header_fields(section).unwrap();
    let spans = try_wire_spans(section, &refs).unwrap();
    let headers = Headers::from_spans(Bytes::copy_from_slice(section), spans);
    assert_eq!(headers.get("host"), Some("example.com"));
  }
}
