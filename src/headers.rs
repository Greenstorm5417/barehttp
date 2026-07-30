use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use bytes::{Bytes, BytesMut};
use core::hash::{Hash, Hasher};
use hashbrown::HashMap;

/// Ordered list of `(name, value)` header fields.
///
/// Names and values live in a shared [`Bytes`] arena as UTF-8 sub-slices (offsets).
/// A lowercase-hash → first-index map, when present, backs [`Self::get`] /
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
  /// Contiguous UTF-8 name/value bytes. On the connection path this may be a
  /// frozen slice of the receive buffer (status line / CRLFs / OWS may remain as
  /// dead bytes). `Clone` bumps the arena refcount; mutation copy-on-writes only
  /// when the arena is shared (unique mutates in place via [`BytesMut`]).
  buf: Bytes,
  /// Insertion-ordered fields as offsets into `buf`.
  fields: Vec<FieldSpan>,
  /// FNV-1a of ASCII-lowercased name → index of the first matching field.
  /// Shared via [`Arc`] so `Clone` keeps O(1) lookups without deep-copying the map
  /// (mutation uses [`Arc::make_mut`]). Lookups always re-check
  /// [`eq_ignore_ascii_case`](str::eq_ignore_ascii_case) so hash collisions fall
  /// back to a linear scan.
  index: Option<Arc<HashMap<u64, usize>>>,
}

/// Name/value span into [`Headers::buf`] (`u32` offsets; header sections are << 4 GiB).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct FieldSpan {
  name_start: u32,
  name_len: u32,
  value_start: u32,
  value_len: u32,
}

impl PartialEq for Headers {
  fn eq(
    &self,
    other: &Self,
  ) -> bool {
    // Index is a cache; equality is defined by ordered fields only.
    if self.fields.len() != other.fields.len() {
      return false;
    }
    self.iter().zip(other.iter()).all(|(a, b)| a == b)
  }
}

impl Eq for Headers {}

impl Hash for Headers {
  fn hash<H: Hasher>(
    &self,
    state: &mut H,
  ) {
    // Must match `PartialEq`: hash fields only, not the index cache or dead arena bytes.
    self.fields.len().hash(state);
    for (n, v) in self {
      n.hash(state);
      v.hash(state);
    }
  }
}

/// Iterator over `(name, value)` pairs in a [`Headers`] map.
#[derive(Debug, Clone)]
pub struct Iter<'a> {
  headers: &'a Headers,
  idx: usize,
}

impl<'a> Iterator for Iter<'a> {
  type Item = (&'a str, &'a str);

  fn next(&mut self) -> Option<Self::Item> {
    let span = self.headers.fields.get(self.idx)?;
    self.idx = self.idx.saturating_add(1);
    Some((self.headers.name_str(span), self.headers.value_str(span)))
  }

  fn size_hint(&self) -> (usize, Option<usize>) {
    let rem = self.headers.fields.len().saturating_sub(self.idx);
    (rem, Some(rem))
  }
}

impl ExactSizeIterator for Iter<'_> {
  fn len(&self) -> usize {
    self.headers.fields.len().saturating_sub(self.idx)
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

/// FNV-1a over ASCII-lowercased bytes (index key; not a stored lowercase string).
#[inline]
fn ascii_lower_hash(name: &str) -> u64 {
  let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
  for &b in name.as_bytes() {
    hash ^= u64::from(b.to_ascii_lowercase());
    hash = hash.wrapping_mul(0x0100_0000_01b3);
  }
  hash
}

#[inline]
fn usize_to_u32(n: usize) -> u32 {
  // Header sections are capped far below 4 GiB; truncate rather than panic under deny lint.
  u32::try_from(n).unwrap_or(u32::MAX)
}

impl Headers {
  /// Empty collection.
  #[must_use]
  pub const fn new() -> Self {
    Self {
      buf: Bytes::new(),
      fields: Vec::new(),
      index: None,
    }
  }

  /// Empty collection with room for `capacity` fields.
  #[must_use]
  pub fn with_capacity(capacity: usize) -> Self {
    Self {
      buf: Bytes::new(),
      fields: Vec::with_capacity(capacity),
      // Deferred after batch fill; built on set/remove/insert past [`INDEX_THRESHOLD`].
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

  /// Adopt a parent arena and field spans (offsets into `buf`).
  ///
  /// Used by the parser materialize path and by the connection path when the
  /// receive buffer's header section is frozen into `buf` (zero-copy). Spans may
  /// leave dead bytes (status line, CRLFs, OWS) in the arena. Side-index stays
  /// deferred. Mutation copy-on-writes (packing live spans when the arena is
  /// shared and has dead bytes) via [`BytesMut`].
  #[must_use]
  pub(crate) fn from_spans(
    buf: Bytes,
    fields: Vec<(u32, u32, u32, u32)>,
  ) -> Self {
    Self {
      buf,
      fields: fields
        .into_iter()
        .map(|(name_start, name_len, value_start, value_len)| FieldSpan {
          name_start,
          name_len,
          value_start,
          value_len,
        })
        .collect(),
      index: None,
    }
  }

  /// Length of the backing arena (including any dead wire bytes).
  #[cfg(test)]
  #[must_use]
  pub(crate) const fn arena_len(&self) -> usize {
    self.buf.len()
  }

  /// Consume into owned `(name, value)` pairs as [`String`].
  ///
  /// Returns standard [`String`] pairs (not the crate's internal storage) so
  /// callers get a stable, dependency-free owned export.
  #[must_use]
  pub fn into_vec(self) -> Vec<(String, String)> {
    self.into_iter().collect()
  }

  /// Append a field; keeps any existing values for the same name.
  pub fn insert(
    &mut self,
    name: impl AsRef<str>,
    value: impl AsRef<str>,
  ) {
    self.push_str(name.as_ref(), value.as_ref());
  }

  /// Append an already-owned field without touching the side-index.
  ///
  /// Hot path for wire materialize: leaves `index` unset. Lookups stay linear
  /// until a mutating API ([`Self::set`] / [`Self::remove`] / [`Self::insert`])
  /// builds past [`INDEX_THRESHOLD`].
  #[inline]
  pub(crate) fn push_owned(
    &mut self,
    name: impl AsRef<str>,
    value: impl AsRef<str>,
  ) {
    let pair = self.append_pair(name.as_ref(), value.as_ref());
    self.fields.push(pair);
  }

  /// Rebuild the lowercase-hash → first-index map from `fields` (source of truth).
  ///
  /// With few fields, leaves the map unset: a linear scan costs less than building
  /// a [`HashMap`] for the usual 2–4 header response.
  pub(crate) fn rebuild_index(&mut self) {
    if self.fields.len() < INDEX_THRESHOLD {
      self.index = None;
      return;
    }
    let Self { buf, fields, index } = self;
    let map = Arc::make_mut(index.get_or_insert_with(|| Arc::new(HashMap::with_capacity(fields.len()))));
    map.clear();
    map.reserve(fields.len());
    for (i, span) in fields.iter().enumerate() {
      let key = ascii_lower_hash(str_from_buf(buf, span.name_start, span.name_len));
      map.entry(key).or_insert(i);
    }
  }

  /// Build the side-index on first mutating use when past [`INDEX_THRESHOLD`].
  ///
  /// No-op when already present or the field count is still small.
  /// (`get`/`contains` stay `&self` + `Sync`, so they linear-scan while deferred.)
  #[inline]
  fn ensure_index(&mut self) {
    if self.index.is_none() && self.fields.len() >= INDEX_THRESHOLD {
      self.rebuild_index();
    }
  }

  #[inline]
  fn push_str(
    &mut self,
    name: &str,
    value: &str,
  ) {
    let idx = self.fields.len();
    let span = self.append_pair(name, value);
    if let Some(map) = self.index.as_mut() {
      let key = ascii_lower_hash(name);
      Arc::make_mut(map).entry(key).or_insert(idx);
      self.fields.push(span);
    } else {
      // No index yet (small / deferred). Keep linear until we cross the threshold.
      self.fields.push(span);
      if self.fields.len() >= INDEX_THRESHOLD {
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
    self.ensure_index();
    let name_ref = name.as_ref();
    let value_ref = value.as_ref();
    let mut first: Option<usize> = None;
    let mut removed = false;
    let mut i = 0usize;
    while i < self.fields.len() {
      let is_match = self
        .fields
        .get(i)
        .is_some_and(|span| self.name_str(span).eq_ignore_ascii_case(name_ref));
      if is_match {
        if first.is_none() {
          first = Some(i);
          i = i.saturating_add(1);
        } else {
          self.fields.remove(i);
          removed = true;
        }
      } else {
        i = i.saturating_add(1);
      }
    }
    if let Some(idx) = first {
      // Keep the existing name span; only append the new value (Host / defaults hot path).
      let (value_start, value_len) = self.append_str(value_ref);
      if let Some(slot) = self.fields.get_mut(idx) {
        slot.value_start = value_start;
        slot.value_len = value_len;
      }
      if removed {
        // Indices after removals shifted — rebuild.
        self.rebuild_index();
      }
      // else: same first-index slot; name bytes (and lowercase hash) unchanged.
    } else {
      let idx = self.fields.len();
      let key = ascii_lower_hash(name_ref);
      let pair = self.append_pair(name_ref, value_ref);
      self.fields.push(pair);
      if let Some(map) = self.index.as_mut() {
        Arc::make_mut(map).entry(key).or_insert(idx);
      } else if self.fields.len() >= INDEX_THRESHOLD {
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
      let h = ascii_lower_hash(name);
      match map.get(&h) {
        Some(&idx) => {
          if let Some(span) = self.fields.get(idx)
            && self.name_str(span).eq_ignore_ascii_case(name)
          {
            return Some(self.value_str(span));
          }
          // Hash collision with a different name — fall through to linear scan.
        },
        None => return None,
      }
    }
    self
      .fields
      .iter()
      .find(|span| self.name_str(span).eq_ignore_ascii_case(name))
      .map(|span| self.value_str(span))
  }

  /// All values for `name` (case-insensitive).
  #[must_use]
  pub fn get_all(
    &self,
    name: &str,
  ) -> Vec<&str> {
    let mut out = Vec::new();
    for span in &self.fields {
      if self.name_str(span).eq_ignore_ascii_case(name) {
        out.push(self.value_str(span));
      }
    }
    out
  }

  /// Iterate values for `name` without allocating (case-insensitive, insertion order).
  /// Used by `cookie-jar` store path; kept available for unit tests when that feature is off.
  #[cfg_attr(not(feature = "cookie-jar"), allow(dead_code))]
  #[inline]
  pub(crate) fn values<'a>(
    &'a self,
    name: &'a str,
  ) -> impl Iterator<Item = &'a str> + 'a {
    self
      .fields
      .iter()
      .filter(|&span| self.name_str(span).eq_ignore_ascii_case(name))
      .map(|span| self.value_str(span))
  }

  /// Whether any field matches `name` (case-insensitive).
  #[must_use]
  pub fn contains(
    &self,
    name: &str,
  ) -> bool {
    if let Some(map) = self.index.as_ref() {
      let h = ascii_lower_hash(name);
      match map.get(&h) {
        Some(&idx) => {
          if let Some(span) = self.fields.get(idx)
            && self.name_str(span).eq_ignore_ascii_case(name)
          {
            return true;
          }
        },
        None => return false,
      }
    }
    self
      .fields
      .iter()
      .any(|span| self.name_str(span).eq_ignore_ascii_case(name))
  }

  /// Remove every field matching `name` (case-insensitive).
  pub fn remove(
    &mut self,
    name: &str,
  ) {
    let before = self.fields.len();
    {
      let Self { buf, fields, .. } = self;
      fields.retain(|span| !str_from_buf(buf, span.name_start, span.name_len).eq_ignore_ascii_case(name));
    }
    if self.fields.len() == before {
      // No removal; still promote a deferred index once past the threshold.
      self.ensure_index();
    } else {
      self.rebuild_index();
    }
  }

  /// Append to `Cookie` (`; `-joined), or insert if absent. No-op when `value` is empty.
  ///
  /// Internal builder/client helper; not a public header-map API.
  pub(crate) fn merge_cookie(
    &mut self,
    value: &str,
  ) {
    if value.is_empty() {
      return;
    }
    if let Some(idx) = self
      .fields
      .iter()
      .position(|span| self.name_str(span).eq_ignore_ascii_case(Self::COOKIE))
    {
      // Snapshot old value bytes before COW (cannot borrow arena across `extend_parts`).
      let old_bytes = self
        .fields
        .get(idx)
        .map(|s| self.value_str(s).as_bytes().to_vec());
      if let Some(old) = old_bytes {
        let add = old.len().saturating_add(2).saturating_add(value.len());
        let value_start = self.extend_parts(&[&old, b"; ", value.as_bytes()]);
        if let Some(slot) = self.fields.get_mut(idx) {
          slot.value_start = value_start;
          slot.value_len = usize_to_u32(add);
        }
      }
    } else {
      self.insert(Self::COOKIE, value);
    }
  }

  /// Iterate `(name, value)` pairs in insertion order.
  #[must_use]
  pub const fn iter(&self) -> Iter<'_> {
    Iter { headers: self, idx: 0 }
  }

  /// Number of fields (including duplicate names).
  #[must_use]
  pub const fn len(&self) -> usize {
    self.fields.len()
  }

  /// `true` when there are no fields.
  #[must_use]
  pub const fn is_empty(&self) -> bool {
    self.fields.is_empty()
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

  #[inline]
  fn name_str(
    &self,
    span: &FieldSpan,
  ) -> &str {
    self.str_at(span.name_start, span.name_len)
  }

  #[inline]
  fn value_str(
    &self,
    span: &FieldSpan,
  ) -> &str {
    self.str_at(span.value_start, span.value_len)
  }

  #[inline]
  fn str_at(
    &self,
    start: u32,
    len: u32,
  ) -> &str {
    let start_usize = start as usize;
    let end = start_usize.saturating_add(len as usize);
    let bytes = self.buf.get(start_usize..end).unwrap_or(&[]);
    // SAFETY: arena only receives UTF-8 (`str` / ASCII wire / lossy UTF-8).
    unsafe { core::str::from_utf8_unchecked(bytes) }
  }

  #[inline]
  fn append_str(
    &mut self,
    s: &str,
  ) -> (u32, u32) {
    self.append_bytes(s.as_bytes())
  }

  #[inline]
  fn append_bytes(
    &mut self,
    bytes: &[u8],
  ) -> (u32, u32) {
    let start = self.extend_parts(&[bytes]);
    (start, usize_to_u32(bytes.len()))
  }

  /// Append `name` then `value` in one COW/extend (single uniqueness check).
  #[inline]
  fn append_pair(
    &mut self,
    name: &str,
    value: &str,
  ) -> FieldSpan {
    let start = self.extend_parts(&[name.as_bytes(), value.as_bytes()]);
    let name_len = name.len();
    let start_usize = start as usize;
    FieldSpan {
      name_start: start,
      name_len: usize_to_u32(name_len),
      value_start: usize_to_u32(start_usize.saturating_add(name_len)),
      value_len: usize_to_u32(value.len()),
    }
  }

  /// Extend the arena; returns the start offset of the appended bytes.
  ///
  /// Copy-on-write only when `buf` is shared. Shared + dead wire bytes: pack
  /// live name/value spans into a tight buffer (cheaper than memcpy of the full
  /// section). Shared + already packed: one memcpy of the live view. Unique:
  /// mutate in place via [`Bytes::try_into_mut`].
  fn extend_parts(
    &mut self,
    parts: &[&[u8]],
  ) -> u32 {
    let add: usize = parts.iter().map(|p| p.len()).sum();
    if add == 0 {
      return usize_to_u32(self.buf.len());
    }
    let buf = core::mem::replace(&mut self.buf, Bytes::new());
    let mut mut_buf = match buf.try_into_mut() {
      Ok(b) => b,
      Err(shared) => self.cow_arena(&shared, add),
    };
    let start = mut_buf.len();
    mut_buf.reserve(add);
    for part in parts {
      mut_buf.extend_from_slice(part);
    }
    self.buf = mut_buf.freeze();
    usize_to_u32(start)
  }

  /// Build a uniquely owned arena from a shared `Bytes` view, reserving `extra`.
  fn cow_arena(
    &mut self,
    shared: &Bytes,
    extra: usize,
  ) -> BytesMut {
    let live: usize = self
      .fields
      .iter()
      .map(|s| (s.name_len as usize).saturating_add(s.value_len as usize))
      .sum();
    if live < shared.len() {
      // Drop dead status-line / CRLF / rewritten-value bytes; remap spans.
      let mut out = BytesMut::with_capacity(live.saturating_add(extra));
      for span in &mut self.fields {
        let name = str_from_buf(shared, span.name_start, span.name_len).as_bytes();
        let value = str_from_buf(shared, span.value_start, span.value_len).as_bytes();
        let name_start = out.len();
        out.extend_from_slice(name);
        let value_start = out.len();
        out.extend_from_slice(value);
        *span = FieldSpan {
          name_start: usize_to_u32(name_start),
          name_len: usize_to_u32(name.len()),
          value_start: usize_to_u32(value_start),
          value_len: usize_to_u32(value.len()),
        };
      }
      // Field indices unchanged → existing side-index stays valid.
      out
    } else {
      // Already packed: one memcpy of the live view; spans stay put.
      let mut out = BytesMut::with_capacity(shared.len().saturating_add(extra));
      out.extend_from_slice(shared);
      out
    }
  }
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
      out.push_owned(name.as_ref(), value.as_ref());
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
      self.push_str(name.as_ref(), value.as_ref());
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
  buf: Bytes,
  fields: alloc::vec::IntoIter<FieldSpan>,
}

impl Iterator for IntoIter {
  type Item = (String, String);

  fn next(&mut self) -> Option<Self::Item> {
    let span = self.fields.next()?;
    let name = str_from_buf(&self.buf, span.name_start, span.name_len);
    let value = str_from_buf(&self.buf, span.value_start, span.value_len);
    Some((String::from(name), String::from(value)))
  }

  fn size_hint(&self) -> (usize, Option<usize>) {
    self.fields.size_hint()
  }
}

impl ExactSizeIterator for IntoIter {
  fn len(&self) -> usize {
    self.fields.len()
  }
}

impl IntoIterator for Headers {
  type Item = (String, String);
  type IntoIter = IntoIter;

  fn into_iter(self) -> Self::IntoIter {
    IntoIter {
      buf: self.buf,
      fields: self.fields.into_iter(),
    }
  }
}

#[inline]
fn str_from_buf(
  buf: &Bytes,
  start: u32,
  len: u32,
) -> &str {
  let start_usize = start as usize;
  let end = start_usize.saturating_add(len as usize);
  let bytes = buf.get(start_usize..end).unwrap_or(&[]);
  // SAFETY: arena only receives UTF-8.
  unsafe { core::str::from_utf8_unchecked(bytes) }
}

#[cfg(test)]
mod tests {
  #![allow(clippy::unwrap_used, clippy::expect_used)]
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
  fn values_iterates_duplicates_without_collect() {
    let mut h = Headers::new();
    h.insert("Set-Cookie", "a=1");
    h.insert("X-Other", "z");
    h.insert("Set-Cookie", "b=2");
    let mut it = h.values("set-cookie");
    assert_eq!(it.next(), Some("a=1"));
    assert_eq!(it.next(), Some("b=2"));
    assert_eq!(it.next(), None);
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
    h.push_owned("Host", "a");
    h.push_owned("X-A", "1");
    h.rebuild_index();
    assert_eq!(h.get("host"), Some("a"));
    h.insert("X-B", "2");
    assert_eq!(h.get("host"), Some("a"));
    assert_eq!(h.get("x-a"), Some("1"));
    assert_eq!(h.get("x-b"), Some("2"));
  }

  #[test]
  fn deferred_index_after_batch_push_owned() {
    // Wire materialize leaves index unset; get/contains still work (linear).
    let mut h = Headers::with_capacity(INDEX_THRESHOLD);
    for i in 0..INDEX_THRESHOLD {
      h.push_owned(alloc::format!("X-{i}"), "1");
    }
    assert!(h.index.is_none());
    assert_eq!(h.get("x-0"), Some("1"));
    assert!(h.contains("x-7"));
    // First mutating API past the threshold builds the index.
    h.set("x-0", "2");
    assert!(h.index.is_some());
    assert_eq!(h.get("x-0"), Some("2"));
  }

  #[test]
  fn arena_preserves_wire_case() {
    let mut h = Headers::with_capacity(2);
    h.push_owned("Content-Type", "text/Plain");
    assert_eq!(h.iter().next(), Some(("Content-Type", "text/Plain")));
    assert_eq!(h.get("content-type"), Some("text/Plain"));
  }

  #[test]
  fn clone_shares_arena_until_mutation() {
    let mut a = Headers::new();
    a.insert("Host", "example.com");
    let b = a.clone();
    assert_eq!(a, b);
    a.set("Host", "other.example");
    assert_eq!(a.get("host"), Some("other.example"));
    assert_eq!(b.get("host"), Some("example.com"));
  }

  #[test]
  fn clone_shares_side_index_cache() {
    let mut a = Headers::new();
    for i in 0..INDEX_THRESHOLD {
      a.insert(alloc::format!("X-{i}"), "1");
    }
    assert!(a.index.is_some());
    let b = a.clone();
    assert!(b.index.is_some());
    // Same Arc — clone must not deep-copy the map (Drop/Callgrind sensitive).
    assert!(Arc::ptr_eq(a.index.as_ref().unwrap(), b.index.as_ref().unwrap()));
    assert_eq!(b.get("x-0"), Some("1"));
    assert_eq!(a, b);
  }

  #[test]
  fn shared_mutation_packs_dead_arena_bytes() {
    // Simulate a wire section: dead prefix + live name/value (shared via clone).
    let mut wire = BytesMut::new();
    wire.extend_from_slice(b"HTTP/1.1 200 OK\r\n");
    let name_start = wire.len();
    wire.extend_from_slice(b"Host");
    let value_start = wire.len();
    wire.extend_from_slice(b"example.com");
    wire.extend_from_slice(b"\r\n\r\n");
    let buf = wire.freeze();
    let live = "Host".len() + "example.com".len();
    assert!(buf.len() > live);

    let mut a = Headers::from_spans(
      buf,
      alloc::vec![(
        usize_to_u32(name_start),
        usize_to_u32(4),
        usize_to_u32(value_start),
        usize_to_u32("example.com".len()),
      )],
    );
    let _keep_shared = a.clone();
    let before = a.arena_len();
    a.set("Host", "other.example");
    // Packed live fields + new value only — no status line / CRLFs retained.
    assert!(a.arena_len() < before);
    assert_eq!(a.get("host"), Some("other.example"));
    assert_eq!(a.iter().next(), Some(("Host", "other.example")));
  }

  #[test]
  fn set_keeps_name_span_on_replace() {
    let mut a = Headers::new();
    a.insert("Content-Type", "text/plain");
    let _shared = a.clone();
    a.set("content-type", "application/json");
    // Wire name casing preserved; only the value was rewritten.
    assert_eq!(a.iter().next(), Some(("Content-Type", "application/json")));
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
