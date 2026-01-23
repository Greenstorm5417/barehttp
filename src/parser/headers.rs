use crate::error::ParseError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeaderField<'a> {
  pub name: &'a [u8],
  pub value: &'a [u8],
}

impl<'a> HeaderField<'a> {
  /// Parse header fields with obs-fold (obsolete line folding) support.
  /// RFC 9112 Section 5.2: User agents MUST replace obs-fold with one or more SP octets.
  ///
  /// This function collects all header fields, handling obs-fold by replacing CRLF+whitespace
  /// with a single space character.
  pub fn parse(
    input: &'a [u8]
  ) -> Result<(alloc::vec::Vec<(alloc::vec::Vec<u8>, alloc::vec::Vec<u8>)>, &'a [u8]), ParseError> {
    use alloc::vec::Vec;

    let mut headers = Vec::new();
    let mut remaining = input;

    // RFC 9112 Section 2.2: Check for whitespace before first header
    // If whitespace appears between start-line and first header field,
    // either reject OR consume whitespace-prefixed lines. We choose to reject.
    if !remaining.is_empty() {
      let first_byte = remaining.first().copied();
      if first_byte == Some(b' ') || first_byte == Some(b'\t') {
        return Err(ParseError::WhitespaceBeforeHeaders);
      }
    }

    loop {
      // Check for end of headers
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

      // Parse header field name
      let Some(colon_pos) = remaining.iter().position(|&b| b == b':') else {
        return Err(ParseError::InvalidHeaderName);
      };

      if colon_pos == 0 {
        return Err(ParseError::InvalidHeaderName);
      }

      let name = remaining
        .get(..colon_pos)
        .ok_or(ParseError::InvalidHeaderName)?;

      // RFC 9112 Section 5.1: Check for trailing whitespace in header name
      // (which would indicate whitespace before colon)
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

      // Skip leading whitespace
      while !remaining.is_empty() {
        let first_byte = remaining.first().copied();
        if first_byte == Some(b' ') || first_byte == Some(b'\t') {
          remaining = remaining.get(1..).ok_or(ParseError::InvalidHeaderValue)?;
        } else {
          break;
        }
      }

      // Collect value with obs-fold handling
      loop {
        let mut line_end = 0;
        while line_end < remaining.len() {
          let byte_at_end = remaining.get(line_end).copied();
          if byte_at_end == Some(b'\r') || byte_at_end == Some(b'\n') {
            break;
          }
          line_end += 1;
        }

        // Add this line's value
        if line_end > 0 {
          let line_value = remaining
            .get(..line_end)
            .ok_or(ParseError::InvalidHeaderValue)?;
          value_bytes.extend_from_slice(line_value);
        }

        remaining = remaining.get(line_end..).ok_or(ParseError::MissingCrlf)?;

        // Check for CRLF or LF
        let next_byte0 = remaining.first().copied();
        let next_byte1 = remaining.get(1).copied();
        let next_byte2 = remaining.get(2).copied();

        // Check for obs-fold: CRLF followed by SP or HTAB
        if remaining.len() >= 3
          && next_byte0 == Some(b'\r')
          && next_byte1 == Some(b'\n')
          && (next_byte2 == Some(b' ') || next_byte2 == Some(b'\t'))
        {
          // RFC 9112 Section 5.2: Replace obs-fold with SP
          value_bytes.push(b' ');
          remaining = remaining.get(2..).ok_or(ParseError::MissingCrlf)?;
          // Skip the whitespace character(s)
          while !remaining.is_empty() {
            let ws_byte = remaining.first().copied();
            if ws_byte == Some(b' ') || ws_byte == Some(b'\t') {
              remaining = remaining.get(1..).ok_or(ParseError::InvalidHeaderValue)?;
            } else {
              break;
            }
          }
          continue; // Continue collecting value
        }

        // Check for obs-fold: LF followed by SP or HTAB
        if remaining.len() >= 2 && next_byte0 == Some(b'\n') && (next_byte1 == Some(b' ') || next_byte1 == Some(b'\t'))
        {
          // RFC 9112 Section 5.2: Replace obs-fold with SP
          value_bytes.push(b' ');
          remaining = remaining.get(1..).ok_or(ParseError::MissingCrlf)?;
          // Skip the whitespace character(s)
          while !remaining.is_empty() {
            let ws_byte = remaining.first().copied();
            if ws_byte == Some(b' ') || ws_byte == Some(b'\t') {
              remaining = remaining.get(1..).ok_or(ParseError::InvalidHeaderValue)?;
            } else {
              break;
            }
          }
          continue; // Continue collecting value
        }

        // Normal end of header field
        if remaining.len() >= 2 && next_byte0 == Some(b'\r') && next_byte1 == Some(b'\n') {
          remaining = remaining.get(2..).ok_or(ParseError::MissingCrlf)?;
          break;
        }

        if !remaining.is_empty() && next_byte0 == Some(b'\n') {
          remaining = remaining.get(1..).ok_or(ParseError::MissingCrlf)?;
          break;
        }

        return Err(ParseError::MissingCrlf);
      }

      // Trim trailing whitespace from value
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
}

const fn is_token_char(b: u8) -> bool {
  matches!(b,
    b'!' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'*' | b'+' | b'-' | b'.' |
    b'0'..=b'9' | b'A'..=b'Z' | b'^' | b'_' | b'`' | b'a'..=b'z' | b'|' | b'~'
  )
}
