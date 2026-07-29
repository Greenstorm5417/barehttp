use alloc::string::String;
use alloc::vec::Vec;

/// Ordered list of `(name, value)` header fields.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Headers {
  headers: Vec<(String, String)>,
}

impl Headers {
  /// Empty collection.
  #[must_use]
  pub const fn new() -> Self {
    Self { headers: Vec::new() }
  }

  /// Wrap an existing `(name, value)` list.
  #[must_use]
  pub const fn from_vec(headers: Vec<(String, String)>) -> Self {
    Self { headers }
  }

  /// Append a field; keeps any existing values for the same name.
  pub fn insert(
    &mut self,
    name: impl Into<String>,
    value: impl Into<String>,
  ) {
    self.headers.push((name.into(), value.into()));
  }

  /// Replace every value for `name` (case-insensitive) with a single value.
  pub fn set(
    &mut self,
    name: impl Into<String>,
    value: impl Into<String>,
  ) {
    let owned_name = name.into();
    self
      .headers
      .retain(|(n, _)| !n.eq_ignore_ascii_case(&owned_name));
    self.headers.push((owned_name, value.into()));
  }

  /// First value for `name`, if any (case-insensitive).
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

  /// All values for `name` (case-insensitive).
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

  /// Whether any field matches `name` (case-insensitive).
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

  /// Remove every field matching `name` (case-insensitive).
  pub fn remove(
    &mut self,
    name: &str,
  ) {
    self.headers.retain(|(n, _)| !n.eq_ignore_ascii_case(name));
  }

  /// Append to `Cookie` (`; `-joined), or insert if absent. No-op when `value` is empty.
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

  /// Iterate `(name, value)` pairs in insertion order.
  pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
    self.headers.iter().map(|(n, v)| (n.as_str(), v.as_str()))
  }

  /// Number of fields (including duplicate names).
  #[must_use]
  pub const fn len(&self) -> usize {
    self.headers.len()
  }

  /// `true` when there are no fields.
  #[must_use]
  pub const fn is_empty(&self) -> bool {
    self.headers.is_empty()
  }

  // Wire names used by this crate (string literals elsewhere are fine too).
  /// `Accept`
  pub const ACCEPT: &'static str = "Accept";
  /// `Accept-Encoding`
  pub const ACCEPT_ENCODING: &'static str = "Accept-Encoding";
  /// `Connection`
  pub const CONNECTION: &'static str = "Connection";
  /// `Content-Length`
  pub const CONTENT_LENGTH: &'static str = "Content-Length";
  /// `Content-Type`
  pub const CONTENT_TYPE: &'static str = "Content-Type";
  /// `Cookie`
  pub const COOKIE: &'static str = "Cookie";
  /// `Host`
  pub const HOST: &'static str = "Host";
  /// `Set-Cookie`
  pub const SET_COOKIE: &'static str = "Set-Cookie";
  /// `TE`
  pub const TE: &'static str = "TE";
  /// `Transfer-Encoding`
  pub const TRANSFER_ENCODING: &'static str = "Transfer-Encoding";
  /// `User-Agent`
  pub const USER_AGENT: &'static str = "User-Agent";
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
