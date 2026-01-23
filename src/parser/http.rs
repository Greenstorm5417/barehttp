use super::status::StatusCode as StatusCodeType;
use super::version::Version;
use crate::error::ParseError;

pub type HttpVersion = Version;

impl HttpVersion {
  /// # Errors
  /// Returns an error if the input is not a valid HTTP version string.
  pub fn parse_http(input: &[u8]) -> Result<(Self, &[u8]), ParseError> {
    if input.len() < 8 {
      return Err(ParseError::InvalidHttpVersion);
    }

    let version = Self::parse(input).map_err(|_| ParseError::InvalidHttpVersion)?;
    let remaining = input.get(8..).ok_or(ParseError::InvalidHttpVersion)?;
    Ok((version, remaining))
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusCode {
  inner: StatusCodeType,
}

impl StatusCode {
  pub fn parse(input: &[u8]) -> Result<(Self, &[u8]), ParseError> {
    if input.len() < 3 {
      return Err(ParseError::InvalidStatusCode);
    }

    let d0 = *input.first().ok_or(ParseError::InvalidStatusCode)?;
    let d1 = *input.get(1).ok_or(ParseError::InvalidStatusCode)?;
    let d2 = *input.get(2).ok_or(ParseError::InvalidStatusCode)?;

    if !d0.is_ascii_digit() || !d1.is_ascii_digit() || !d2.is_ascii_digit() {
      return Err(ParseError::InvalidStatusCode);
    }

    #[allow(clippy::cast_lossless)]
    let code = u16::from(d0 - b'0') * 100 + u16::from(d1 - b'0') * 10 + u16::from(d2 - b'0');

    let inner = StatusCodeType::new(code).ok_or(ParseError::InvalidStatusCode)?;
    let remaining = input.get(3..).ok_or(ParseError::InvalidStatusCode)?;
    Ok((Self { inner }, remaining))
  }

  pub const fn code(self) -> u16 {
    self.inner.as_u16()
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusLine<'a> {
  pub version: HttpVersion,
  pub status: StatusCode,
  pub reason: &'a [u8],
}

impl<'a> StatusLine<'a> {
  pub fn parse(input: &'a [u8]) -> Result<(Self, &'a [u8]), ParseError> {
    let (version, rest1) = HttpVersion::parse_http(input)?;

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
