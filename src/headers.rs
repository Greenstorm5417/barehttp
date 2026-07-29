use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use compact_str::CompactString;
use core::hash::{Hash, Hasher};
use core::slice;
use hashbrown::Equivalent;
use hashbrown::HashMap;

/// Ordered list of `(name, value)` header fields.
///
/// Names and values are stored compactly (short strings stay inline).
/// A lowercase → first-index map, when present, backs [`Self::get`] /
/// [`Self::contains`]; iteration and multi-value order follow insertion order.
///
/// # String policy
///
/// - Mutation / ingest that copies into storage: `impl AsRef<str>`
/// - Lookups: `&str`
/// - Owned export via [`Self::into_vec`]: `(String, String)`
///
/// # Examples
///
/// ```
/// use barehttp::Headers;
///
/// let mut headers = Headers::new();
/// headers.insert("Content-Type", "text/plain");
/// assert_eq!(headers.get("content-type"), Some("text/plain"));
/// ```
#[derive(Debug, Clone, Default)]
pub struct Headers {
  headers: Vec<(CompactString, CompactString)>,
  /// ASCII-lowercased name → index of the first matching field in `headers`.
  /// Boxed so empty `Headers` stays pointer-sized (keeps `Error::HttpStatus` small).
  index: Option<Box<HashMap<CompactString, usize>>>,
}

impl PartialEq for Headers {
  fn eq(
    &self,
    other: &Self,
  ) -> bool {
    // Index is a cache; equality is defined by ordered fields only.
    self.headers == other.headers
  }
}

impl Eq for Headers {}

impl Hash for Headers {
  fn hash<H: Hasher>(
    &self,
    state: &mut H,
  ) {
    // Must match `PartialEq`: hash fields only, not the index cache.
    self.headers.hash(state);
  }
}

/// Iterator over `(name, value)` pairs in a [`Headers`] map.
#[derive(Debug, Clone)]
pub struct Iter<'a> {
  inner: slice::Iter<'a, (CompactString, CompactString)>,
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

/// Common header names with a compile-time `phf` lookup (lowercase keys).
///
/// Framing and wire code match names against this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum WellKnownHeader {
  /// `Accept`
  Accept,
  /// `Accept-Encoding`
  AcceptEncoding,
  /// `Connection`
  Connection,
  /// `Content-Encoding`
  ContentEncoding,
  /// `Content-Length`
  ContentLength,
  /// `Content-Type`
  ContentType,
  /// `Cookie`
  Cookie,
  /// `Host`
  Host,
  /// `Set-Cookie`
  SetCookie,
  /// `TE`
  Te,
  /// `Transfer-Encoding`
  TransferEncoding,
  /// `User-Agent`
  UserAgent,
}

impl WellKnownHeader {
  /// Canonical wire name matching [`Headers`] constants (mixed case).
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Accept => Headers::ACCEPT,
      Self::AcceptEncoding => Headers::ACCEPT_ENCODING,
      Self::Connection => Headers::CONNECTION,
      Self::ContentEncoding => Headers::CONTENT_ENCODING,
      Self::ContentLength => Headers::CONTENT_LENGTH,
      Self::ContentType => Headers::CONTENT_TYPE,
      Self::Cookie => Headers::COOKIE,
      Self::Host => Headers::HOST,
      Self::SetCookie => Headers::SET_COOKIE,
      Self::Te => Headers::TE,
      Self::TransferEncoding => Headers::TRANSFER_ENCODING,
      Self::UserAgent => Headers::USER_AGENT,
    }
  }
}

/// Lowercase well-known names → [`WellKnownHeader`] (compile-time PHF).
static WELL_KNOWN: phf::Map<&'static str, WellKnownHeader> = phf::phf_map! {
  "accept" => WellKnownHeader::Accept,
  "accept-encoding" => WellKnownHeader::AcceptEncoding,
  "connection" => WellKnownHeader::Connection,
  "content-encoding" => WellKnownHeader::ContentEncoding,
  "content-length" => WellKnownHeader::ContentLength,
  "content-type" => WellKnownHeader::ContentType,
  "cookie" => WellKnownHeader::Cookie,
  "host" => WellKnownHeader::Host,
  "set-cookie" => WellKnownHeader::SetCookie,
  "te" => WellKnownHeader::Te,
  "transfer-encoding" => WellKnownHeader::TransferEncoding,
  "user-agent" => WellKnownHeader::UserAgent,
};

const WELL_KNOWN_MAX_LEN: usize = 32;

/// Below this field count, skip the side-index (linear scan is cheaper).
const INDEX_THRESHOLD: usize = 8;

/// Case-insensitive lookup of a well-known header name (`&str`).
#[must_use]
pub fn well_known_header(name: &str) -> Option<WellKnownHeader> {
  well_known_header_bytes(name.as_bytes())
}

/// Case-insensitive lookup of a well-known header name (raw bytes).
#[must_use]
pub fn well_known_header_bytes(name: &[u8]) -> Option<WellKnownHeader> {
  if name.len() > WELL_KNOWN_MAX_LEN || name.is_empty() {
    return None;
  }
  let mut buf = [0u8; WELL_KNOWN_MAX_LEN];
  for (i, &b) in name.iter().enumerate() {
    // Reject non-ASCII early — well-known names are ASCII tokens only.
    if !b.is_ascii() {
      return None;
    }
    if let Some(slot) = buf.get_mut(i) {
      *slot = b.to_ascii_lowercase();
    }
  }
  let key_bytes = buf.get(..name.len())?;
  // SAFETY: every byte was checked `is_ascii` then lowercased.
  let key = unsafe { core::str::from_utf8_unchecked(key_bytes) };
  WELL_KNOWN.get(key).copied()
}

/// ASCII-lowercase `name` into internal key storage (inline for typical header lengths).
#[inline]
fn ascii_lowercase_key(name: &str) -> CompactString {
  const STACK: usize = 64;
  let bytes = name.as_bytes();
  if !bytes.iter().any(u8::is_ascii_uppercase) {
    return CompactString::new(name);
  }
  // Byte-wise build (avoids `push(char)` per octet). Stack for common lengths.
  if bytes.len() <= STACK {
    let mut buf = [0u8; STACK];
    for (i, &b) in bytes.iter().enumerate() {
      if let Some(slot) = buf.get_mut(i) {
        *slot = b.to_ascii_lowercase();
      }
    }
    // SAFETY: ASCII in → ASCII out.
    return CompactString::new(unsafe { core::str::from_utf8_unchecked(buf.get(..bytes.len()).unwrap_or(&[])) });
  }
  let mut owned = Vec::with_capacity(bytes.len());
  for &b in bytes {
    owned.push(b.to_ascii_lowercase());
  }
  // SAFETY: ASCII in → ASCII out.
  CompactString::new(unsafe { core::str::from_utf8_unchecked(&owned) })
}

/// Case-insensitive query for the lowercase index keys (no allocation on lookup).
#[derive(Clone, Copy)]
struct AsciiLowerQuery<'a>(&'a str);

impl Hash for AsciiLowerQuery<'_> {
  #[inline]
  fn hash<H: Hasher>(
    &self,
    state: &mut H,
  ) {
    const STACK: usize = 64;
    // Must match hashing of the lowercased form stored in the index.
    let bytes = self.0.as_bytes();
    if !bytes.iter().any(u8::is_ascii_uppercase) {
      self.0.hash(state);
      return;
    }
    if bytes.len() <= STACK {
      let mut buf = [0u8; STACK];
      for (i, &b) in bytes.iter().enumerate() {
        if let Some(slot) = buf.get_mut(i) {
          *slot = b.to_ascii_lowercase();
        }
      }
      // SAFETY: ASCII in → ASCII out.
      let s = unsafe { core::str::from_utf8_unchecked(buf.get(..bytes.len()).unwrap_or(&[])) };
      s.hash(state);
      return;
    }
    ascii_lowercase_key(self.0).as_str().hash(state);
  }
}

impl Equivalent<CompactString> for AsciiLowerQuery<'_> {
  #[inline]
  fn equivalent(
    &self,
    key: &CompactString,
  ) -> bool {
    self.0.eq_ignore_ascii_case(key.as_str())
  }
}

impl Headers {
  /// Empty collection.
  #[must_use]
  pub const fn new() -> Self {
    Self {
      headers: Vec::new(),
      index: None,
    }
  }

  /// Empty collection with room for `capacity` fields.
  #[must_use]
  pub fn with_capacity(capacity: usize) -> Self {
    Self {
      headers: Vec::with_capacity(capacity),
      // Index is built once after batch fills (`rebuild_index` / first `insert`).
      index: None,
    }
  }

  /// Wrap an existing `(name, value)` list.
  ///
  /// Accepts any pair types that borrow as [`str`] (e.g. [`&str`], [`String`]).
  #[must_use]
  pub fn from_vec<S, T>(headers: Vec<(S, T)>) -> Self
  where
    S: AsRef<str>,
    T: AsRef<str>,
  {
    headers.into_iter().collect()
  }

  /// Consume into owned `(name, value)` pairs as [`String`].
  ///
  /// Returns standard [`String`] pairs (not the crate's internal storage) so
  /// callers get a stable, dependency-free owned export.
  #[must_use]
  pub fn into_vec(self) -> Vec<(String, String)> {
    self
      .headers
      .into_iter()
      .map(|(n, v)| (n.into_string(), v.into_string()))
      .collect()
  }

  /// Append a field; keeps any existing values for the same name.
  pub fn insert(
    &mut self,
    name: impl AsRef<str>,
    value: impl AsRef<str>,
  ) {
    self.push_compact(CompactString::from(name.as_ref()), CompactString::from(value.as_ref()));
  }

  /// Append an already-owned field without touching the side-index.
  ///
  /// Hot path for wire materialize: caller must [`Self::rebuild_index`] once
  /// after the batch (avoids per-field [`HashMap`] inserts during parse).
  #[inline]
  pub(crate) fn push_owned(
    &mut self,
    name: CompactString,
    value: CompactString,
  ) {
    self.headers.push((name, value));
  }

  /// Rebuild the lowercase → first-index map from `headers` (source of truth).
  ///
  /// With few fields, leaves the map unset: a linear scan costs less than building
  /// a [`HashMap`] for the usual 2–4 header response.
  pub(crate) fn rebuild_index(&mut self) {
    if self.headers.len() < INDEX_THRESHOLD {
      self.index = None;
      return;
    }
    let map = self
      .index
      .get_or_insert_with(|| Box::new(HashMap::with_capacity(self.headers.len())));
    map.clear();
    map.reserve(self.headers.len());
    for (i, (name, _)) in self.headers.iter().enumerate() {
      let key = ascii_lowercase_key(name.as_str());
      map.entry(key).or_insert(i);
    }
  }

  #[inline]
  fn push_compact(
    &mut self,
    name: CompactString,
    value: CompactString,
  ) {
    let idx = self.headers.len();
    if let Some(map) = self.index.as_mut() {
      let key = ascii_lowercase_key(name.as_str());
      map.entry(key).or_insert(idx);
      self.headers.push((name, value));
    } else {
      // No index yet (small / deferred). Keep linear until we cross the threshold.
      self.headers.push((name, value));
      if self.headers.len() >= INDEX_THRESHOLD {
        self.rebuild_index();
      }
    }
  }

  /// Replace every value for `name` (case-insensitive) with a single value.
  pub fn set(
    &mut self,
    name: impl AsRef<str>,
    value: impl AsRef<str>,
  ) {
    let owned_name = CompactString::from(name.as_ref());
    let owned_value = CompactString::from(value.as_ref());
    let mut first: Option<usize> = None;
    let mut removed = false;
    let mut i = 0usize;
    while i < self.headers.len() {
      let is_match = self
        .headers
        .get(i)
        .is_some_and(|(n, _)| n.eq_ignore_ascii_case(owned_name.as_str()));
      if is_match {
        if first.is_none() {
          first = Some(i);
          i = i.saturating_add(1);
        } else {
          self.headers.remove(i);
          removed = true;
        }
      } else {
        i = i.saturating_add(1);
      }
    }
    if let Some(idx) = first {
      if let Some(slot) = self.headers.get_mut(idx) {
        *slot = (owned_name, owned_value);
      }
      if removed {
        // Indices after removals shifted — rebuild.
        self.rebuild_index();
      }
      // else: same first-index slot; lowercase key unchanged.
    } else {
      let idx = self.headers.len();
      let key = ascii_lowercase_key(owned_name.as_str());
      self.headers.push((owned_name, owned_value));
      if let Some(map) = self.index.as_mut() {
        map.entry(key).or_insert(idx);
      } else if self.headers.len() >= INDEX_THRESHOLD {
        self.rebuild_index();
      }
    }
  }

  /// First value for `name`, if any (case-insensitive).
  #[must_use]
  pub fn get(
    &self,
    name: &str,
  ) -> Option<&str> {
    if let Some(map) = self.index.as_ref() {
      let idx = *map.get(&AsciiLowerQuery(name))?;
      return self.headers.get(idx).map(|(_, v)| v.as_str());
    }
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
    if let Some(map) = self.index.as_ref() {
      return map.contains_key(&AsciiLowerQuery(name));
    }
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
    let before = self.headers.len();
    self.headers.retain(|(n, _)| !n.eq_ignore_ascii_case(name));
    if self.headers.len() != before {
      self.rebuild_index();
    }
  }

  /// Append to `Cookie` (`; `-joined), or insert if absent. No-op when `value` is empty.
  ///
  /// Builder / client plumbing — not part of the general header-map API.
  pub(crate) fn merge_cookie(
    &mut self,
    value: &str,
  ) {
    if value.is_empty() {
      return;
    }
    if let Some((_, existing)) = self
      .headers
      .iter_mut()
      .find(|(n, _)| n.eq_ignore_ascii_case(Self::COOKIE))
    {
      existing.push_str("; ");
      existing.push_str(value);
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
  /// `Content-Encoding`
  pub const CONTENT_ENCODING: &'static str = "Content-Encoding";
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

impl<S, T> FromIterator<(S, T)> for Headers
where
  S: AsRef<str>,
  T: AsRef<str>,
{
  fn from_iter<I: IntoIterator<Item = (S, T)>>(iter: I) -> Self {
    let pairs = iter.into_iter();
    let (lower, upper) = pairs.size_hint();
    let mut out = Self::with_capacity(upper.unwrap_or(lower));
    for (name, value) in pairs {
      out
        .headers
        .push((CompactString::from(name.as_ref()), CompactString::from(value.as_ref())));
    }
    out.rebuild_index();
    out
  }
}

impl<S, T> Extend<(S, T)> for Headers
where
  S: AsRef<str>,
  T: AsRef<str>,
{
  fn extend<I: IntoIterator<Item = (S, T)>>(
    &mut self,
    iter: I,
  ) {
    for (name, value) in iter {
      self.push_compact(CompactString::from(name.as_ref()), CompactString::from(value.as_ref()));
    }
  }
}

impl<'a> IntoIterator for &'a Headers {
  type Item = (&'a str, &'a str);
  type IntoIter = Iter<'a>;

  fn into_iter(self) -> Self::IntoIter {
    self.iter()
  }
}

/// Owning iterator over `(name, value)` pairs from [`Headers`].
#[derive(Debug)]
pub struct IntoIter {
  inner: alloc::vec::IntoIter<(CompactString, CompactString)>,
}

impl Iterator for IntoIter {
  type Item = (String, String);

  fn next(&mut self) -> Option<Self::Item> {
    self
      .inner
      .next()
      .map(|(n, v)| (String::from(n), String::from(v)))
  }

  fn size_hint(&self) -> (usize, Option<usize>) {
    self.inner.size_hint()
  }
}

impl ExactSizeIterator for IntoIter {
  fn len(&self) -> usize {
    self.inner.len()
  }
}

impl IntoIterator for Headers {
  type Item = (String, String);
  type IntoIter = IntoIter;

  fn into_iter(self) -> Self::IntoIter {
    IntoIter {
      inner: self.headers.into_iter(),
    }
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
  fn from_iterator_and_extend_accept_str_pairs() {
    let h: Headers = [("A", "1"), ("B", "2")].into_iter().collect();
    assert_eq!(h.get("a"), Some("1"));
    let mut h2 = Headers::new();
    h2.extend([("C", "3")]);
    assert_eq!(h2.get("c"), Some("3"));
  }

  #[test]
  fn from_vec_accepts_str_pairs() {
    let h = Headers::from_vec(alloc::vec![("Host", "example.com")]);
    assert_eq!(h.get("host"), Some("example.com"));
  }

  #[test]
  fn into_vec_returns_string_pairs() {
    let mut h = Headers::new();
    h.insert("Host", "example.com");
    let v = h.into_vec();
    assert_eq!(v, alloc::vec![(String::from("Host"), String::from("example.com"))]);
  }

  #[test]
  fn hash_matches_field_equality() {
    let mut a = Headers::new();
    a.insert("Host", "example.com");
    // Cross the index threshold so `a` has a side-index cache.
    for i in 0..INDEX_THRESHOLD {
      a.insert(alloc::format!("X-{i}"), "1");
    }
    // `b` built via FromIterator also rebuilds an index — Hash must still match Eq.
    let b: Headers = a.iter().collect();
    assert_eq!(a, b);
    assert_eq!(hash_value(&a), hash_value(&b));
  }

  fn hash_value(h: &Headers) -> u64 {
    struct CountingHasher(u64);
    impl Hasher for CountingHasher {
      fn finish(&self) -> u64 {
        self.0
      }
      fn write(
        &mut self,
        bytes: &[u8],
      ) {
        for b in bytes {
          self.0 = self.0.wrapping_mul(31).wrapping_add(u64::from(*b));
        }
      }
    }
    let mut hasher = CountingHasher(0);
    h.hash(&mut hasher);
    hasher.finish()
  }

  #[test]
  fn index_survives_set_and_remove() {
    let mut h = Headers::new();
    h.insert("A", "1");
    h.insert("B", "2");
    h.insert("a", "3");
    assert_eq!(h.get("A"), Some("1"));
    h.set("a", "9");
    assert_eq!(h.get("A"), Some("9"));
    assert_eq!(h.get("B"), Some("2"));
    h.remove("B");
    assert!(!h.contains("b"));
    assert_eq!(h.get("a"), Some("9"));
  }

  #[test]
  fn insert_after_small_materialize_keeps_lookups() {
    // Rebuild skips the side-index below the threshold; insert must not leave a
    // partial map that shadows earlier fields.
    let mut h = Headers::with_capacity(4);
    h.push_owned(CompactString::from("Host"), CompactString::from("a"));
    h.push_owned(CompactString::from("X-A"), CompactString::from("1"));
    h.rebuild_index();
    assert_eq!(h.get("host"), Some("a"));
    h.insert("X-B", "2");
    assert_eq!(h.get("host"), Some("a"));
    assert_eq!(h.get("x-a"), Some("1"));
    assert_eq!(h.get("x-b"), Some("2"));
  }

  #[test]
  fn well_known_lookup_is_case_insensitive() {
    assert_eq!(
      well_known_header("Transfer-Encoding"),
      Some(WellKnownHeader::TransferEncoding)
    );
    assert_eq!(
      well_known_header("CONTENT-LENGTH"),
      Some(WellKnownHeader::ContentLength)
    );
    assert_eq!(
      well_known_header_bytes(b"content-encoding"),
      Some(WellKnownHeader::ContentEncoding)
    );
    assert_eq!(well_known_header("X-Custom"), None);
    assert_eq!(WellKnownHeader::Host.as_str(), Headers::HOST);
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
