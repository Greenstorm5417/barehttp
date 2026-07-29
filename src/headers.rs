use alloc::string::String;
use alloc::vec::Vec;
use core::slice;

/// Ordered list of `(name, value)` header fields.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Headers {
  headers: Vec<(String, String)>,
}

/// Iterator over `(name, value)` pairs in a [`Headers`] map.
#[derive(Debug, Clone)]
pub struct Iter<'a> {
  inner: slice::Iter<'a, (String, String)>,
}

impl<'a> Iterator for Iter<'a> {
  type Item = (&'a str, &'a str);

  fn next(&mut self) -> Option<Self::Item> {
    self.inner.next().map(|(n, v)| (n.as_str(), v.as_str()))
  }

  fn size_hint(&self) -> (usize, Option<usize>) {
    self.inner.size_hint()
  }
}

impl ExactSizeIterator for Iter<'_> {
  fn len(&self) -> usize {
    self.inner.len()
  }
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
    self.remove(&owned_name);
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
    let mut out = Vec::new();
    for (n, v) in &self.headers {
      if n.eq_ignore_ascii_case(name) {
        out.push(v.as_str());
      }
    }
    out
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
  #[must_use]
  pub fn iter(&self) -> Iter<'_> {
    Iter {
      inner: self.headers.iter(),
    }
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

impl FromIterator<(String, String)> for Headers {
  fn from_iter<T: IntoIterator<Item = (String, String)>>(iter: T) -> Self {
    Self {
      headers: iter.into_iter().collect(),
    }
  }
}

impl Extend<(String, String)> for Headers {
  fn extend<T: IntoIterator<Item = (String, String)>>(
    &mut self,
    iter: T,
  ) {
    self.headers.extend(iter);
  }
}

impl<'a> IntoIterator for &'a Headers {
  type Item = (&'a str, &'a str);
  type IntoIter = Iter<'a>;

  fn into_iter(self) -> Self::IntoIter {
    self.iter()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn set_replaces_all_case_insensitive_values() {
    let mut h = Headers::new();
    h.insert("Content-Type", "text/plain");
    h.insert("content-type", "text/html");
    h.set("CONTENT-TYPE", "application/json");
    assert_eq!(h.get_all("content-type"), alloc::vec!["application/json"]);
    assert_eq!(h.len(), 1);
  }

  #[test]
  fn remove_drops_all_case_insensitive() {
    let mut h = Headers::new();
    h.insert("X-A", "1");
    h.insert("x-a", "2");
    h.insert("Host", "example.com");
    h.remove("x-a");
    assert!(!h.contains("X-A"));
    assert_eq!(h.get("host"), Some("example.com"));
  }

  #[test]
  fn get_all_preserves_order_of_duplicates() {
    let mut h = Headers::new();
    h.insert("Set-Cookie", "a=1");
    h.insert("Set-Cookie", "b=2");
    assert_eq!(h.get_all("set-cookie"), alloc::vec!["a=1", "b=2"]);
  }

  #[test]
  fn merge_cookie_appends_or_inserts() {
    let mut h = Headers::new();
    h.merge_cookie("a=1");
    assert_eq!(h.get(Headers::COOKIE), Some("a=1"));
    h.merge_cookie("b=2");
    assert_eq!(h.get(Headers::COOKIE), Some("a=1; b=2"));
    h.merge_cookie("");
    assert_eq!(h.get(Headers::COOKIE), Some("a=1; b=2"));
  }

  #[test]
  fn from_iterator_and_extend() {
    let h: Headers = [
      (String::from("A"), String::from("1")),
      (String::from("B"), String::from("2")),
    ]
    .into_iter()
    .collect();
    assert_eq!(h.get("a"), Some("1"));
    let mut h2 = Headers::new();
    h2.extend([(String::from("C"), String::from("3"))]);
    assert_eq!(h2.get("c"), Some("3"));
  }

  #[test]
  fn property_header_lookup_case_insensitive() {
    use proptest::prelude::*;
    proptest::proptest!(|(
      name in "[A-Za-z][A-Za-z0-9-]{0,24}",
      value in "[ -~]{0,40}",
      case_flip in any::<bool>()
    )| {
      let mut h = Headers::new();
      h.insert(name.clone(), value.clone());
      let probe = if case_flip {
        name.to_ascii_uppercase()
      } else {
        name.to_ascii_lowercase()
      };
      prop_assert_eq!(h.get(&probe), Some(value.as_str()));
      prop_assert!(h.contains(&probe));
    });
  }
}
