use alloc::string::String;
use alloc::vec::Vec;

/// HTTP headers collection
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Headers {
  headers: Vec<(String, String)>,
}

impl Headers {
  /// Create an empty headers collection
  #[must_use]
  pub const fn new() -> Self {
    Self {
      headers: Vec::new(),
    }
  }

  /// Create headers from a vector of tuples
  #[must_use]
  pub const fn from_vec(headers: Vec<(String, String)>) -> Self {
    Self { headers }
  }

  /// Add a header
  pub fn insert(&mut self, name: impl Into<String>, value: impl Into<String>) {
    self.headers.push((name.into(), value.into()));
  }

  /// Get the first value for a header name (case-insensitive)
  #[must_use]
  pub fn get(&self, name: &str) -> Option<&str> {
    self
      .headers
      .iter()
      .find(|(n, _)| n.eq_ignore_ascii_case(name))
      .map(|(_, v)| v.as_str())
  }

  /// Get all values for a header name (case-insensitive)
  #[must_use]
  pub fn get_all(&self, name: &str) -> Vec<&str> {
    self
      .headers
      .iter()
      .filter(|(n, _)| n.eq_ignore_ascii_case(name))
      .map(|(_, v)| v.as_str())
      .collect()
  }

  /// Check if a header exists (case-insensitive)
  #[must_use]
  pub fn contains(&self, name: &str) -> bool {
    self
      .headers
      .iter()
      .any(|(n, _)| n.eq_ignore_ascii_case(name))
  }

  /// Remove all headers with the given name (case-insensitive)
  pub fn remove(&mut self, name: &str) {
    self.headers.retain(|(n, _)| !n.eq_ignore_ascii_case(name));
  }

  /// Get an iterator over all headers
  pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
    self.headers.iter().map(|(n, v)| (n.as_str(), v.as_str()))
  }

  /// Get the number of headers
  #[must_use]
  pub const fn len(&self) -> usize {
    self.headers.len()
  }

  /// Check if the headers collection is empty
  #[must_use]
  pub const fn is_empty(&self) -> bool {
    self.headers.is_empty()
  }

  /// Get a reference to the internal vector
  #[must_use]
  pub const fn as_vec(&self) -> &Vec<(String, String)> {
    &self.headers
  }

  /// Get a mutable reference to the internal vector
  #[must_use]
  pub const fn as_vec_mut(&mut self) -> &mut Vec<(String, String)> {
    &mut self.headers
  }

  /// Convert into the internal vector
  #[must_use]
  pub fn into_vec(self) -> Vec<(String, String)> {
    self.headers
  }
}

impl From<Vec<(String, String)>> for Headers {
  fn from(headers: Vec<(String, String)>) -> Self {
    Self::from_vec(headers)
  }
}

impl<'a> IntoIterator for &'a Headers {
  type Item = &'a (String, String);
  type IntoIter = core::slice::Iter<'a, (String, String)>;

  fn into_iter(self) -> Self::IntoIter {
    self.headers.iter()
  }
}

impl IntoIterator for Headers {
  type Item = (String, String);
  type IntoIter = alloc::vec::IntoIter<(String, String)>;

  fn into_iter(self) -> Self::IntoIter {
    self.headers.into_iter()
  }
}
