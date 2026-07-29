//! Header-section parser (RFC 9112 §5). Obs-fold is rejected.

use crate::error::ParseError;

/// Parse header fields. Obs-fold is rejected (`ObsoleteFoldInHeader`).
///
/// Returns owned name/value pairs and remainder after the blank line.
///
/// # Errors
/// Returns [`ParseError`] on malformed headers, obs-fold, or whitespace before the first field.
pub fn parse_header_fields(
  input: &[u8]
) -> Result<(alloc::vec::Vec<(alloc::vec::Vec<u8>, alloc::vec::Vec<u8>)>, &[u8]), ParseError> {
  use alloc::vec::Vec;

  let mut headers = Vec::new();
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

    let name = remaining
      .get(..colon_pos)
      .ok_or(ParseError::InvalidHeaderName)?;

    if name.iter().any(|&b| b == b' ' || b == b'\t') {
      return Err(ParseError::InvalidHeaderName);
    }

    for &b in name {
      if !is_token_char(b) {
        return Err(ParseError::InvalidHeaderName);
      }
    }

    let mut value_bytes = Vec::new();
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

    let mut line_end = 0;
    while line_end < remaining.len() {
      let byte_at_end = remaining.get(line_end).copied();
      if byte_at_end == Some(b'\r') || byte_at_end == Some(b'\n') {
        break;
      }
      line_end += 1;
    }

    if line_end > 0 {
      let line_value = remaining
        .get(..line_end)
        .ok_or(ParseError::InvalidHeaderValue)?;
      value_bytes.extend_from_slice(line_value);
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

    while !value_bytes.is_empty() {
      let len = value_bytes.len();
      let last_byte = value_bytes.get(len - 1).copied();
      if last_byte == Some(b' ') || last_byte == Some(b'\t') {
        value_bytes.pop();
      } else {
        break;
      }
    }

    headers.push((name.to_vec(), value_bytes));
  }

  Ok((headers, remaining))
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

  if byte0 == Some(b'\r') {
    return Err(ParseError::BareCarriageReturn);
  }

  Err(ParseError::MissingCrlf)
}
