use alloc::string::String;
use alloc::vec::Vec;
use core::hash::{Hash, Hasher};

/// Ordered list of `(name, value)` header fields.
///
/// Names and values live in one packed UTF-8 byte arena (offsets per field).
/// Lookups scan fields linearly with a length prefilter (no side index: for
/// realistic field counts it is as fast and costs no memory). Iteration and
/// multi-value order follow insertion order.
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
#[derive(Debug, Default)]
pub struct Headers {
  /// Contiguous UTF-8 name/value bytes (no CRLF / `": "` / status-line dead
  /// bytes). Parsed responses get an exact-capacity arena; inserts grow it
  /// amortized in place (no refcount / copy-on-write bookkeeping).
  buf: Vec<u8>,
  /// Insertion-ordered fields as offsets into `buf` (inline until [`INLINE_FIELDS`]).
  fields: FieldList,
}

/// Name/value span into [`Headers::buf`] (`u32` offsets; header sections are << 4 GiB).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct FieldSpan {
  pub(crate) name_start: u32,
  pub(crate) name_len: u32,
  pub(crate) value_start: u32,
  pub(crate) value_len: u32,
}

impl FieldSpan {
  #[inline]
  pub(crate) const fn from_offsets(
    name_start: u32,
    name_len: u32,
    value_start: u32,
    value_len: u32,
  ) -> Self {
    Self {
      name_start,
      name_len,
      value_start,
      value_len,
    }
  }

  const fn empty() -> Self {
    Self::from_offsets(0, 0, 0, 0)
  }

  /// Name bytes when the span's offsets are relative to `base`.
  #[inline]
  pub(crate) fn name_in<'a>(
    &self,
    base: &'a [u8],
  ) -> &'a [u8] {
    sub(base, self.name_start, self.name_len)
  }

  /// Value bytes when the span's offsets are relative to `base`.
  #[inline]
  pub(crate) fn value_in<'a>(
    &self,
    base: &'a [u8],
  ) -> &'a [u8] {
    sub(base, self.value_start, self.value_len)
  }
}

#[inline]
fn sub(
  base: &[u8],
  start: u32,
  len: u32,
) -> &[u8] {
  let s = start as usize;
  base.get(s..s.saturating_add(len as usize)).unwrap_or(&[])
}

/// Typical responses have a handful of fields; keep those off the heap.
const INLINE_FIELDS: usize = 8;

#[inline]
#[allow(clippy::cast_lossless)]
const fn u8_as_usize(n: u8) -> usize {
  // `From` is not const-stable; u8 → usize is lossless.
  n as usize
}

/// Insertion-ordered field spans: inline until [`INLINE_FIELDS`], then a heap [`Vec`].
///
/// `len == HEAP` means `heap` is live (including an empty reserved `Vec` from
/// [`FieldList::with_capacity`]).
#[derive(Debug, Clone)]
pub(crate) struct FieldList {
  inline: [FieldSpan; INLINE_FIELDS],
  len: u8,
  heap: Vec<FieldSpan>,
}

/// Marker: `heap` is the source of truth (not `inline[..len]`).
const HEAP: u8 = u8::MAX;

impl Default for FieldList {
  fn default() -> Self {
    Self::new()
  }
}

impl FieldList {
  pub(crate) const fn new() -> Self {
    Self {
      inline: [FieldSpan::empty(); INLINE_FIELDS],
      len: 0,
      heap: Vec::new(),
    }
  }

  pub(crate) fn with_capacity(capacity: usize) -> Self {
    if capacity > INLINE_FIELDS {
      Self {
        inline: [FieldSpan::empty(); INLINE_FIELDS],
        len: HEAP,
        heap: Vec::with_capacity(capacity),
      }
    } else {
      Self::new()
    }
  }

  #[inline]
  const fn on_heap(&self) -> bool {
    self.len == HEAP
  }

  #[inline]
  pub(crate) const fn len(&self) -> usize {
    if self.on_heap() {
      self.heap.len()
    } else {
      u8_as_usize(self.len)
    }
  }

  #[inline]
  pub(crate) const fn is_empty(&self) -> bool {
    self.len() == 0
  }

  #[inline]
  fn as_slice(&self) -> &[FieldSpan] {
    if self.on_heap() {
      self.heap.as_slice()
    } else {
      self.inline.get(..usize::from(self.len)).unwrap_or(&[])
    }
  }

  #[inline]
  fn as_mut_slice(&mut self) -> &mut [FieldSpan] {
    if self.on_heap() {
      self.heap.as_mut_slice()
    } else {
      let n = usize::from(self.len);
      self.inline.get_mut(..n).unwrap_or(&mut [])
    }
  }

  #[inline]
  pub(crate) fn get(
    &self,
    i: usize,
  ) -> Option<&FieldSpan> {
    self.as_slice().get(i)
  }

  #[inline]
  fn get_mut(
    &mut self,
    i: usize,
  ) -> Option<&mut FieldSpan> {
    self.as_mut_slice().get_mut(i)
  }

  #[inline]
  pub(crate) fn iter(&self) -> core::slice::Iter<'_, FieldSpan> {
    self.as_slice().iter()
  }

  #[inline]
  pub(crate) fn iter_mut(&mut self) -> core::slice::IterMut<'_, FieldSpan> {
    self.as_mut_slice().iter_mut()
  }

  /// Append inline without spilling; `false` when the inline array is full.
  #[inline]
  pub(crate) fn try_push_inline(
    &mut self,
    span: FieldSpan,
  ) -> bool {
    if self.on_heap() {
      return false;
    }
    match self.inline.get_mut(usize::from(self.len)) {
      Some(slot) => {
        *slot = span;
        self.len += 1;
        true
      },
      None => false,
    }
  }

  /// Move an inline list to one exact heap `Vec` with `extra` more spans.
  pub(crate) fn spill_exact(
    &mut self,
    extra_a: &[FieldSpan],
    extra_b: &[FieldSpan],
  ) {
    if self.on_heap() {
      self.heap.extend_from_slice(extra_a);
      self.heap.extend_from_slice(extra_b);
      return;
    }
    let n = usize::from(self.len);
    let total = n
      .saturating_add(extra_a.len())
      .saturating_add(extra_b.len());
    self.heap.reserve_exact(total);
    self
      .heap
      .extend_from_slice(self.inline.get(..n).unwrap_or(&[]));
    self.heap.extend_from_slice(extra_a);
    self.heap.extend_from_slice(extra_b);
    self.len = HEAP;
  }

  fn spill(&mut self) {
    if self.on_heap() {
      return;
    }
    let n = usize::from(self.len);
    self.heap.reserve(n.saturating_add(4));
    self
      .heap
      .extend_from_slice(self.inline.get(..n).unwrap_or(&[]));
    self.len = HEAP;
  }

  pub(crate) fn push(
    &mut self,
    span: FieldSpan,
  ) {
    if !self.on_heap() && usize::from(self.len) < INLINE_FIELDS {
      if let Some(slot) = self.inline.get_mut(usize::from(self.len)) {
        *slot = span;
        self.len = self.len.saturating_add(1);
      }
      return;
    }
    self.spill();
    self.heap.push(span);
  }

  fn remove(
    &mut self,
    i: usize,
  ) {
    if self.on_heap() {
      if i < self.heap.len() {
        let _ = self.heap.remove(i);
      }
      return;
    }
    let n = usize::from(self.len);
    if i >= n {
      return;
    }
    let mut src = i.saturating_add(1);
    while src < n {
      let val = self.inline.get(src).copied().unwrap_or(FieldSpan::empty());
      if let Some(dst) = self.inline.get_mut(src.saturating_sub(1)) {
        *dst = val;
      }
      src = src.saturating_add(1);
    }
    self.len = self.len.saturating_sub(1);
  }

  fn retain<F: FnMut(&FieldSpan) -> bool>(
    &mut self,
    mut keep: F,
  ) {
    if self.on_heap() {
      self.heap.retain(keep);
      return;
    }
    let n = usize::from(self.len);
    let mut w = 0usize;
    let mut r = 0usize;
    while r < n {
      let span = self.inline.get(r).copied().unwrap_or(FieldSpan::empty());
      if keep(&span) {
        if let Some(slot) = self.inline.get_mut(w) {
          *slot = span;
        }
        w = w.saturating_add(1);
      }
      r = r.saturating_add(1);
    }
    self.len = u8::try_from(w).unwrap_or(0);
  }
}

impl core::iter::FromIterator<FieldSpan> for FieldList {
  fn from_iter<I: IntoIterator<Item = FieldSpan>>(iter: I) -> Self {
    let mut list = Self::new();
    for span in iter {
      list.push(span);
    }
    list
  }
}

impl<'a> IntoIterator for &'a FieldList {
  type Item = &'a FieldSpan;
  type IntoIter = core::slice::Iter<'a, FieldSpan>;

  fn into_iter(self) -> Self::IntoIter {
    self.iter()
  }
}

impl<'a> IntoIterator for &'a mut FieldList {
  type Item = &'a mut FieldSpan;
  type IntoIter = core::slice::IterMut<'a, FieldSpan>;

  fn into_iter(self) -> Self::IntoIter {
    self.iter_mut()
  }
}

#[cfg(test)]
impl FieldList {
  const fn is_inline(&self) -> bool {
    !self.on_heap()
  }
}

impl Clone for Headers {
  /// Clones only live name/value bytes: a clone never carries dead bytes from
  /// replaced values or a response body stored in the arena tail.
  fn clone(&self) -> Self {
    let live: usize = self
      .fields
      .iter()
      .map(|s| (s.name_len as usize).saturating_add(s.value_len as usize))
      .sum();
    if live == self.buf.len() {
      return self.clone_raw();
    }
    let mut buf = Vec::with_capacity(live);
    let mut fields = self.fields.clone();
    for span in &mut fields {
      let name = span.name_in(&self.buf);
      let value = span.value_in(&self.buf);
      span.name_start = usize_to_u32(buf.len());
      buf.extend_from_slice(name);
      span.value_start = usize_to_u32(buf.len());
      buf.extend_from_slice(value);
    }
    Self { buf, fields }
  }
}

impl PartialEq for Headers {
  fn eq(
    &self,
    other: &Self,
  ) -> bool {
    // Equality is defined by ordered fields only (not arena layout).
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
    // Must match `PartialEq`: hash fields only, not dead / tail arena bytes.
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

/// Common header names with a branch-only case-insensitive lookup.
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

/// Case-insensitive lookup of a well-known header name (`&str`).
#[must_use]
pub fn well_known_header(name: &str) -> Option<WellKnownHeader> {
  well_known_header_bytes(name.as_bytes())
}

/// Case-insensitive lookup of a well-known header name (raw bytes).
///
/// Length switch + one `eq_ignore_ascii_case` (no hashing, no lowercase copy).
#[must_use]
pub fn well_known_header_bytes(name: &[u8]) -> Option<WellKnownHeader> {
  let (want, hdr): (&[u8], WellKnownHeader) = match name.len() {
    2 => (b"te", WellKnownHeader::Te),
    4 => (b"host", WellKnownHeader::Host),
    6 => match name.first().copied().map(|b| b.to_ascii_lowercase()) {
      Some(b'a') => (b"accept", WellKnownHeader::Accept),
      _ => (b"cookie", WellKnownHeader::Cookie),
    },
    10 => match name.first().copied().map(|b| b.to_ascii_lowercase()) {
      Some(b'c') => (b"connection", WellKnownHeader::Connection),
      Some(b's') => (b"set-cookie", WellKnownHeader::SetCookie),
      _ => (b"user-agent", WellKnownHeader::UserAgent),
    },
    12 => (b"content-type", WellKnownHeader::ContentType),
    14 => (b"content-length", WellKnownHeader::ContentLength),
    15 => (b"accept-encoding", WellKnownHeader::AcceptEncoding),
    16 => (b"content-encoding", WellKnownHeader::ContentEncoding),
    17 => (b"transfer-encoding", WellKnownHeader::TransferEncoding),
    _ => return None,
  };
  if name.eq_ignore_ascii_case(want) {
    Some(hdr)
  } else {
    None
  }
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
      buf: Vec::new(),
      fields: FieldList::new(),
    }
  }

  /// Empty collection with room for `capacity` fields.
  #[must_use]
  pub fn with_capacity(capacity: usize) -> Self {
    Self {
      buf: Vec::new(),
      fields: FieldList::with_capacity(capacity),
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

  /// Adopt a packed arena and field spans (offsets into `buf`).
  ///
  /// Used by the parser: one exact-capacity arena per header section.
  #[must_use]
  pub(crate) const fn from_spans(
    buf: Vec<u8>,
    fields: FieldList,
  ) -> Self {
    Self { buf, fields }
  }

  /// Length of the backing arena (including any dead or tail bytes).
  #[cfg(test)]
  #[must_use]
  pub(crate) const fn arena_len(&self) -> usize {
    self.buf.len()
  }

  /// Whole backing arena (live fields, dead bytes, and any response-body tail).
  #[inline]
  #[must_use]
  pub(crate) fn arena(&self) -> &[u8] {
    &self.buf
  }

  /// Append-only access to the arena tail (a response body stored after the
  /// packed fields). Callers must never truncate below the current length.
  #[inline]
  pub(crate) const fn arena_tail_mut(&mut self) -> &mut Vec<u8> {
    &mut self.buf
  }

  /// Give up the arena allocation (field spans are discarded).
  #[must_use]
  pub(crate) fn into_arena(self) -> Vec<u8> {
    self.buf
  }

  /// Byte-exact clone, including dead / tail bytes (for an owner that keeps
  /// data in the tail, e.g. a response body).
  #[must_use]
  pub(crate) fn clone_raw(&self) -> Self {
    Self {
      buf: self.buf.clone(),
      fields: self.fields.clone(),
    }
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

  /// Append an already-owned field.
  #[inline]
  pub(crate) fn push_owned(
    &mut self,
    name: impl AsRef<str>,
    value: impl AsRef<str>,
  ) {
    let pair = self.append_pair(name.as_ref(), value.as_ref());
    self.fields.push(pair);
  }

  #[inline]
  fn push_str(
    &mut self,
    name: &str,
    value: &str,
  ) {
    let span = self.append_pair(name, value);
    self.fields.push(span);
  }

  /// Replace every value for `name` (case-insensitive) with a single value.
  pub fn set(
    &mut self,
    name: impl AsRef<str>,
    value: impl AsRef<str>,
  ) {
    let name_ref = name.as_ref();
    let value_ref = value.as_ref();
    let mut first: Option<usize> = None;
    let mut i = 0usize;
    while i < self.fields.len() {
      let is_match = self
        .fields
        .get(i)
        .is_some_and(|span| self.name_is(span, name_ref));
      if is_match {
        if first.is_none() {
          first = Some(i);
          i = i.saturating_add(1);
        } else {
          self.fields.remove(i);
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
    } else {
      let pair = self.append_pair(name_ref, value_ref);
      self.fields.push(pair);
    }
  }

  /// First value for `name`, if any (case-insensitive).
  #[must_use]
  pub fn get(
    &self,
    name: &str,
  ) -> Option<&str> {
    self
      .fields
      .iter()
      .find(|span| self.name_is(span, name))
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
      if self.name_is(span, name) {
        out.push(self.value_str(span));
      }
    }
    out
  }

  /// Iterate values for `name` without allocating (case-insensitive, insertion order).
  /// Used by the connection persistence check and the `cookie-jar` store path.
  #[inline]
  pub(crate) fn values<'a>(
    &'a self,
    name: &'a str,
  ) -> impl Iterator<Item = &'a str> + 'a {
    self
      .fields
      .iter()
      .filter(|&span| self.name_is(span, name))
      .map(|span| self.value_str(span))
  }

  /// Whether any field matches `name` (case-insensitive).
  #[must_use]
  pub fn contains(
    &self,
    name: &str,
  ) -> bool {
    self.fields.iter().any(|span| self.name_is(span, name))
  }

  /// Remove every field matching `name` (case-insensitive).
  pub fn remove(
    &mut self,
    name: &str,
  ) {
    let Self { buf, fields } = self;
    fields.retain(|span| !name_matches(buf, span, name));
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
      .position(|span| self.name_is(span, Self::COOKIE))
    {
      if let Some(span) = self.fields.get(idx).copied() {
        let old_start = span.value_start as usize;
        let old_end = old_start.saturating_add(span.value_len as usize);
        let add = (span.value_len as usize)
          .saturating_add(2)
          .saturating_add(value.len());
        let value_start = usize_to_u32(self.buf.len());
        self.buf.reserve(add);
        // Old value already lives in the arena: copy within (no temp Vec).
        if old_end <= self.buf.len() {
          self.buf.extend_from_within(old_start..old_end);
        }
        self.buf.extend_from_slice(b"; ");
        self.buf.extend_from_slice(value.as_bytes());
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

  /// Case-insensitive name match; length is checked from the span first so
  /// non-matching fields cost a compare, not a slice + byte loop.
  #[inline]
  fn name_is(
    &self,
    span: &FieldSpan,
    name: &str,
  ) -> bool {
    name_matches(&self.buf, span, name)
  }

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
  fn extend_parts(
    &mut self,
    parts: &[&[u8]],
  ) -> u32 {
    let start = usize_to_u32(self.buf.len());
    let add: usize = parts.iter().map(|p| p.len()).sum();
    if add == 0 {
      return start;
    }
    if self.buf.capacity() == 0 {
      // First field: a little headroom so a typical request builder's handful of
      // headers lands in one allocation instead of 3–4 doubling steps.
      self.buf.reserve(add.max(96));
    } else {
      self.buf.reserve(add);
    }
    for part in parts {
      self.buf.extend_from_slice(part);
    }
    start
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
  buf: Vec<u8>,
  fields: FieldList,
  idx: usize,
}

impl Iterator for IntoIter {
  type Item = (String, String);

  fn next(&mut self) -> Option<Self::Item> {
    let span = *self.fields.get(self.idx)?;
    self.idx = self.idx.saturating_add(1);
    let name = str_from_buf(&self.buf, span.name_start, span.name_len);
    let value = str_from_buf(&self.buf, span.value_start, span.value_len);
    Some((String::from(name), String::from(value)))
  }

  fn size_hint(&self) -> (usize, Option<usize>) {
    let rem = self.fields.len().saturating_sub(self.idx);
    (rem, Some(rem))
  }
}

impl ExactSizeIterator for IntoIter {
  fn len(&self) -> usize {
    self.fields.len().saturating_sub(self.idx)
  }
}

impl IntoIterator for Headers {
  type Item = (String, String);
  type IntoIter = IntoIter;

  fn into_iter(self) -> Self::IntoIter {
    IntoIter {
      buf: self.buf,
      fields: self.fields,
      idx: 0,
    }
  }
}

#[inline]
fn name_matches(
  buf: &[u8],
  span: &FieldSpan,
  name: &str,
) -> bool {
  span.name_len as usize == name.len() && span.name_in(buf).eq_ignore_ascii_case(name.as_bytes())
}

#[inline]
fn str_from_buf(
  buf: &[u8],
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
  fn typical_field_counts_stay_inline() {
    let mut h = Headers::new();
    for i in 0..INLINE_FIELDS {
      h.insert(alloc::format!("H{i}"), "v");
    }
    assert!(h.fields.is_inline());
    assert_eq!(h.len(), INLINE_FIELDS);
    h.insert("overflow", "v");
    assert!(!h.fields.is_inline());
    assert_eq!(h.len(), INLINE_FIELDS.saturating_add(1));
  }

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
    for i in 0..INLINE_FIELDS {
      a.insert(alloc::format!("X-{i}"), "1");
    }
    // Different arena layout (rebuilt via FromIterator) — Hash must still match Eq.
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
  fn many_fields_lookup_linear() {
    let mut h = Headers::with_capacity(4);
    for i in 0..40 {
      h.push_owned(alloc::format!("X-{i}"), "1");
    }
    h.insert("Host", "a");
    assert_eq!(h.get("x-0"), Some("1"));
    assert!(h.contains("x-39"));
    assert_eq!(h.get("HOST"), Some("a"));
    h.set("x-0", "2");
    assert_eq!(h.get("x-0"), Some("2"));
    h.remove("x-5");
    assert!(!h.contains("x-5"));
    assert_eq!(h.len(), 40);
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

#[cfg(test)]
mod clone_tests {
  #![allow(clippy::unwrap_used)]
  use super::*;

  #[test]
  fn clone_drops_dead_and_tail_bytes() {
    let mut h = Headers::new();
    h.insert("A", "1");
    h.set("A", "2222");
    h.arena_tail_mut().extend_from_slice(b"BODY-BYTES");
    let c = h.clone();
    assert_eq!(c.get("a"), Some("2222"));
    assert_eq!(c.arena_len(), "A".len() + "2222".len());
    assert_eq!(h.clone_raw().arena_len(), h.arena_len());
    assert_eq!(c, h);
  }
}
