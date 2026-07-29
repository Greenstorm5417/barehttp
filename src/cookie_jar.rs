use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

use crate::parser::cookie::SetCookie;
use crate::parser::uri::{Host, Uri};

#[derive(Debug, Clone)]
/// One cookie as kept in [`CookieStore`].
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
  /// Host-only flag - cookie only matches exact host
  pub host_only: bool,
  /// Creation time (logical counter for sort order)
  pub creation_time: u64,
  /// Expiry as Unix seconds (UTC); `None` means session cookie
  pub expiry_time: Option<u64>,
}

#[derive(Debug)]
/// Mutex-backed jar: RFC 6265 domain/path matching, expiry, and `Secure`.
pub struct CookieStore {
  cookies: Mutex<Vec<StoredCookie>>,
}

impl CookieStore {
  /// Creates a new empty cookie store
  #[must_use]
  pub const fn new() -> Self {
    Self {
      cookies: Mutex::new(Vec::new()),
    }
  }

  /// Parse `Set-Cookie` values and insert them (RFC 6265 domain/path match; replace on name+domain+path).
  pub fn store_response_cookies(
    &self,
    uri: &str,
    set_cookie_headers: &[String],
  ) {
    let Some((request_host, request_path)) = host_and_path(uri) else {
      return;
    };

    let mut cookies = self.cookies.lock();
    for header_value in set_cookie_headers {
      if let Some(parsed) = SetCookie::parse(header_value) {
        Self::insert_cookie_locked(&mut cookies, parsed, &request_host, &request_path);
      }
    }
  }

  fn insert_cookie_locked(
    cookies: &mut Vec<StoredCookie>,
    cookie: SetCookie,
    request_host: &str,
    request_path: &str,
  ) {
    let creation = u64::try_from(cookies.len()).unwrap_or(u64::MAX);
    let now = now_unix_secs();

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

    // RFC 6265: Max-Age wins over Expires when both are present.
    let expiry_time = if let Some(max_age) = cookie.max_age {
      if max_age <= 0 {
        Some(0)
      } else {
        Some(now.saturating_add(max_age.unsigned_abs()))
      }
    } else if let Some(expires) = cookie.expires {
      match expires.to_unix_secs() {
        Some(ts) if ts > now => Some(ts),
        _ => Some(0),
      }
    } else {
      None
    };

    cookies.retain(|c| !(c.name == cookie.name && c.domain == domain && c.path == path));

    if expiry_time != Some(0) {
      cookies.push(StoredCookie {
        name: cookie.name,
        value: cookie.value,
        domain,
        path,
        secure: cookie.secure,
        host_only,
        creation_time: creation,
        expiry_time,
      });
    }
  }

  /// Cookie header value for `uri` (RFC 6265 path-length / creation-time sort).
  ///
  /// Empty when nothing matches. Skips `Secure` cookies unless `is_secure`.
  pub fn get_request_cookies(
    &self,
    uri: &str,
    is_secure: bool,
  ) -> String {
    let Some((request_host, request_path)) = host_and_path(uri) else {
      return String::new();
    };

    let now = now_unix_secs();

    let cookies = self.cookies.lock();
    let mut matching_cookies = Vec::new();

    for cookie in cookies.iter() {
      if let Some(expiry) = cookie.expiry_time
        && expiry <= now
      {
        continue;
      }

      if cookie.secure && !is_secure {
        continue;
      }

      let domain_match = if cookie.host_only {
        request_host.eq_ignore_ascii_case(&cookie.domain)
      } else {
        domain_matches(&request_host, &cookie.domain)
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
}

impl Default for CookieStore {
  fn default() -> Self {
    Self::new()
  }
}

/// Wall-clock Unix seconds for cookie expiry (RFC 6265 Absolute Time).
fn now_unix_secs() -> u64 {
  #[cfg(unix)]
  {
    // SAFETY: time(NULL) is well-defined; negative means error → 0.
    let t = unsafe { libc::time(core::ptr::null_mut()) };
    if t < 0 {
      0
    } else {
      u64::try_from(t).unwrap_or(0)
    }
  }
  #[cfg(windows)]
  {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::SystemInformation::GetSystemTimeAsFileTime;
    let mut ft = FILETIME {
      dwLowDateTime: 0,
      dwHighDateTime: 0,
    };
    // SAFETY: writes into stack FILETIME.
    unsafe { GetSystemTimeAsFileTime(&mut ft) };
    let ticks = (u64::from(ft.dwHighDateTime) << 32) | u64::from(ft.dwLowDateTime);
    // FILETIME: 100ns since 1601-01-01; Unix epoch offset = 11_644_473_600 s
    ticks
      .checked_div(10_000_000)
      .and_then(|s| s.checked_sub(11_644_473_600))
      .unwrap_or(0)
  }
  #[cfg(not(any(unix, windows)))]
  {
    0
  }
}

fn host_and_path(uri: &str) -> Option<(String, String)> {
  let parsed = Uri::parse(uri).ok()?;
  let auth = parsed.authority()?;
  let host = match auth.host() {
    Host::RegName(name) => String::from(*name),
    Host::IpAddr(addr) => crate::util::format_ip_for_host(*addr),
  };
  let path = if parsed.path().is_empty() {
    String::from("/")
  } else {
    String::from(parsed.path())
  };
  Some((host, path))
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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_host_and_path() {
    assert_eq!(
      host_and_path("http://example.com"),
      Some((String::from("example.com"), String::from("/")))
    );
    assert_eq!(
      host_and_path("https://example.com/path"),
      Some((String::from("example.com"), String::from("/path")))
    );
    assert_eq!(
      host_and_path("http://example.com:8080/path"),
      Some((String::from("example.com"), String::from("/path")))
    );
    assert_eq!(
      host_and_path("http://example.com/path?query"),
      Some((String::from("example.com"), String::from("/path")))
    );
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
    let store = CookieStore::new();

    let set_cookie = alloc::vec!["session=abc123".to_string()];
    store.store_response_cookies("http://example.com/", &set_cookie);

    let cookies = store.get_request_cookies("http://example.com/", false);
    assert_eq!(cookies, "session=abc123");
  }

  #[test]
  fn test_cookie_path_matching() {
    let store = CookieStore::new();

    let set_cookie = alloc::vec!["id=123; Path=/admin".to_string()];
    store.store_response_cookies("http://example.com/admin/panel", &set_cookie);

    let cookies_admin = store.get_request_cookies("http://example.com/admin/panel", false);
    assert_eq!(cookies_admin, "id=123");

    let cookies_other = store.get_request_cookies("http://example.com/other", false);
    assert_eq!(cookies_other, "");
  }

  #[test]
  fn test_cookie_domain_matching() {
    let store = CookieStore::new();

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
    let store = CookieStore::new();

    let set_cookie = alloc::vec!["token=secret; Secure".to_string()];
    store.store_response_cookies("https://example.com/", &set_cookie);

    let cookies_https = store.get_request_cookies("https://example.com/", true);
    assert_eq!(cookies_https, "token=secret");

    let cookies_http = store.get_request_cookies("http://example.com/", false);
    assert_eq!(cookies_http, "");
  }

  #[test]
  fn test_cookie_replacement() {
    let store = CookieStore::new();

    store.store_response_cookies("http://example.com/", &alloc::vec!["id=first".to_string()]);
    let cookies_first = store.get_request_cookies("http://example.com/", false);
    assert_eq!(cookies_first, "id=first");

    store.store_response_cookies("http://example.com/", &alloc::vec!["id=second".to_string()]);
    let cookies_second = store.get_request_cookies("http://example.com/", false);
    assert_eq!(cookies_second, "id=second");
  }

  #[test]
  fn test_multiple_cookies() {
    let store = CookieStore::new();

    store.store_response_cookies(
      "http://example.com/",
      &alloc::vec!["session=abc".to_string(), "lang=en".to_string()],
    );

    let cookies = store.get_request_cookies("http://example.com/", false);
    assert!(cookies.contains("session=abc"));
    assert!(cookies.contains("lang=en"));
  }

  #[test]
  fn expires_in_past_is_not_stored() {
    let store = CookieStore::new();
    store.store_response_cookies(
      "http://example.com/",
      &alloc::vec!["gone=1; Expires=Thu, 01 Jan 1970 00:00:00 GMT".to_string()],
    );
    assert!(
      store
        .get_request_cookies("http://example.com/", false)
        .is_empty()
    );
  }

  #[test]
  fn expires_in_future_is_stored() {
    let store = CookieStore::new();
    store.store_response_cookies(
      "http://example.com/",
      &alloc::vec!["keep=1; Expires=Wed, 09 Jun 2099 10:18:14 GMT".to_string()],
    );
    assert_eq!(store.get_request_cookies("http://example.com/", false), "keep=1");
  }

  #[test]
  fn max_age_zero_deletes() {
    let store = CookieStore::new();
    store.store_response_cookies("http://example.com/", &alloc::vec!["id=1".to_string()]);
    assert_eq!(store.get_request_cookies("http://example.com/", false), "id=1");
    store.store_response_cookies("http://example.com/", &alloc::vec!["id=1; Max-Age=0".to_string()]);
    assert!(
      store
        .get_request_cookies("http://example.com/", false)
        .is_empty()
    );
  }
}
