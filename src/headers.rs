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
    Self { headers: Vec::new() }
  }

  /// Create headers from a vector of tuples
  #[must_use]
  pub const fn from_vec(headers: Vec<(String, String)>) -> Self {
    Self { headers }
  }

  /// Add a header
  pub fn insert(
    &mut self,
    name: impl Into<String>,
    value: impl Into<String>,
  ) {
    self.headers.push((name.into(), value.into()));
  }

  /// Get the first value for a header name (case-insensitive)
  #[must_use]
  pub fn get(
    &self,
    name: &str,
  ) -> Option<&str> {
    self
      .headers
      .iter()
      .find(|(n, _)| n.eq_ignore_ascii_case(name))
      .map(|(_, v)| v.as_str())
  }

  /// Get all values for a header name (case-insensitive)
  #[must_use]
  pub fn get_all(
    &self,
    name: &str,
  ) -> Vec<&str> {
    self
      .headers
      .iter()
      .filter(|(n, _)| n.eq_ignore_ascii_case(name))
      .map(|(_, v)| v.as_str())
      .collect()
  }

  /// Check if a header exists (case-insensitive)
  #[must_use]
  pub fn contains(
    &self,
    name: &str,
  ) -> bool {
    self
      .headers
      .iter()
      .any(|(n, _)| n.eq_ignore_ascii_case(name))
  }

  /// Remove all headers with the given name (case-insensitive)
  pub fn remove(
    &mut self,
    name: &str,
  ) {
    self.headers.retain(|(n, _)| !n.eq_ignore_ascii_case(name));
  }

  /// Append to Cookie header (`; `-joined), or insert if absent. No-op if `value` is empty.
  pub fn merge_cookie(
    &mut self,
    value: &str,
  ) {
    if value.is_empty() {
      return;
    }
    if let Some(existing) = self.get(Self::COOKIE) {
      let combined = alloc::format!("{existing}; {value}");
      self.remove(Self::COOKIE);
      self.insert(Self::COOKIE, combined);
    } else {
      self.insert(Self::COOKIE, value);
    }
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

  // Header names used by this crate (string literals elsewhere are fine too).
  /// `accept`
  pub const ACCEPT: &'static str = "accept";
  /// `accept-encoding`
  pub const ACCEPT_ENCODING: &'static str = "accept-encoding";
  /// `connection`
  pub const CONNECTION: &'static str = "connection";
  /// `content-length`
  pub const CONTENT_LENGTH: &'static str = "content-length";
  /// `content-type`
  pub const CONTENT_TYPE: &'static str = "content-type";
  /// `cookie`
  pub const COOKIE: &'static str = "cookie";
  /// `host`
  pub const HOST: &'static str = "host";
  /// `set-cookie`
  pub const SET_COOKIE: &'static str = "set-cookie";
  /// `te`
  pub const TE: &'static str = "te";
  /// `transfer-encoding`
  pub const TRANSFER_ENCODING: &'static str = "transfer-encoding";
  /// `user-agent`
  pub const USER_AGENT: &'static str = "user-agent";
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
