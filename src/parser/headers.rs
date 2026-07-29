//! Header-section scanner (RFC 9112 §5). Obs-fold is rejected.
//!
//! Zero-copy: field names/values are `&[u8]` views into the input until
//! [`materialize_headers`] (or [`parse_header_fields`]) builds owned [`Headers`].

use crate::error::ParseError;
use crate::headers::Headers;
use alloc::vec::Vec;
use compact_str::CompactString;

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
pub fn scan_header_fields(input: &[u8]) -> Result<(Vec<HeaderRef<'_>>, &[u8]), ParseError> {
  let mut headers = Vec::with_capacity(8);
  let remaining = for_each_header_field(input, |h| {
    headers.push(h);
    Ok(())
  })?;
  Ok((headers, remaining))
}

/// Promote borrowed header refs into owned [`Headers`].
#[must_use]
pub fn materialize_headers(refs: &[HeaderRef<'_>]) -> Headers {
  let mut headers = Headers::with_capacity(refs.len());
  for &h in refs {
    push_materialized(&mut headers, h);
  }
  headers.rebuild_index();
  headers
}

/// One-pass scan + materialize (buffered `Response::parse` / trailers).
///
/// Avoids an intermediate [`HeaderRef`] `Vec` when owned [`Headers`] are required
/// immediately.
///
/// # Errors
/// Malformed fields, obs-fold, or whitespace before the first field.
pub fn parse_header_fields(input: &[u8]) -> Result<(Headers, &[u8]), ParseError> {
  let mut headers = Headers::with_capacity(8);
  let remaining = for_each_header_field(input, |h| {
    push_materialized(&mut headers, h);
    Ok(())
  })?;
  headers.rebuild_index();
  Ok((headers, remaining))
}

#[inline]
fn push_materialized(
  headers: &mut Headers,
  h: HeaderRef<'_>,
) {
  // Token charset was validated in [`for_each_header_field`] → ASCII UTF-8.
  // SAFETY: every name byte is an RFC 9110 token char (ASCII).
  let name = CompactString::new(unsafe { core::str::from_utf8_unchecked(h.name) });
  // `from_utf8_lossy` fast-paths valid UTF-8; no intermediate `String`.
  let value = CompactString::from_utf8_lossy(h.value);
  headers.push_owned(name, value);
}

/// Shared field-line scanner. `on_field` runs once per header before the blank line.
fn for_each_header_field<'a, F>(
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

    let Some(colon_pos) = remaining.iter().position(|&b| b == b':') else {
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

    let line_end = remaining
      .iter()
      .position(|&b| b == b'\r' || b == b'\n')
      .unwrap_or(remaining.len());

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

/// RFC 9110 token character (header names).
#[inline]
pub const fn is_token_char(b: u8) -> bool {
  matches!(
    b,
    b'!'
      | b'#'
      | b'$'
      | b'%'
      | b'&'
      | b'\''
      | b'*'
      | b'+'
      | b'-'
      | b'.'
      | b'0'..=b'9'
      | b'A'..=b'Z'
      | b'^'
      | b'_'
      | b'`'
      | b'a'..=b'z'
      | b'|'
      | b'~'
  )
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
