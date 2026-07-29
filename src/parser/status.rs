//! HTTP status code helpers (crate-internal + re-exported at crate root).

use crate::error::ParseError;

/// HTTP status code in `100..=599`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StatusCode(u16);

/// Broad class of an HTTP status code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StatusClass {
  /// 1xx
  Informational,
  /// 2xx
  Successful,
  /// 3xx
  Redirection,
  /// 4xx
  ClientError,
  /// 5xx
  ServerError,
}

impl StatusCode {
  /// Create a status code, or `None` if outside `100..=599`.
  #[must_use]
  pub const fn new(code: u16) -> Option<Self> {
    if code >= 100 && code <= 599 {
      Some(Self(code))
    } else {
      None
    }
  }

  /// Numeric code.
  #[must_use]
  pub const fn as_u16(self) -> u16 {
    self.0
  }

  /// Parse a 3-digit status code from the front of `input`.
  ///
  /// # Errors
  /// Returns an error if fewer than 3 digits, non-digits, or out of `100..=599`.
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

    let status = Self::new(code).ok_or(ParseError::InvalidStatusCode)?;
    let remaining = input.get(3..).ok_or(ParseError::InvalidStatusCode)?;
    Ok((status, remaining))
  }

  /// Status class (1xx..=5xx).
  #[must_use]
  pub const fn class(self) -> StatusClass {
    match self.0 {
      100..=199 => StatusClass::Informational,
      200..=299 => StatusClass::Successful,
      300..=399 => StatusClass::Redirection,
      400..=499 => StatusClass::ClientError,
      _ => StatusClass::ServerError,
    }
  }

  /// 1xx
  #[must_use]
  pub const fn is_informational(self) -> bool {
    matches!(self.class(), StatusClass::Informational)
  }

  /// 2xx
  #[must_use]
  pub const fn is_successful(self) -> bool {
    matches!(self.class(), StatusClass::Successful)
  }

  /// 3xx
  #[must_use]
  pub const fn is_redirection(self) -> bool {
    matches!(self.class(), StatusClass::Redirection)
  }

  /// 4xx
  #[must_use]
  pub const fn is_client_error(self) -> bool {
    matches!(self.class(), StatusClass::ClientError)
  }

  /// 5xx
  #[must_use]
  pub const fn is_server_error(self) -> bool {
    matches!(self.class(), StatusClass::ServerError)
  }
}
