/// HTTP protocol version
///
/// Represents an HTTP version like HTTP/1.1 or HTTP/2.0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Version {
  major: u8,
  minor: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionParseError {
  TooShort,
  InvalidPrefix,
  InvalidMajor,
  MissingDot,
  InvalidMinor,
}

impl Version {
  /// Create a new HTTP version
  #[must_use]
  pub const fn new(
    major: u8,
    minor: u8,
  ) -> Self {
    Self { major, minor }
  }

  /// Get the major version number
  #[must_use]
  pub const fn major(self) -> u8 {
    self.major
  }

  /// Get the minor version number
  #[must_use]
  pub const fn minor(self) -> u8 {
    self.minor
  }

  /// Parse HTTP version from bytes.
  ///
  /// # Errors
  ///
  /// Returns an error if the input is too short, has an invalid prefix, or contains invalid version numbers.
  pub fn parse(input: &[u8]) -> Result<Self, VersionParseError> {
    if input.len() < 8 {
      return Err(VersionParseError::TooShort);
    }

    if input.get(0..5) != Some(b"HTTP/") {
      return Err(VersionParseError::InvalidPrefix);
    }

    let major = *input.get(5).ok_or(VersionParseError::TooShort)?;
    if !major.is_ascii_digit() {
      return Err(VersionParseError::InvalidMajor);
    }

    if input.get(6) != Some(&b'.') {
      return Err(VersionParseError::MissingDot);
    }

    let minor = *input.get(7).ok_or(VersionParseError::TooShort)?;
    if !minor.is_ascii_digit() {
      return Err(VersionParseError::InvalidMinor);
    }

    Ok(Self::new(major - b'0', minor - b'0'))
  }

  /// HTTP/0.9
  pub const HTTP_09: Self = Self { major: 0, minor: 9 };
  /// HTTP/1.0
  pub const HTTP_10: Self = Self { major: 1, minor: 0 };
  /// HTTP/1.1
  pub const HTTP_11: Self = Self { major: 1, minor: 1 };
  /// HTTP/2.0
  pub const HTTP_2: Self = Self { major: 2, minor: 0 };
  /// HTTP/3.0
  pub const HTTP_3: Self = Self { major: 3, minor: 0 };
}
