/// HTTP version (e.g. HTTP/1.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Version {
  major: u8,
  minor: u8,
}

impl Version {
  /// HTTP/1.0
  pub const HTTP_10: Self = Self { major: 1, minor: 0 };
  /// HTTP/1.1
  pub const HTTP_11: Self = Self { major: 1, minor: 1 };

  /// Parse HTTP version from bytes (`HTTP/x.y`).
  ///
  /// # Errors
  ///
  /// Returns [`crate::ParseError::InvalidHttpVersion`] if the input is too short, has an
  /// invalid prefix, or contains invalid version numbers.
  pub fn parse(input: &[u8]) -> Result<Self, crate::error::ParseError> {
    use crate::error::ParseError;
    if input.len() < 8 {
      return Err(ParseError::InvalidHttpVersion);
    }

    if input.get(0..5) != Some(b"HTTP/") {
      return Err(ParseError::InvalidHttpVersion);
    }

    let major = *input.get(5).ok_or(ParseError::InvalidHttpVersion)?;
    if !major.is_ascii_digit() {
      return Err(ParseError::InvalidHttpVersion);
    }

    if input.get(6) != Some(&b'.') {
      return Err(ParseError::InvalidHttpVersion);
    }

    let minor = *input.get(7).ok_or(ParseError::InvalidHttpVersion)?;
    if !minor.is_ascii_digit() {
      return Err(ParseError::InvalidHttpVersion);
    }

    Ok(Self {
      major: major - b'0',
      minor: minor - b'0',
    })
  }
}

/// Parse status line: `HTTP/x.y <code> <reason>\r\n`.
///
/// Returns `(version, status, reason, rest)`.
pub fn parse_status_line(input: &[u8]) -> Result<(Version, u16, &[u8], &[u8]), crate::error::ParseError> {
  use crate::error::ParseError;
  use crate::parser::headers::expect_crlf;

  if input.len() < 8 {
    return Err(ParseError::InvalidHttpVersion);
  }
  let version = Version::parse(input)?;
  let rest1 = input.get(8..).ok_or(ParseError::InvalidHttpVersion)?;

  if rest1.first().copied() != Some(b' ') {
    return Err(ParseError::InvalidWhitespace);
  }
  let rest2 = rest1.get(1..).ok_or(ParseError::InvalidWhitespace)?;

  let (status, rest3) = parse_status_code(rest2)?;

  if rest3.first().copied() != Some(b' ') {
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

  Ok((version, status, reason, rest6))
}

fn parse_status_code(input: &[u8]) -> Result<(u16, &[u8]), crate::error::ParseError> {
  use crate::error::ParseError;

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

  if !(100..=599).contains(&code) {
    return Err(ParseError::InvalidStatusCode);
  }
  let remaining = input.get(3..).ok_or(ParseError::InvalidStatusCode)?;
  Ok((code, remaining))
}
