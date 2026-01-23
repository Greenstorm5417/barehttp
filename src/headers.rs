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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
  use super::*;
  use alloc::vec;

  #[test]
  fn headers_new_creates_empty() {
    let headers = Headers::new();
    assert!(headers.is_empty());
    assert_eq!(headers.len(), 0);
  }

  #[test]
  fn headers_insert_and_get() {
    let mut headers = Headers::new();
    headers.insert("Content-Type", "text/plain");

    assert_eq!(headers.get("Content-Type"), Some("text/plain"));
    assert_eq!(headers.len(), 1);
  }

  #[test]
  fn headers_get_is_case_insensitive() {
    let mut headers = Headers::new();
    headers.insert("Content-Type", "application/json");

    assert_eq!(headers.get("content-type"), Some("application/json"));
    assert_eq!(headers.get("CONTENT-TYPE"), Some("application/json"));
    assert_eq!(headers.get("CoNtEnT-TyPe"), Some("application/json"));
  }

  #[test]
  fn headers_contains_is_case_insensitive() {
    let mut headers = Headers::new();
    headers.insert("Authorization", "Bearer token");

    assert!(headers.contains("authorization"));
    assert!(headers.contains("AUTHORIZATION"));
    assert!(headers.contains("Authorization"));
    assert!(!headers.contains("Content-Type"));
  }

  #[test]
  fn headers_get_all_returns_multiple_values() {
    let mut headers = Headers::new();
    headers.insert("Set-Cookie", "session=abc");
    headers.insert("Set-Cookie", "user=john");
    headers.insert("Set-Cookie", "theme=dark");

    let cookies = headers.get_all("Set-Cookie");
    assert_eq!(cookies.len(), 3);
    assert!(cookies.contains(&"session=abc"));
    assert!(cookies.contains(&"user=john"));
    assert!(cookies.contains(&"theme=dark"));
  }

  #[test]
  fn headers_get_all_is_case_insensitive() {
    let mut headers = Headers::new();
    headers.insert("Set-Cookie", "value1");
    headers.insert("set-cookie", "value2");

    let values = headers.get_all("SET-COOKIE");
    assert_eq!(values.len(), 2);
  }

  #[test]
  fn headers_get_returns_first_value() {
    let mut headers = Headers::new();
    headers.insert("Accept", "text/html");
    headers.insert("Accept", "application/json");

    assert_eq!(headers.get("Accept"), Some("text/html"));
  }

  #[test]
  fn headers_remove_is_case_insensitive() {
    let mut headers = Headers::new();
    headers.insert("X-Custom", "value1");
    headers.insert("Content-Type", "text/plain");

    headers.remove("x-custom");

    assert!(!headers.contains("X-Custom"));
    assert!(headers.contains("Content-Type"));
    assert_eq!(headers.len(), 1);
  }

  #[test]
  fn headers_remove_all_matching() {
    let mut headers = Headers::new();
    headers.insert("Cache-Control", "no-cache");
    headers.insert("cache-control", "no-store");
    headers.insert("Content-Type", "text/plain");

    headers.remove("CACHE-CONTROL");

    assert_eq!(headers.len(), 1);
    assert!(!headers.contains("Cache-Control"));
  }

  #[test]
  fn headers_iter_returns_all_headers() {
    let mut headers = Headers::new();
    headers.insert("Host", "example.com");
    headers.insert("Accept", "*/*");

    assert_eq!(headers.iter().count(), 2);
  }

  #[test]
  fn headers_from_vec_construction() {
    let vec = vec![
      (String::from("Host"), String::from("example.com")),
      (String::from("Accept"), String::from("*/*")),
    ];

    let headers = Headers::from_vec(vec);
    assert_eq!(headers.len(), 2);
    assert_eq!(headers.get("Host"), Some("example.com"));
  }

  #[test]
  fn headers_into_vec_conversion() {
    let mut headers = Headers::new();
    headers.insert("X-Test", "value");

    let vec = headers.into_vec();
    assert_eq!(vec.len(), 1);
    assert_eq!(vec.first().unwrap().0, "X-Test");
    assert_eq!(vec.first().unwrap().1, "value");
  }

  #[test]
  fn headers_clone_creates_independent_copy() {
    let mut headers1 = Headers::new();
    headers1.insert("Original", "value");

    let mut headers2 = headers1.clone();
    headers2.insert("New", "data");

    assert_eq!(headers1.len(), 1);
    assert_eq!(headers2.len(), 2);
    assert!(!headers1.contains("New"));
  }

  #[test]
  fn headers_equality() {
    let mut headers1 = Headers::new();
    headers1.insert("Content-Type", "text/plain");

    let mut headers2 = Headers::new();
    headers2.insert("Content-Type", "text/plain");

    assert_eq!(headers1, headers2);
  }

  #[test]
  fn headers_get_nonexistent_returns_none() {
    let headers = Headers::new();
    assert_eq!(headers.get("Missing"), None);
  }
}
