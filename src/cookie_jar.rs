use crate::sync::Mutex;
use alloc::borrow::Cow;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::error::ParseError;
use crate::parser::cookie::SetCookie;
use crate::parser::uri::{Host, Uri};

pub use crate::parser::cookie::SameSite;

/// Cookie age cap (RFC 10025 §5.5): 400 days in seconds.
const COOKIE_AGE_LIMIT_SECS: u64 = 400 * 24 * 60 * 60;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// Cookie entry in [`CookieStore`].
pub struct StoredCookie {
  name: String,
  value: String,
  domain: String,
  path: String,
  secure: bool,
  http_only: bool,
  host_only: bool,
  same_site: SameSite,
  creation_time: u64,
  expiry_time: Option<u64>,
}

impl StoredCookie {
  /// Cookie name.
  #[must_use]
  pub fn name(&self) -> &str {
    &self.name
  }

  /// Cookie value.
  #[must_use]
  pub fn value(&self) -> &str {
    &self.value
  }

  /// Domain attribute (lowercase).
  #[must_use]
  pub fn domain(&self) -> &str {
    &self.domain
  }

  /// Path attribute.
  #[must_use]
  pub fn path(&self) -> &str {
    &self.path
  }

  /// Send only on HTTPS (`Secure`).
  #[must_use]
  pub const fn secure(&self) -> bool {
    self.secure
  }

  /// `HttpOnly` attribute (stored; all jar retrievals are HTTP).
  #[must_use]
  pub const fn http_only(&self) -> bool {
    self.http_only
  }

  /// Match the exact host only (`host-only`).
  #[must_use]
  pub const fn host_only(&self) -> bool {
    self.host_only
  }

  /// `SameSite` attribute (RFC 10025). Browser cross-site send rules are not applied;
  /// matching cookies are always attached on domain/path/`Secure` match.
  #[must_use]
  pub const fn same_site(&self) -> SameSite {
    self.same_site
  }

  /// Logical creation counter (sort key).
  #[must_use]
  pub const fn creation_time(&self) -> u64 {
    self.creation_time
  }

  /// Expiry as Unix seconds (UTC); `None` = session cookie.
  #[must_use]
  pub const fn expiry_time(&self) -> Option<u64> {
    self.expiry_time
  }
}

/// Domain-keyed cookie map: `BTreeMap<domain, small Vec>` plus jar-wide length.
///
/// Request matching only probes the request host and its DNS parent suffixes.
/// Insert/replace scans only the target domain's vec (one pass).
#[derive(Debug)]
struct CookieMap {
  by_domain: BTreeMap<String, Vec<StoredCookie>>,
  len: usize,
}

impl CookieMap {
  const fn new() -> Self {
    Self {
      by_domain: BTreeMap::new(),
      len: 0,
    }
  }

  const fn len(&self) -> usize {
    self.len
  }

  fn clear(&mut self) {
    self.by_domain.clear();
    self.len = 0;
  }
}

/// Mutex-backed RFC 10025 cookie jar (domain/path match, expiry, `Secure`, prefixes).
///
/// # Examples
///
/// ```
/// use barehttp::cookie_jar::CookieStore;
///
/// let store = CookieStore::new();
/// store.store_response_cookies("http://example.com/", ["id=1; Path=/"])?;
/// assert_eq!(store.request_cookie_header("http://example.com/"), "id=1");
/// # Ok::<(), barehttp::ParseError>(())
/// ```
#[derive(Debug)]
pub struct CookieStore {
  cookies: Mutex<CookieMap>,
}

impl CookieStore {
  /// Empty store.
  #[must_use]
  pub const fn new() -> Self {
    Self {
      cookies: Mutex::new(CookieMap::new()),
    }
  }

  /// Parse `Set-Cookie` values and insert them (RFC 10025 domain/path/`host-only` match).
  ///
  /// `Secure` cookies (and `SameSite=None` / `__Secure-` / `__Host-` rules) are rejected
  /// unless `uri` is `https://`. Malformed values are skipped.
  ///
  /// # Errors
  /// [`ParseError::InvalidUri`] if `uri` is not a usable absolute HTTP(S) URI
  /// (same failure as [`Uri::parse`] / missing authority).
  pub fn store_response_cookies(
    &self,
    uri: &str,
    set_cookie_headers: impl IntoIterator<Item = impl AsRef<str>>,
  ) -> Result<(), ParseError> {
    let (request_host, request_path, is_secure) = host_path_secure(uri)?;

    let mut cookies = self.cookies.lock();
    for header_value in set_cookie_headers {
      if let Some(parsed) = SetCookie::parse(header_value.as_ref()) {
        Self::insert_cookie_locked(&mut cookies, parsed, &request_host, &request_path, is_secure);
      }
    }
    Ok(())
  }

  fn insert_cookie_locked(
    cookies: &mut CookieMap,
    cookie: SetCookie,
    request_host: &str,
    request_path: &str,
    request_is_secure: bool,
  ) {
    // RFC 10025 §5.7: reject Secure cookies received over a non-secure channel.
    if cookie.secure && !request_is_secure {
      return;
    }

    // SameSite=None requires Secure.
    if cookie.same_site == SameSite::None && !cookie.secure {
      return;
    }

    let host_only = cookie.domain.is_none();

    let domain = if let Some(domain_attr) = cookie.domain {
      if !domain_attr_acceptable(&domain_attr) {
        return;
      }
      if !domain_matches(request_host, &domain_attr) {
        return;
      }
      domain_attr
    } else {
      request_host.to_ascii_lowercase()
    };

    let path = cookie.path.unwrap_or_else(|| default_path(request_path));

    // Cookie-name prefixes (case-insensitive match on the name).
    if name_has_prefix(&cookie.name, "__Secure-") && !cookie.secure {
      return;
    }
    if name_has_prefix(&cookie.name, "__Host-") && !(cookie.secure && host_only && path == "/") {
      return;
    }
    // Nameless cookie must not mimic a prefix via its value.
    if cookie.name.is_empty()
      && (name_has_prefix(&cookie.value, "__Secure-") || name_has_prefix(&cookie.value, "__Host-"))
    {
      return;
    }

    // Non-secure cookie must not overlay an existing Secure cookie (RFC 10025 §5.7).
    // Related domains only: same key, parents, and children (not the whole jar).
    if !cookie.secure && overlays_secure_cookie(cookies, &cookie.name, &domain, &path) {
      return;
    }

    let now = crate::util::now_unix_secs();
    let age_limit = now.saturating_add(COOKIE_AGE_LIMIT_SECS);

    // RFC 10025: Max-Age wins over Expires; both capped to 400 days.
    let expiry_time = if let Some(max_age) = cookie.max_age {
      if max_age <= 0 {
        Some(0)
      } else {
        let capped = max_age.unsigned_abs().min(COOKIE_AGE_LIMIT_SECS);
        Some(now.saturating_add(capped))
      }
    } else if let Some(expires) = cookie.expires {
      match expires.to_unix_secs() {
        Some(ts) if ts > now => Some(ts.min(age_limit)),
        _ => Some(0),
      }
    } else {
      None
    };

    // Uniqueness: name + domain + host-only + path; one-pass replace in the domain bucket.
    let mut creation = u64::try_from(cookies.len()).unwrap_or(u64::MAX);
    if expiry_time == Some(0) {
      let removed_empty = if let Some(bucket) = cookies.by_domain.get_mut(&domain) {
        if let Some(pos) = bucket
          .iter()
          .position(|c| c.name == cookie.name && c.host_only == host_only && c.path == path)
        {
          bucket.remove(pos);
          cookies.len = cookies.len.saturating_sub(1);
        }
        bucket.is_empty()
      } else {
        false
      };
      if removed_empty {
        cookies.by_domain.remove(&domain);
      }
      return;
    }

    let bucket = cookies.by_domain.entry(domain.clone()).or_default();
    if let Some(pos) = bucket
      .iter()
      .position(|c| c.name == cookie.name && c.host_only == host_only && c.path == path)
    {
      if let Some(old) = bucket.get(pos) {
        creation = old.creation_time;
      }
      if let Some(slot) = bucket.get_mut(pos) {
        *slot = StoredCookie {
          name: cookie.name,
          value: cookie.value,
          domain,
          path,
          secure: cookie.secure,
          http_only: cookie.http_only,
          host_only,
          same_site: cookie.same_site,
          creation_time: creation,
          expiry_time,
        };
      }
      return;
    }

    bucket.push(StoredCookie {
      name: cookie.name,
      value: cookie.value,
      domain,
      path,
      secure: cookie.secure,
      http_only: cookie.http_only,
      host_only,
      same_site: cookie.same_site,
      creation_time: creation,
      expiry_time,
    });
    cookies.len = cookies.len.saturating_add(1);
  }

  /// Cookie header value for `uri` (RFC 10025 path-length / creation-time sort).
  ///
  /// Empty when nothing matches, or when `uri` is not a usable absolute HTTP(S)
  /// URI. Unlike [`Self::store_response_cookies`], invalid URIs return an empty
  /// string (no [`ParseError::InvalidUri`]).
  /// Skips `Secure` cookies unless `uri` uses the `https` scheme (same rule as
  /// store-time rejection of `Secure` over cleartext).
  ///
  /// `SameSite` browser cross-site filtering is not applied (no document context).
  pub fn request_cookie_header(
    &self,
    uri: &str,
  ) -> String {
    let Ok(parsed) = Uri::parse(uri) else {
      return String::new();
    };
    self.cookie_header_for_uri(&parsed)
  }

  /// Same as [`Self::request_cookie_header`], using an already-parsed [`Uri`]
  /// (avoids a second parse on the client send path).
  pub(crate) fn cookie_header_for_uri(
    &self,
    uri: &Uri<'_>,
  ) -> String {
    let Some(auth) = uri.authority() else {
      return String::new();
    };

    // One host normalization for map keys + domain match; path stays borrowed.
    // Skip alloc when the URI host is already lowercase (common case).
    let host_lower: Cow<'_, str> = match auth.host() {
      Host::RegName(name) => ascii_host_key(name),
      Host::IpAddr(addr) => Cow::Owned(crate::util::format_ip_for_host(*addr)),
    };
    let request_path = if uri.path().is_empty() {
      "/"
    } else {
      uri.path()
    };
    let is_secure = uri.scheme().eq_ignore_ascii_case("https");
    let now = crate::util::now_unix_secs();

    let cookies = self.cookies.lock();
    let mut matching = Vec::new();

    // Host-only cookies live under the exact host key; Domain cookies under a
    // DNS parent (or the host itself). Keys are `&str` slices into `host_lower`.
    for domain_key in domain_lookup_keys(&host_lower) {
      let Some(bucket) = cookies.by_domain.get(domain_key) else {
        continue;
      };
      for cookie in bucket {
        if let Some(expiry) = cookie.expiry_time
          && expiry <= now
        {
          continue;
        }

        if cookie.secure && !is_secure {
          continue;
        }

        // Stored domains are lowercase; host is already normalized.
        let domain_match = if cookie.host_only {
          host_lower == cookie.domain
        } else {
          domain_matches(&host_lower, &cookie.domain)
        };

        if !domain_match {
          continue;
        }

        if !path_matches(request_path, &cookie.path) {
          continue;
        }

        matching.push(cookie);
      }
    }

    if matching.is_empty() {
      return String::new();
    }

    if matching.len() > 1 {
      sort_cookies_for_send(&mut matching);
    }

    let mut needed = 0usize;
    for (i, cookie) in matching.iter().enumerate() {
      if i > 0 {
        needed = needed.saturating_add(2); // "; "
      }
      needed = needed
        .saturating_add(cookie.name.len())
        .saturating_add(1) // '='
        .saturating_add(cookie.value.len());
    }

    let mut result = String::with_capacity(needed);
    for (i, cookie) in matching.iter().enumerate() {
      if i > 0 {
        result.push_str("; ");
      }
      result.push_str(&cookie.name);
      result.push('=');
      result.push_str(&cookie.value);
    }

    result
  }

  /// Clear every cookie.
  pub fn clear(&self) {
    self.cookies.lock().clear();
  }

  /// Remove the cookie matching `(name, domain, path)`.
  ///
  /// Returns `true` if one was present.
  pub fn remove(
    &self,
    name: &str,
    domain: &str,
    path: &str,
  ) -> bool {
    let key = domain.to_ascii_lowercase();
    let mut cookies = self.cookies.lock();
    let removed = {
      let Some(bucket) = cookies.by_domain.get_mut(&key) else {
        return false;
      };
      let before = bucket.len();
      bucket.retain(|c| !(c.name == name && c.path == path));
      before.saturating_sub(bucket.len())
    };
    if removed == 0 {
      return false;
    }
    cookies.len = cookies.len.saturating_sub(removed);
    if cookies.by_domain.get(&key).is_some_and(Vec::is_empty) {
      cookies.by_domain.remove(&key);
    }
    true
  }

  /// Iterate stored cookies (including expired). Holds the store lock for the iterator's lifetime.
  #[must_use]
  pub fn iter(&self) -> Iter<'_> {
    Iter {
      guard: self.cookies.lock(),
      domain_idx: 0,
      cookie_idx: 0,
    }
  }
}

/// Iterator over [`StoredCookie`]s in a [`CookieStore`] (holds the store lock).
pub struct Iter<'a> {
  guard: crate::sync::MutexGuard<'a, CookieMap>,
  domain_idx: usize,
  cookie_idx: usize,
}

impl core::fmt::Debug for Iter<'_> {
  fn fmt(
    &self,
    f: &mut core::fmt::Formatter<'_>,
  ) -> core::fmt::Result {
    f.debug_struct("Iter")
      .field("domain_idx", &self.domain_idx)
      .field("cookie_idx", &self.cookie_idx)
      .finish_non_exhaustive()
  }
}

impl<'a> Iterator for Iter<'a> {
  type Item = &'a StoredCookie;

  fn next(&mut self) -> Option<&'a StoredCookie> {
    loop {
      let (_, bucket) = self.guard.by_domain.iter().nth(self.domain_idx)?;
      if let Some(cookie) = bucket.get(self.cookie_idx) {
        self.cookie_idx = self.cookie_idx.saturating_add(1);
        // SAFETY: `guard` is held for `'a` and grants exclusive access to the map.
        // The returned reference points into a bucket vec and does not outlive the guard.
        // `Iter` never mutates the map, so the reference stays valid.
        return Some(unsafe { &*core::ptr::from_ref(cookie) });
      }
      self.domain_idx = self.domain_idx.saturating_add(1);
      self.cookie_idx = 0;
    }
  }

  fn size_hint(&self) -> (usize, Option<usize>) {
    let remaining = remaining_cookies(&self.guard, self.domain_idx, self.cookie_idx);
    (remaining, Some(remaining))
  }
}

impl ExactSizeIterator for Iter<'_> {}

impl<'a> IntoIterator for &'a CookieStore {
  type Item = &'a StoredCookie;
  type IntoIter = Iter<'a>;

  fn into_iter(self) -> Self::IntoIter {
    self.iter()
  }
}

impl Default for CookieStore {
  fn default() -> Self {
    Self::new()
  }
}

/// Alias for [`CookieStore`] (matches the `cookie-jar` feature / module name).
///
/// Stable (not deprecated); prefer [`CookieStore`] in new code.
pub type CookieJar = CookieStore;

fn remaining_cookies(
  map: &CookieMap,
  domain_idx: usize,
  cookie_idx: usize,
) -> usize {
  let mut remaining = 0usize;
  for (i, (_, bucket)) in map.by_domain.iter().enumerate() {
    if i < domain_idx {
      continue;
    }
    if i == domain_idx {
      remaining = remaining.saturating_add(bucket.len().saturating_sub(cookie_idx));
    } else {
      remaining = remaining.saturating_add(bucket.len());
    }
  }
  remaining
}

/// Lowercase ASCII host for map lookup, borrowing when already lowercase.
fn ascii_host_key(host: &str) -> Cow<'_, str> {
  if host.bytes().all(|b| !b.is_ascii_uppercase()) {
    Cow::Borrowed(host)
  } else {
    Cow::Owned(host.to_ascii_lowercase())
  }
}

/// Request-host key plus each DNS parent suffix (`a.b.com` → `a.b.com`, `b.com`, `com`).
///
/// `host_lower` must already be ASCII-lowercased (stored domain keys are). Yields
/// `&str` slices into that buffer — no per-suffix allocations.
const fn domain_lookup_keys(host_lower: &str) -> DomainParentKeys<'_> {
  DomainParentKeys {
    host: host_lower,
    pos: 0,
    yielded_host: false,
  }
}

/// Zero-alloc iterator over a host and its DNS parent suffixes.
struct DomainParentKeys<'a> {
  host: &'a str,
  pos: usize,
  yielded_host: bool,
}

impl<'a> Iterator for DomainParentKeys<'a> {
  type Item = &'a str;

  fn next(&mut self) -> Option<&'a str> {
    if !self.yielded_host {
      self.yielded_host = true;
      return Some(self.host);
    }
    let bytes = self.host.as_bytes();
    while self.pos < bytes.len() {
      let i = self.pos;
      self.pos = i.saturating_add(1);
      if bytes.get(i) == Some(&b'.') {
        let start = i.saturating_add(1);
        if start < bytes.len() {
          return self.host.get(start..);
        }
      }
    }
    None
  }
}

/// Secure-overlay scan limited to domains related to `domain` (RFC 10025 §5.7).
fn overlays_secure_cookie(
  cookies: &CookieMap,
  name: &str,
  domain: &str,
  path: &str,
) -> bool {
  // Same domain + DNS parents.
  for key in domain_lookup_keys(domain) {
    if let Some(bucket) = cookies.by_domain.get(key)
      && bucket
        .iter()
        .any(|c| c.secure && c.name == name && path_matches(path, &c.path))
    {
      return true;
    }
  }
  // Children: domain-match the other way (`www.example.com` under `example.com`).
  for (key, bucket) in &cookies.by_domain {
    if key.as_str() == domain || !domain_matches(key, domain) {
      continue;
    }
    if bucket
      .iter()
      .any(|c| c.secure && c.name == name && path_matches(path, &c.path))
    {
      return true;
    }
  }
  false
}

fn host_path_secure(uri: &str) -> Result<(String, String, bool), ParseError> {
  let parsed = Uri::parse(uri)?;
  let auth = parsed.authority().ok_or(ParseError::InvalidUri)?;
  let host = match auth.host() {
    Host::RegName(name) => String::from(*name),
    Host::IpAddr(addr) => crate::util::format_ip_for_host(*addr),
  };
  let path = if parsed.path().is_empty() {
    String::from("/")
  } else {
    String::from(parsed.path())
  };
  let is_secure = parsed.scheme().eq_ignore_ascii_case("https");
  Ok((host, path, is_secure))
}

fn domain_matches(
  request_host: &str,
  cookie_domain: &str,
) -> bool {
  if request_host.eq_ignore_ascii_case(cookie_domain) {
    return true;
  }

  if request_host.len() <= cookie_domain.len() {
    return false;
  }

  let split = request_host.len() - cookie_domain.len();
  if request_host.as_bytes().get(split.saturating_sub(1)) != Some(&b'.') {
    return false;
  }

  request_host
    .get(split..)
    .is_some_and(|suffix| suffix.eq_ignore_ascii_case(cookie_domain))
}

/// Reject public-suffix-like Domain attrs (no embedded `.`) and IP Domain (RFC 10025).
fn domain_attr_acceptable(domain: &str) -> bool {
  if domain.is_empty() {
    return false;
  }
  if is_ip_host(domain) {
    return false;
  }
  // Minimal PSL guard: require an embedded dot (`Domain=com` → reject).
  domain.contains('.')
}

fn is_ip_host(host: &str) -> bool {
  let bare = host
    .strip_prefix('[')
    .and_then(|h| h.strip_suffix(']'))
    .unwrap_or(host);
  bare.parse::<core::net::IpAddr>().is_ok()
}

fn name_has_prefix(
  name: &str,
  prefix: &str,
) -> bool {
  name.len() >= prefix.len() && name[..prefix.len()].eq_ignore_ascii_case(prefix)
}

fn path_matches(
  request_path: &str,
  cookie_path: &str,
) -> bool {
  if request_path == cookie_path {
    return true;
  }

  if request_path.starts_with(cookie_path) && cookie_path.ends_with('/') {
    return true;
  }

  if request_path.starts_with(cookie_path)
    && let Some(next_char) = request_path.as_bytes().get(cookie_path.len())
  {
    return *next_char == b'/';
  }

  false
}

/// RFC 10025 cookie ordering: longer path first, then earlier creation time.
/// Insertion sort for small jars; avoids `slice::sort` monomorphization.
fn sort_cookies_for_send(cookies: &mut [&StoredCookie]) {
  for i in 1..cookies.len() {
    let mut j = i;
    while j > 0 {
      let Some(prev) = cookies.get(j - 1).copied() else {
        break;
      };
      let Some(cur) = cookies.get(j).copied() else {
        break;
      };
      let swap = cur.path().len() > prev.path().len()
        || (cur.path().len() == prev.path().len() && cur.creation_time() < prev.creation_time());
      if !swap {
        break;
      }
      cookies.swap(j - 1, j);
      j -= 1;
    }
  }
}

fn default_path(request_path: &str) -> String {
  if request_path.matches('/').count() <= 1 {
    return "/".to_string();
  }

  request_path.rfind('/').map_or_else(
    || "/".to_string(),
    |last_slash| {
      if last_slash == 0 {
        "/".to_string()
      } else {
        request_path[..last_slash].to_string()
      }
    },
  )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
  use super::*;

  #[test]
  fn test_host_path_secure() {
    assert_eq!(
      host_path_secure("http://example.com"),
      Ok((String::from("example.com"), String::from("/"), false))
    );
    assert_eq!(
      host_path_secure("https://example.com/path"),
      Ok((String::from("example.com"), String::from("/path"), true))
    );
    assert_eq!(
      host_path_secure("http://example.com:8080/path"),
      Ok((String::from("example.com"), String::from("/path"), false))
    );
    assert_eq!(
      host_path_secure("http://example.com/path?query"),
      Ok((String::from("example.com"), String::from("/path"), false))
    );
    assert_eq!(host_path_secure("not a uri"), Err(ParseError::InvalidUri));
  }

  #[test]
  fn test_domain_matches() {
    assert!(domain_matches("example.com", "example.com"));
    assert!(domain_matches("www.example.com", "example.com"));
    assert!(domain_matches("sub.example.com", "example.com"));
    assert!(!domain_matches("example.com", "www.example.com"));
    assert!(!domain_matches("notexample.com", "example.com"));
  }

  #[test]
  fn domain_lookup_keys_are_suffix_slices() {
    let keys: Vec<&str> = domain_lookup_keys("a.b.example.com").collect();
    assert_eq!(keys, ["a.b.example.com", "b.example.com", "example.com", "com"]);
    assert!(matches!(ascii_host_key("example.com"), Cow::Borrowed(_)));
    assert!(matches!(ascii_host_key("Example.COM"), Cow::Owned(_)));
  }

  #[test]
  fn test_path_matches() {
    assert!(path_matches("/", "/"));
    assert!(path_matches("/path", "/path"));
    assert!(path_matches("/path/sub", "/path"));
    assert!(path_matches("/path/sub", "/path/"));
    assert!(!path_matches("/path", "/path2"));
    assert!(!path_matches("/path", "/pathological"));
  }

  #[test]
  fn test_default_path() {
    assert_eq!(default_path("/"), "/");
    assert_eq!(default_path("/path"), "/");
    assert_eq!(default_path("/path/sub"), "/path");
    assert_eq!(default_path("/path/sub/deep"), "/path/sub");
  }

  #[test]
  fn test_store_and_retrieve_cookie() {
    let store = CookieStore::new();

    let set_cookie = alloc::vec!["session=abc123".to_string()];
    store
      .store_response_cookies("http://example.com/", &set_cookie)
      .expect("uri");

    let cookies = store.request_cookie_header("http://example.com/");
    assert_eq!(cookies, "session=abc123");
  }

  #[test]
  fn store_response_cookies_rejects_invalid_uri() {
    let store = CookieStore::new();
    assert_eq!(
      store.store_response_cookies("not a uri", ["a=1"]),
      Err(ParseError::InvalidUri)
    );
    assert!(store.iter().next().is_none());
  }

  #[test]
  fn test_cookie_path_matching() {
    let store = CookieStore::new();

    let set_cookie = alloc::vec!["id=123; Path=/admin".to_string()];
    store
      .store_response_cookies("http://example.com/admin/panel", &set_cookie)
      .expect("uri");

    let cookies_admin = store.request_cookie_header("http://example.com/admin/panel");
    assert_eq!(cookies_admin, "id=123");

    let cookies_other = store.request_cookie_header("http://example.com/other");
    assert_eq!(cookies_other, "");
  }

  #[test]
  fn test_cookie_domain_matching() {
    let store = CookieStore::new();

    let set_cookie = alloc::vec!["id=123; Domain=example.com".to_string()];
    store
      .store_response_cookies("http://www.example.com/", &set_cookie)
      .expect("uri");

    let cookies_www = store.request_cookie_header("http://www.example.com/");
    assert_eq!(cookies_www, "id=123");

    let cookies_sub = store.request_cookie_header("http://sub.example.com/");
    assert_eq!(cookies_sub, "id=123");

    let cookies_other = store.request_cookie_header("http://other.com/");
    assert_eq!(cookies_other, "");
  }

  #[test]
  fn test_secure_cookie() {
    let store = CookieStore::new();

    let set_cookie = alloc::vec!["token=secret; Secure".to_string()];
    store
      .store_response_cookies("https://example.com/", &set_cookie)
      .expect("uri");

    let cookies_https = store.request_cookie_header("https://example.com/");
    assert_eq!(cookies_https, "token=secret");

    let cookies_http = store.request_cookie_header("http://example.com/");
    assert_eq!(cookies_http, "");
  }

  #[test]
  fn test_secure_cookie_rejected_over_http() {
    let store = CookieStore::new();
    store
      .store_response_cookies("http://example.com/", alloc::vec!["token=secret; Secure".to_string()])
      .expect("uri");
    // Must not store. Later HTTPS must not see a Secure cookie set over cleartext.
    assert_eq!(store.request_cookie_header("https://example.com/"), "");
    assert!(store.iter().next().is_none());
  }

  #[test]
  fn test_cookie_replacement() {
    let store = CookieStore::new();

    store
      .store_response_cookies("http://example.com/", alloc::vec!["id=first".to_string()])
      .expect("uri");
    let cookies_first = store.request_cookie_header("http://example.com/");
    assert_eq!(cookies_first, "id=first");

    store
      .store_response_cookies("http://example.com/", alloc::vec!["id=second".to_string()])
      .expect("uri");
    let cookies_second = store.request_cookie_header("http://example.com/");
    assert_eq!(cookies_second, "id=second");
  }

  #[test]
  fn cookie_send_order_longer_path_first_then_creation() {
    let store = CookieStore::new();
    // Shorter path first in time, then longer path. Wire order must be longer path first.
    store
      .store_response_cookies("http://example.com/a/b", alloc::vec!["a=1; Path=/".to_string()])
      .expect("uri");
    store
      .store_response_cookies("http://example.com/a/b", alloc::vec!["b=2; Path=/a".to_string()])
      .expect("uri");
    store
      .store_response_cookies("http://example.com/a/b", alloc::vec!["c=3; Path=/a/b".to_string()])
      .expect("uri");
    assert_eq!(store.request_cookie_header("http://example.com/a/b"), "c=3; b=2; a=1");
  }

  #[test]
  fn cookie_send_order_same_path_creation_time() {
    let store = CookieStore::new();
    store
      .store_response_cookies("http://example.com/", alloc::vec!["first=1; Path=/".to_string()])
      .expect("uri");
    store
      .store_response_cookies("http://example.com/", alloc::vec!["second=2; Path=/".to_string()])
      .expect("uri");
    assert_eq!(store.request_cookie_header("http://example.com/"), "first=1; second=2");
  }

  #[test]
  fn test_multiple_cookies() {
    let store = CookieStore::new();

    store
      .store_response_cookies(
        "http://example.com/",
        alloc::vec!["session=abc".to_string(), "lang=en".to_string()],
      )
      .expect("uri");

    let cookies = store.request_cookie_header("http://example.com/");
    assert!(cookies.contains("session=abc"));
    assert!(cookies.contains("lang=en"));
  }

  #[test]
  fn expires_in_past_is_not_stored() {
    let store = CookieStore::new();
    store
      .store_response_cookies(
        "http://example.com/",
        alloc::vec!["gone=1; Expires=Thu, 01 Jan 1970 00:00:00 GMT".to_string()],
      )
      .expect("uri");
    assert!(
      store
        .request_cookie_header("http://example.com/")
        .is_empty()
    );
  }

  #[test]
  fn expires_in_future_is_stored() {
    let store = CookieStore::new();
    store
      .store_response_cookies(
        "http://example.com/",
        alloc::vec!["keep=1; Expires=Wed, 09 Jun 2099 10:18:14 GMT".to_string()],
      )
      .expect("uri");
    assert_eq!(store.request_cookie_header("http://example.com/"), "keep=1");
  }

  #[test]
  fn max_age_zero_deletes() {
    let store = CookieStore::new();
    store
      .store_response_cookies("http://example.com/", alloc::vec!["id=1".to_string()])
      .expect("uri");
    assert_eq!(store.request_cookie_header("http://example.com/"), "id=1");
    store
      .store_response_cookies("http://example.com/", alloc::vec!["id=1; Max-Age=0".to_string()])
      .expect("uri");
    assert!(
      store
        .request_cookie_header("http://example.com/")
        .is_empty()
    );
  }

  #[test]
  fn rejects_public_suffix_like_domain() {
    let store = CookieStore::new();
    store
      .store_response_cookies("http://example.com/", alloc::vec!["x=1; Domain=com".to_string()])
      .expect("uri");
    assert!(
      store
        .request_cookie_header("http://example.com/")
        .is_empty()
    );
    assert_eq!(store.iter().len(), 0);
  }

  #[test]
  fn rejects_ip_domain_attribute() {
    let store = CookieStore::new();
    store
      .store_response_cookies("http://192.0.2.1/", alloc::vec!["x=1; Domain=192.0.2.1".to_string()])
      .expect("uri");
    assert_eq!(store.iter().len(), 0);
  }

  #[test]
  fn clear_and_remove() {
    let store = CookieStore::new();
    store
      .store_response_cookies(
        "http://example.com/",
        alloc::vec!["a=1".to_string(), "b=2; Path=/".to_string()],
      )
      .expect("uri");
    assert_eq!(store.iter().len(), 2);
    assert!(store.remove("a", "example.com", "/"));
    assert_eq!(store.iter().len(), 1);
    store.clear();
    assert_eq!(store.iter().len(), 0);
  }

  /// Exercises `CookieStore::Iter`'s unsafe lifetime extension under Miri.
  #[test]
  fn iter_walks_locked_cookies() {
    let store = CookieStore::new();
    store
      .store_response_cookies(
        "http://example.com/",
        alloc::vec!["session=abc".to_string(), "lang=en".to_string()],
      )
      .expect("uri");

    let mut pairs = alloc::vec::Vec::new();
    for cookie in &store {
      pairs.push((cookie.name().to_string(), cookie.value().to_string()));
      assert_eq!(cookie.domain(), "example.com");
      assert_eq!(cookie.path(), "/");
      assert!(!cookie.secure());
      assert!(!cookie.http_only());
      assert!(cookie.host_only());
      assert_eq!(cookie.same_site(), SameSite::Default);
      assert!(cookie.expiry_time().is_none());
    }
    pairs.sort_unstable();
    assert_eq!(
      pairs,
      [
        ("lang".to_string(), "en".to_string()),
        ("session".to_string(), "abc".to_string()),
      ]
    );
    assert_eq!(store.iter().len(), 2);
    assert_eq!(store.iter().count(), 2);
  }

  #[test]
  fn secure_prefix_requires_secure_https() {
    let store = CookieStore::new();
    store
      .store_response_cookies("http://example.com/", ["__Secure-SID=1; Secure"])
      .expect("uri");
    assert_eq!(store.iter().len(), 0);

    store
      .store_response_cookies("https://example.com/", ["__Secure-SID=1; Secure"])
      .expect("uri");
    assert_eq!(store.request_cookie_header("https://example.com/"), "__Secure-SID=1");
  }

  #[test]
  fn host_prefix_requires_secure_host_only_root_path() {
    let store = CookieStore::new();
    store
      .store_response_cookies(
        "https://example.com/",
        ["__Host-SID=1; Secure; Domain=example.com; Path=/"],
      )
      .expect("uri");
    assert_eq!(store.iter().len(), 0);

    store
      .store_response_cookies("https://example.com/", ["__Host-SID=1; Secure; Path=/"])
      .expect("uri");
    assert_eq!(store.request_cookie_header("https://example.com/"), "__Host-SID=1");
  }

  #[test]
  fn samesite_none_requires_secure() {
    let store = CookieStore::new();
    store
      .store_response_cookies("https://example.com/", ["x=1; SameSite=None"])
      .expect("uri");
    assert_eq!(store.iter().len(), 0);

    store
      .store_response_cookies("https://example.com/", ["x=1; SameSite=None; Secure"])
      .expect("uri");
    assert_eq!(store.iter().next().unwrap().same_site(), SameSite::None);
  }

  #[test]
  fn non_secure_cannot_overlay_secure() {
    let store = CookieStore::new();
    store
      .store_response_cookies("https://example.com/", ["a=secret; Secure; Path=/login"])
      .expect("uri");
    store
      .store_response_cookies("http://example.com/", ["a=evil; Path=/login"])
      .expect("uri");
    assert_eq!(store.request_cookie_header("https://example.com/login"), "a=secret");
  }

  #[test]
  fn non_secure_cannot_overlay_secure_child_domain() {
    let store = CookieStore::new();
    store
      .store_response_cookies(
        "https://www.example.com/",
        ["a=secret; Secure; Domain=www.example.com; Path=/"],
      )
      .expect("uri");
    store
      .store_response_cookies("http://www.example.com/", ["a=evil; Domain=example.com; Path=/"])
      .expect("uri");
    assert_eq!(store.request_cookie_header("https://www.example.com/"), "a=secret");
    assert_eq!(store.iter().len(), 1);
  }

  #[test]
  fn multi_domain_jar_isolates_lookups() {
    let store = CookieStore::new();
    store
      .store_response_cookies("http://a.example.com/", ["a=1"])
      .expect("uri");
    store
      .store_response_cookies("http://b.other.org/", ["b=2"])
      .expect("uri");
    store
      .store_response_cookies("http://c.third.net/", ["c=3"])
      .expect("uri");
    assert_eq!(store.request_cookie_header("http://a.example.com/"), "a=1");
    assert_eq!(store.request_cookie_header("http://b.other.org/"), "b=2");
    assert_eq!(store.request_cookie_header("http://c.third.net/"), "c=3");
    assert_eq!(store.iter().len(), 3);
  }
}
