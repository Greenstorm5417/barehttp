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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct HeaderName;
#[allow(missing_docs)]
impl HeaderName {
  pub const A_IM: &'static str = "a-im";
  pub const ACCEPT: &'static str = "accept";
  pub const ACCEPT_ADDITIONS: &'static str = "accept-additions";
  pub const ACCEPT_CH: &'static str = "accept-ch";
  pub const ACCEPT_CHARSET: &'static str = "accept-charset";
  pub const ACCEPT_DATETIME: &'static str = "accept-datetime";
  pub const ACCEPT_ENCODING: &'static str = "accept-encoding";
  pub const ACCEPT_FEATURES: &'static str = "accept-features";
  pub const ACCEPT_LANGUAGE: &'static str = "accept-language";
  pub const ACCEPT_PATCH: &'static str = "accept-patch";
  pub const ACCEPT_POST: &'static str = "accept-post";
  pub const ACCEPT_RANGES: &'static str = "accept-ranges";
  pub const ACCEPT_SIGNATURE: &'static str = "accept-signature";
  pub const ACCESS_CONTROL_ALLOW_CREDENTIALS: &'static str = "access-control-allow-credentials";
  pub const ACCESS_CONTROL_ALLOW_HEADERS: &'static str = "access-control-allow-headers";
  pub const ACCESS_CONTROL_ALLOW_METHODS: &'static str = "access-control-allow-methods";
  pub const ACCESS_CONTROL_ALLOW_ORIGIN: &'static str = "access-control-allow-origin";
  pub const ACCESS_CONTROL_EXPOSE_HEADERS: &'static str = "access-control-expose-headers";
  pub const ACCESS_CONTROL_MAX_AGE: &'static str = "access-control-max-age";
  pub const ACCESS_CONTROL_REQUEST_HEADERS: &'static str = "access-control-request-headers";
  pub const ACCESS_CONTROL_REQUEST_METHOD: &'static str = "access-control-request-method";
  pub const AGE: &'static str = "age";
  pub const ALLOW: &'static str = "allow";
  pub const ALPN: &'static str = "alpn";
  pub const ALT_SVC: &'static str = "alt-svc";
  pub const ALT_USED: &'static str = "alt-used";
  pub const ALTERNATES: &'static str = "alternates";
  pub const APPLY_TO_REDIRECT_REF: &'static str = "apply-to-redirect-ref";
  pub const AUTHENTICATION_CONTROL: &'static str = "authentication-control";
  pub const AUTHENTICATION_INFO: &'static str = "authentication-info";
  pub const AUTHORIZATION: &'static str = "authorization";
  pub const AVAILABLE_DICTIONARY: &'static str = "available-dictionary";
  pub const CACHE_CONTROL: &'static str = "cache-control";
  pub const CACHE_STATUS: &'static str = "cache-status";
  pub const CAL_MANAGED_ID: &'static str = "cal-managed-id";
  pub const CALDAV_TIMEZONES: &'static str = "caldav-timezones";
  pub const CAPSULE_PROTOCOL: &'static str = "capsule-protocol";
  pub const CDN_CACHE_CONTROL: &'static str = "cdn-cache-control";
  pub const CDN_LOOP: &'static str = "cdn-loop";
  pub const CERT_NOT_AFTER: &'static str = "cert-not-after";
  pub const CERT_NOT_BEFORE: &'static str = "cert-not-before";
  pub const CLEAR_SITE_DATA: &'static str = "clear-site-data";
  pub const CLIENT_CERT: &'static str = "client-cert";
  pub const CLIENT_CERT_CHAIN: &'static str = "client-cert-chain";
  pub const CLOSE: &'static str = "close";
  pub const CONNECTION: &'static str = "connection";
  pub const CONTENT_DIGEST: &'static str = "content-digest";
  pub const CONTENT_DISPOSITION: &'static str = "content-disposition";
  pub const CONTENT_ENCODING: &'static str = "content-encoding";
  pub const CONTENT_LANGUAGE: &'static str = "content-language";
  pub const CONTENT_LENGTH: &'static str = "content-length";
  pub const CONTENT_LOCATION: &'static str = "content-location";
  pub const CONTENT_RANGE: &'static str = "content-range";
  pub const CONTENT_SECURITY_POLICY: &'static str = "content-security-policy";
  pub const CONTENT_SECURITY_POLICY_REPORT_ONLY: &'static str = "content-security-policy-report-only";
  pub const CONTENT_TYPE: &'static str = "content-type";
  pub const COOKIE: &'static str = "cookie";
  pub const CROSS_ORIGIN_EMBEDDER_POLICY: &'static str = "cross-origin-embedder-policy";
  pub const CROSS_ORIGIN_EMBEDDER_POLICY_REPORT_ONLY: &'static str = "cross-origin-embedder-policy-report-only";
  pub const CROSS_ORIGIN_OPENER_POLICY: &'static str = "cross-origin-opener-policy";
  pub const CROSS_ORIGIN_OPENER_POLICY_REPORT_ONLY: &'static str = "cross-origin-opener-policy-report-only";
  pub const CROSS_ORIGIN_RESOURCE_POLICY: &'static str = "cross-origin-resource-policy";
  pub const DASL: &'static str = "dasl";
  pub const DATE: &'static str = "date";
  pub const DAV: &'static str = "dav";
  pub const DELTA_BASE: &'static str = "delta-base";
  pub const DEPRECATION: &'static str = "deprecation";
  pub const DEPTH: &'static str = "depth";
  pub const DESTINATION: &'static str = "destination";
  pub const DICTIONARY_ID: &'static str = "dictionary-id";
  pub const DPOP: &'static str = "dpop";
  pub const DPOP_NONCE: &'static str = "dpop-nonce";
  pub const EARLY_DATA: &'static str = "early-data";
  pub const ETAG: &'static str = "etag";
  pub const EXPECT: &'static str = "expect";
  pub const EXPIRES: &'static str = "expires";
  pub const FORWARDED: &'static str = "forwarded";
  pub const FROM: &'static str = "from";
  pub const HOBAREG: &'static str = "hobareg";
  pub const HOST: &'static str = "host";
  pub const IF: &'static str = "if";
  pub const IF_MATCH: &'static str = "if-match";
  pub const IF_MODIFIED_SINCE: &'static str = "if-modified-since";
  pub const IF_NONE_MATCH: &'static str = "if-none-match";
  pub const IF_RANGE: &'static str = "if-range";
  pub const IF_SCHEDULE_TAG_MATCH: &'static str = "if-schedule-tag-match";
  pub const IF_UNMODIFIED_SINCE: &'static str = "if-unmodified-since";
  pub const IM: &'static str = "im";
  pub const INCLUDE_REFERRED_TOKEN_BINDING_ID: &'static str = "include-referred-token-binding-id";
  pub const KEEP_ALIVE: &'static str = "keep-alive";
  pub const LABEL: &'static str = "label";
  pub const LAST_EVENT_ID: &'static str = "last-event-id";
  pub const LAST_MODIFIED: &'static str = "last-modified";
  pub const LINK: &'static str = "link";
  pub const LINK_TEMPLATE: &'static str = "link-template";
  pub const LOCATION: &'static str = "location";
  pub const LOCK_TOKEN: &'static str = "lock-token";
  pub const MAX_FORWARDS: &'static str = "max-forwards";
  pub const MEMENTO_DATETIME: &'static str = "memento-datetime";
  pub const METER: &'static str = "meter";
  pub const MIME_VERSION: &'static str = "mime-version";
  pub const NEGOTIATE: &'static str = "negotiate";
  pub const NEL: &'static str = "nel";
  pub const ODATA_ENTITYID: &'static str = "odata-entityid";
  pub const ODATA_ISOLATION: &'static str = "odata-isolation";
  pub const ODATA_MAXVERSION: &'static str = "odata-maxversion";
  pub const ODATA_VERSION: &'static str = "odata-version";
  pub const OPTIONAL_WWW_AUTHENTICATE: &'static str = "optional-www-authenticate";
  pub const ORDERING_TYPE: &'static str = "ordering-type";
  pub const ORIGIN: &'static str = "origin";
  pub const ORIGIN_AGENT_CLUSTER: &'static str = "origin-agent-cluster";
  pub const OSCORE: &'static str = "oscore";
  pub const OSLC_CORE_VERSION: &'static str = "oslc-core-version";
  pub const OVERWRITE: &'static str = "overwrite";
  pub const PING_FROM: &'static str = "ping-from";
  pub const PING_TO: &'static str = "ping-to";
  pub const POSITION: &'static str = "position";
  pub const PREFER: &'static str = "prefer";
  pub const PREFERENCE_APPLIED: &'static str = "preference-applied";
  pub const PRIORITY: &'static str = "priority";
  pub const PROXY_AUTHENTICATE: &'static str = "proxy-authenticate";
  pub const PROXY_AUTHENTICATION_INFO: &'static str = "proxy-authentication-info";
  pub const PROXY_AUTHORIZATION: &'static str = "proxy-authorization";
  pub const PROXY_STATUS: &'static str = "proxy-status";
  pub const PUBLIC_KEY_PINS: &'static str = "public-key-pins";
  pub const PUBLIC_KEY_PINS_REPORT_ONLY: &'static str = "public-key-pins-report-only";
  pub const RANGE: &'static str = "range";
  pub const REDIRECT_REF: &'static str = "redirect-ref";
  pub const REFERER: &'static str = "referer";
  pub const REFERRER_POLICY: &'static str = "referrer-policy";
  pub const REFRESH: &'static str = "refresh";
  pub const REPLAY_NONCE: &'static str = "replay-nonce";
  pub const REPR_DIGEST: &'static str = "repr-digest";
  pub const RETRY_AFTER: &'static str = "retry-after";
  pub const SCHEDULE_REPLY: &'static str = "schedule-reply";
  pub const SCHEDULE_TAG: &'static str = "schedule-tag";
  pub const SEC_FETCH_DEST: &'static str = "sec-fetch-dest";
  pub const SEC_FETCH_MODE: &'static str = "sec-fetch-mode";
  pub const SEC_FETCH_SITE: &'static str = "sec-fetch-site";
  pub const SEC_FETCH_USER: &'static str = "sec-fetch-user";
  pub const SEC_PURPOSE: &'static str = "sec-purpose";
  pub const SEC_TOKEN_BINDING: &'static str = "sec-token-binding";
  pub const SEC_WEBSOCKET_ACCEPT: &'static str = "sec-websocket-accept";
  pub const SEC_WEBSOCKET_EXTENSIONS: &'static str = "sec-websocket-extensions";
  pub const SEC_WEBSOCKET_KEY: &'static str = "sec-websocket-key";
  pub const SEC_WEBSOCKET_PROTOCOL: &'static str = "sec-websocket-protocol";
  pub const SEC_WEBSOCKET_VERSION: &'static str = "sec-websocket-version";
  pub const SERVER: &'static str = "server";
  pub const SERVER_TIMING: &'static str = "server-timing";
  pub const SET_COOKIE: &'static str = "set-cookie";
  pub const SIGNATURE: &'static str = "signature";
  pub const SIGNATURE_INPUT: &'static str = "signature-input";
  pub const SLUG: &'static str = "slug";
  pub const SOAPACTION: &'static str = "soapaction";
  pub const STATUS_URI: &'static str = "status-uri";
  pub const STRICT_TRANSPORT_SECURITY: &'static str = "strict-transport-security";
  pub const SUNSET: &'static str = "sunset";
  pub const TCN: &'static str = "tcn";
  pub const TE: &'static str = "te";
  pub const TIMEOUT: &'static str = "timeout";
  pub const TOPIC: &'static str = "topic";
  pub const TRACEPARENT: &'static str = "traceparent";
  pub const TRACESTATE: &'static str = "tracestate";
  pub const TRAILER: &'static str = "trailer";
  pub const TRANSFER_ENCODING: &'static str = "transfer-encoding";
  pub const TTL: &'static str = "ttl";
  pub const UPGRADE: &'static str = "upgrade";
  pub const URGENCY: &'static str = "urgency";
  pub const USE_AS_DICTIONARY: &'static str = "use-as-dictionary";
  pub const USER_AGENT: &'static str = "user-agent";
  pub const VARIANT_VARY: &'static str = "variant-vary";
  pub const VARY: &'static str = "vary";
  pub const VIA: &'static str = "via";
  pub const WANT_CONTENT_DIGEST: &'static str = "want-content-digest";
  pub const WANT_REPR_DIGEST: &'static str = "want-repr-digest";
  pub const WWW_AUTHENTICATE: &'static str = "www-authenticate";
  pub const X_CONTENT_TYPE_OPTIONS: &'static str = "x-content-type-options";
  pub const X_FRAME_OPTIONS: &'static str = "x-frame-options";
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
