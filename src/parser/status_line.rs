//! Status-line parse (`HTTP/1.x <code> <reason>`).

use super::status::StatusCode;
use super::version::Version;
use crate::error::ParseError;

/// Parsed HTTP status line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusLine<'a> {
  /// HTTP version.
  pub version: Version,
  /// Status code.
  pub status: StatusCode,
  /// Reason phrase bytes (may be empty).
  pub reason: &'a [u8],
}

impl<'a> StatusLine<'a> {
  pub fn parse(input: &'a [u8]) -> Result<(Self, &'a [u8]), ParseError> {
    if input.len() < 8 {
      return Err(ParseError::InvalidHttpVersion);
    }
    let version = Version::parse(input).map_err(|_| ParseError::InvalidHttpVersion)?;
    let rest1 = input.get(8..).ok_or(ParseError::InvalidHttpVersion)?;

    let first_char = rest1.first().copied();
    if rest1.is_empty() || first_char != Some(b' ') {
      return Err(ParseError::InvalidWhitespace);
    }
    let rest2 = rest1.get(1..).ok_or(ParseError::InvalidWhitespace)?;

    let (status, rest3) = StatusCode::parse(rest2)?;

    let second_space = rest3.first().copied();
    if rest3.is_empty() || second_space != Some(b' ') {
      return Err(ParseError::InvalidWhitespace);
    }
    let rest4 = rest3.get(1..).ok_or(ParseError::InvalidWhitespace)?;

    let mut i = 0;
    while i < rest4.len() {
      let ch = rest4.get(i).copied();
      if ch == Some(b'\r') || ch == Some(b'\n') {
        break;
      }
      i += 1;
    }

    let reason = rest4.get(..i).ok_or(ParseError::InvalidReasonPhrase)?;
    let rest5 = rest4.get(i..).ok_or(ParseError::InvalidReasonPhrase)?;
    let rest6 = expect_crlf(rest5)?;

    Ok((Self { version, status, reason }, rest6))
  }
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
