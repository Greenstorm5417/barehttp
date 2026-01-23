extern crate alloc;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

#[cfg(feature = "cookie-jar")]
use crate::parser::cookie::SetCookie;

#[cfg(feature = "cookie-jar")]
#[derive(Debug, Clone)]
/// Represents a stored HTTP cookie with all RFC 6265 attributes
///
/// This struct contains all information needed to store and match cookies
/// according to RFC 6265 specifications.
pub struct StoredCookie {
  /// Cookie name
  pub name: String,
  /// Cookie value
  pub value: String,
  /// Domain attribute (lowercase)
  pub domain: String,
  /// Path attribute
  pub path: String,
  /// Secure flag - cookie only sent over HTTPS
  pub secure: bool,
  /// `HttpOnly` flag - cookie not accessible via JavaScript
  pub http_only: bool,
  /// Host-only flag - cookie only matches exact host
  pub host_only: bool,
  /// Creation time (logical counter)
  pub creation_time: u64,
  /// Expiry time (logical counter), None means session cookie
  pub expiry_time: Option<u64>,
}

#[cfg(feature = "cookie-jar")]
#[derive(Debug)]
/// Thread-safe RFC 6265 compliant cookie storage
///
/// Automatically handles cookie domain/path matching, expiration,
/// and secure cookie restrictions.
pub struct CookieStore {
  cookies: Vec<StoredCookie>,
  counter: AtomicU64,
}

#[cfg(feature = "cookie-jar")]
impl CookieStore {
  /// Creates a new empty cookie store
  #[must_use]
  pub const fn new() -> Self {
    Self {
      cookies: Vec::new(),
      counter: AtomicU64::new(0),
    }
  }

  fn current_time(&self) -> u64 {
    self.counter.fetch_add(1, Ordering::SeqCst)
  }

  /// Stores cookies from Set-Cookie response headers
  ///
  /// Parses and stores cookies according to RFC 6265 rules, including
  /// domain/path matching and cookie replacement.
  ///
  /// # Arguments
  /// * `uri` - Request URI for domain/path context
  /// * `set_cookie_headers` - Set-Cookie header values from response
  pub fn store_response_cookies(
    &mut self,
    uri: &str,
    set_cookie_headers: &[String],
  ) {
    let Some(request_host) = extract_host_from_uri(uri) else {
      return;
    };

    let request_path = extract_path_from_uri(uri);

    for header_value in set_cookie_headers {
      if let Some(parsed) = SetCookie::parse(header_value) {
        self.insert_cookie(parsed, request_host, &request_path);
      }
    }
  }

  fn insert_cookie(
    &mut self,
    cookie: SetCookie,
    request_host: &str,
    request_path: &str,
  ) {
    let current = self.current_time();

    let host_only = cookie.domain.is_none();

    let domain = if let Some(domain_attr) = cookie.domain {
      if !domain_matches(request_host, &domain_attr) {
        return;
      }
      domain_attr
    } else {
      request_host.to_string()
    };

    let path = cookie.path.unwrap_or_else(|| default_path(request_path));

    let expiry_time = if let Some(max_age) = cookie.max_age {
      if max_age <= 0 {
        Some(0)
      } else {
        Some(current.saturating_add(max_age.unsigned_abs()))
      }
    } else {
      cookie.expires.map(|_| current.saturating_add(31_536_000))
    };

    self
      .cookies
      .retain(|c| !(c.name == cookie.name && c.domain == domain && c.path == path));

    if expiry_time != Some(0) {
      let stored = StoredCookie {
        name: cookie.name,
        value: cookie.value,
        domain,
        path,
        secure: cookie.secure,
        http_only: cookie.http_only,
        host_only,
        creation_time: current,
        expiry_time,
      };

      self.cookies.push(stored);
    }
  }

  /// Gets cookies to send in Cookie request header
  ///
  /// Returns matching cookies formatted as a Cookie header value,
  /// sorted by path length and creation time per RFC 6265.
  ///
  /// # Arguments
  /// * `uri` - Request URI to match against
  /// * `is_secure` - Whether the request is over HTTPS
  ///
  /// # Returns
  /// Cookie header value (empty string if no matches)
  pub fn get_request_cookies(
    &self,
    uri: &str,
    is_secure: bool,
  ) -> String {
    let Some(request_host) = extract_host_from_uri(uri) else {
      return String::new();
    };

    let request_path = extract_path_from_uri(uri);
    let current = self.current_time();

    let mut matching_cookies = Vec::new();

    for cookie in &self.cookies {
      if let Some(expiry) = cookie.expiry_time
        && expiry <= current
      {
        continue;
      }

      if cookie.secure && !is_secure {
        continue;
      }

      let domain_match = if cookie.host_only {
        request_host.eq_ignore_ascii_case(&cookie.domain)
      } else {
        domain_matches(request_host, &cookie.domain)
      };

      if !domain_match {
        continue;
      }

      if !path_matches(&request_path, &cookie.path) {
        continue;
      }

      matching_cookies.push(cookie);
    }

    matching_cookies.sort_by(|a, b| {
      b.path
        .len()
        .cmp(&a.path.len())
        .then_with(|| a.creation_time.cmp(&b.creation_time))
    });

    let mut result = String::new();
    for (i, cookie) in matching_cookies.iter().enumerate() {
      if i > 0 {
        result.push_str("; ");
      }
      result.push_str(&cookie.name);
      result.push('=');
      result.push_str(&cookie.value);
    }

    result
  }

  /// Clears all stored cookies
  pub fn clear(&mut self) {
    self.cookies.clear();
  }

  /// Returns an iterator over unexpired cookies
  ///
  /// Filters out cookies that have passed their expiration time.
  pub fn iter_unexpired(&self) -> impl Iterator<Item = &StoredCookie> {
    let current = self.current_time();
    self.cookies.iter().filter(move |c| {
      c.expiry_time
        .map_or_else(|| true, |expiry| expiry > current)
    })
  }
}

#[cfg(feature = "cookie-jar")]
impl Default for CookieStore {
  fn default() -> Self {
    Self::new()
  }
}

fn extract_host_from_uri(uri: &str) -> Option<&str> {
  let after_scheme = uri.find("://").map_or(uri, |pos| &uri[pos + 3..]);

  let host_end = after_scheme
    .find('/')
    .or_else(|| after_scheme.find('?'))
    .or_else(|| after_scheme.find('#'))
    .unwrap_or(after_scheme.len());

  let host_with_port = &after_scheme[..host_end];

  let host = host_with_port
    .rfind(':')
    .map_or(host_with_port, |pos| &host_with_port[..pos]);

  if host.is_empty() {
    None
  } else {
    Some(host)
  }
}

fn extract_path_from_uri(uri: &str) -> String {
  let after_scheme = uri.find("://").map_or(uri, |pos| &uri[pos + 3..]);

  after_scheme.find('/').map_or_else(
    || "/".to_string(),
    |path_start| {
      let path_with_query = &after_scheme[path_start..];

      let path_end = path_with_query
        .find('?')
        .or_else(|| path_with_query.find('#'))
        .unwrap_or(path_with_query.len());

      path_with_query[..path_end].to_string()
    },
  )
}

fn domain_matches(
  request_host: &str,
  cookie_domain: &str,
) -> bool {
  let request_lower = request_host.to_ascii_lowercase();
  let domain_lower = cookie_domain.to_ascii_lowercase();

  if request_lower == domain_lower {
    return true;
  }

  if request_lower.ends_with(&domain_lower) {
    let prefix_len = request_lower.len() - domain_lower.len();
    if let Some(byte) = request_lower.as_bytes().get(prefix_len.saturating_sub(1)) {
      return *byte == b'.';
    }
  }

  false
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

#[cfg(all(test, feature = "cookie-jar"))]
mod tests {
  use super::*;

  #[test]
  fn test_extract_host() {
    assert_eq!(extract_host_from_uri("http://example.com"), Some("example.com"));
    assert_eq!(extract_host_from_uri("https://example.com/path"), Some("example.com"));
    assert_eq!(
      extract_host_from_uri("http://example.com:8080/path"),
      Some("example.com")
    );
    assert_eq!(
      extract_host_from_uri("https://sub.example.com"),
      Some("sub.example.com")
    );
  }

  #[test]
  fn test_extract_path() {
    assert_eq!(extract_path_from_uri("http://example.com"), "/");
    assert_eq!(extract_path_from_uri("http://example.com/"), "/");
    assert_eq!(extract_path_from_uri("http://example.com/path"), "/path");
    assert_eq!(extract_path_from_uri("http://example.com/path/sub"), "/path/sub");
    assert_eq!(extract_path_from_uri("http://example.com/path?query"), "/path");
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
    let mut store = CookieStore::new();

    let set_cookie = alloc::vec!["session=abc123".to_string()];
    store.store_response_cookies("http://example.com/", &set_cookie);

    let cookies = store.get_request_cookies("http://example.com/", false);
    assert_eq!(cookies, "session=abc123");
  }

  #[test]
  fn test_cookie_path_matching() {
    let mut store = CookieStore::new();

    let set_cookie = alloc::vec!["id=123; Path=/admin".to_string()];
    store.store_response_cookies("http://example.com/admin/panel", &set_cookie);

    let cookies_admin = store.get_request_cookies("http://example.com/admin/panel", false);
    assert_eq!(cookies_admin, "id=123");

    let cookies_other = store.get_request_cookies("http://example.com/other", false);
    assert_eq!(cookies_other, "");
  }

  #[test]
  fn test_cookie_domain_matching() {
    let mut store = CookieStore::new();

    let set_cookie = alloc::vec!["id=123; Domain=example.com".to_string()];
    store.store_response_cookies("http://www.example.com/", &set_cookie);

    let cookies_www = store.get_request_cookies("http://www.example.com/", false);
    assert_eq!(cookies_www, "id=123");

    let cookies_sub = store.get_request_cookies("http://sub.example.com/", false);
    assert_eq!(cookies_sub, "id=123");

    let cookies_other = store.get_request_cookies("http://other.com/", false);
    assert_eq!(cookies_other, "");
  }

  #[test]
  fn test_secure_cookie() {
    let mut store = CookieStore::new();

    let set_cookie = alloc::vec!["token=secret; Secure".to_string()];
    store.store_response_cookies("https://example.com/", &set_cookie);

    let cookies_https = store.get_request_cookies("https://example.com/", true);
    assert_eq!(cookies_https, "token=secret");

    let cookies_http = store.get_request_cookies("http://example.com/", false);
    assert_eq!(cookies_http, "");
  }

  #[test]
  fn test_cookie_replacement() {
    let mut store = CookieStore::new();

    store.store_response_cookies("http://example.com/", &alloc::vec!["id=first".to_string()]);
    let cookies_first = store.get_request_cookies("http://example.com/", false);
    assert_eq!(cookies_first, "id=first");

    store.store_response_cookies("http://example.com/", &alloc::vec!["id=second".to_string()]);
    let cookies_second = store.get_request_cookies("http://example.com/", false);
    assert_eq!(cookies_second, "id=second");
  }

  #[test]
  fn test_multiple_cookies() {
    let mut store = CookieStore::new();

    store.store_response_cookies(
      "http://example.com/",
      &alloc::vec!["session=abc".to_string(), "lang=en".to_string(),],
    );

    let cookies = store.get_request_cookies("http://example.com/", false);
    assert!(cookies.contains("session=abc"));
    assert!(cookies.contains("lang=en"));
  }
}
