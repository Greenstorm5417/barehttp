use crate::error::ParseError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeaderField<'a> {
  pub name: &'a [u8],
  pub value: &'a [u8],
}

impl<'a> HeaderField<'a> {
  /// Parse a header field line. Returns None if we've reached the end of headers (empty line).
  /// Per RFC 9112 Section 5.2, clients MUST handle obsolete line folding in responses.
  pub fn parse(input: &'a [u8]) -> Result<(Option<Self>, &'a [u8]), ParseError> {
    let byte0 = input.first().copied();
    let byte1 = input.get(1).copied();

    if input.len() >= 2 && byte0 == Some(b'\r') && byte1 == Some(b'\n') {
      let remaining = input.get(2..).ok_or(ParseError::MissingCrlf)?;
      return Ok((None, remaining));
    }

    if !input.is_empty() && byte0 == Some(b'\n') {
      let remaining = input.get(1..).ok_or(ParseError::MissingCrlf)?;
      return Ok((None, remaining));
    }

    let Some(colon_pos) = input.iter().position(|&b| b == b':') else {
      return Err(ParseError::InvalidHeaderName);
    };

    if colon_pos == 0 {
      return Err(ParseError::InvalidHeaderName);
    }

    let name = input
      .get(..colon_pos)
      .ok_or(ParseError::InvalidHeaderName)?;

    for &b in name {
      if !is_token_char(b) {
        return Err(ParseError::InvalidHeaderName);
      }
    }

    let mut rest = input
      .get(colon_pos + 1..)
      .ok_or(ParseError::InvalidHeaderValue)?;

    while !rest.is_empty() {
      let first_byte = rest.first().copied();
      if first_byte == Some(b' ') || first_byte == Some(b'\t') {
        rest = rest.get(1..).ok_or(ParseError::InvalidHeaderValue)?;
      } else {
        break;
      }
    }

    let mut value_end = 0;
    while value_end < rest.len() {
      let byte_at_end = rest.get(value_end).copied();
      if byte_at_end == Some(b'\r') || byte_at_end == Some(b'\n') {
        break;
      }
      value_end += 1;
    }

    let mut value = rest
      .get(..value_end)
      .ok_or(ParseError::InvalidHeaderValue)?;

    while !value.is_empty() {
      let len = value.len();
      let last_byte = value.get(len - 1).copied();
      if last_byte == Some(b' ') || last_byte == Some(b'\t') {
        value = value.get(..len - 1).ok_or(ParseError::InvalidHeaderValue)?;
      } else {
        break;
      }
    }

    let rest_after_value = rest.get(value_end..).ok_or(ParseError::MissingCrlf)?;

    let rest_byte0 = rest_after_value.first().copied();
    let rest_byte1 = rest_after_value.get(1).copied();

    if rest_after_value.len() >= 2
      && rest_byte0 == Some(b'\r')
      && rest_byte1 == Some(b'\n')
    {
      let final_rest = rest_after_value.get(2..).ok_or(ParseError::MissingCrlf)?;
      return Ok((Some(Self { name, value }), final_rest));
    }

    if !rest_after_value.is_empty() && rest_byte0 == Some(b'\n') {
      let final_rest = rest_after_value.get(1..).ok_or(ParseError::MissingCrlf)?;
      return Ok((Some(Self { name, value }), final_rest));
    }

    Err(ParseError::MissingCrlf)
  }
}

const fn is_token_char(b: u8) -> bool {
  matches!(b,
    b'!' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'*' | b'+' | b'-' | b'.' |
    b'0'..=b'9' | b'A'..=b'Z' | b'^' | b'_' | b'`' | b'a'..=b'z' | b'|' | b'~'
  )
}
