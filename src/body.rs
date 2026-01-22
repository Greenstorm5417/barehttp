use alloc::string::String;
use alloc::vec::Vec;

/// HTTP request or response body
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Body {
  data: Vec<u8>,
}

impl Body {
  /// Create an empty body
  #[must_use]
  pub const fn empty() -> Self {
    Self { data: Vec::new() }
  }

  /// Create a body from bytes
  #[must_use]
  pub const fn from_bytes(data: Vec<u8>) -> Self {
    Self { data }
  }

  /// Create a body from a string
  #[must_use]
  pub const fn from_string(s: String) -> Self {
    Self {
      data: s.into_bytes(),
    }
  }

  /// Get the body as a byte slice
  #[must_use]
  pub fn as_bytes(&self) -> &[u8] {
    &self.data
  }

  /// Get the body length
  #[must_use]
  pub const fn len(&self) -> usize {
    self.data.len()
  }

  /// Check if the body is empty
  #[must_use]
  pub const fn is_empty(&self) -> bool {
    self.data.is_empty()
  }

  /// Convert the body into bytes
  #[must_use]
  pub fn into_bytes(self) -> Vec<u8> {
    self.data
  }

  /// Try to convert the body to a UTF-8 string
  ///
  /// # Errors
  /// Returns an error if the body contains invalid UTF-8
  pub fn to_string(&self) -> Result<String, alloc::string::FromUtf8Error> {
    String::from_utf8(self.data.clone())
  }

  /// Convert the body into a UTF-8 string
  ///
  /// # Errors
  /// Returns an error if the body contains invalid UTF-8
  pub fn into_string(self) -> Result<String, alloc::string::FromUtf8Error> {
    String::from_utf8(self.data)
  }

  /// Get a mutable reference to the internal bytes
  #[must_use]
  pub const fn as_bytes_mut(&mut self) -> &mut Vec<u8> {
    &mut self.data
  }
}

impl From<Vec<u8>> for Body {
  fn from(data: Vec<u8>) -> Self {
    Self::from_bytes(data)
  }
}

impl From<String> for Body {
  fn from(s: String) -> Self {
    Self::from_string(s)
  }
}

impl From<&str> for Body {
  fn from(s: &str) -> Self {
    Self::from_string(String::from(s))
  }
}

impl AsRef<[u8]> for Body {
  fn as_ref(&self) -> &[u8] {
    &self.data
  }
}
