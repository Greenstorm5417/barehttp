//! Public API shape locks (compile-time + light runtime).
//!
//! Compile-time checks use `assert_*` helpers (no `static_assertions` dep).
//! Runtime checks call the same signatures.
//!
//! Run with: `cargo test --workspace --all-features --test api_shape`

use barehttp::config::{Config, ConfigBuilder};
use barehttp::{
  Authority, ClientRequestBuilder, DecompressError, Error, Headers, Host, HttpClient, IntoStringError, Method,
  OsBlockingSocket, OsDnsResolver, ParseError, ParseMethodError, Response, Uri, Version,
};
use core::error::Error as StdError;
use core::hash::{Hash, Hasher};
use core::str::FromStr;

fn assert_copy<T: Copy>() {}
fn assert_hash<T: Hash>() {}
fn assert_eq_<T: Eq + PartialEq>() {}
fn assert_default<T: Default>() {}
fn assert_send_sync<T: Send + Sync>() {}

/// Drain a hasher so `Hash` is exercised without depending on a concrete hasher API.
fn hash_value<T: Hash>(value: &T) -> u64 {
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
  let mut h = CountingHasher(0);
  value.hash(&mut h);
  h.finish()
}

// ---------------------------------------------------------------------------
// Trait / type-shape contracts (compile-time + light runtime)
// ---------------------------------------------------------------------------

#[test]
fn uri_authority_host_are_copy_and_hash() {
  assert_copy::<Uri<'static>>();
  assert_copy::<Authority<'static>>();
  assert_copy::<Host<'static>>();
  assert_hash::<Uri<'static>>();
  assert_hash::<Authority<'static>>();
  assert_hash::<Host<'static>>();
  assert_eq_::<Uri<'static>>();

  let uri = Uri::parse("http://example.com/path?q=1").expect("uri");
  assert_eq!(uri.query(), Some("q=1"));
  let _ = hash_value(&uri);
  let auth = uri.authority().copied().expect("authority");
  let _ = hash_value(&auth);
  let _ = hash_value(&auth.host());
}

#[test]
fn version_and_method_defaults() {
  assert_default::<Version>();
  assert_default::<Method>();
  assert_eq!(Version::default(), Version::HTTP_11);
  assert_eq!(Method::default(), Method::Get);
}

#[test]
fn parse_method_error_is_exhaustive_outside_crate() {
  // If `ParseMethodError` gains `#[non_exhaustive]` or a second variant, this fails to compile.
  fn match_exhaustive(err: ParseMethodError) -> &'static str {
    match err {
      ParseMethodError::InvalidToken => "invalid",
    }
  }
  assert_eq!(match_exhaustive(ParseMethodError::InvalidToken), "invalid");
  // Unknown tokens become `Method::Extension`; only bad `tchar` is an error.
  assert!(matches!(Method::from_str("NOPE"), Ok(Method::Extension(_))));
  assert_eq!(Method::from_str("A/B"), Err(ParseMethodError::InvalidToken));
}

#[test]
fn config_is_hash_and_builder_type_exists() {
  assert_hash::<Config>();
  assert_eq_::<Config>();
  let a = Config::default();
  let b = Config::builder().build();
  assert_eq!(hash_value(&a), hash_value(&b));
  // `ConfigBuilder` is `#[must_use]`; we only lock the type name / construction path here.
  let _: ConfigBuilder = Config::builder();
}

#[test]
fn headers_insert_set_accept_str_without_string() {
  let mut headers = Headers::new();
  // Type inference: `&str` must satisfy `AsRef<str>` without `.to_string()`.
  let name: &str = "X-Trace";
  let value: &str = "abc";
  headers.insert(name, value);
  headers.set("content-type", "text/plain");
  assert_eq!(headers.get("x-trace"), Some("abc"));
  assert_eq!(headers.get("Content-Type"), Some("text/plain"));
}

#[test]
fn headers_owned_into_iter_yields_strings() {
  let mut headers = Headers::new();
  headers.insert("Host", "example.com");
  headers.insert("X-A", "1");
  let owned: Vec<(String, String)> = headers.into_iter().collect();
  assert_eq!(owned.len(), 2);
  assert_eq!(owned[0], ("Host".into(), "example.com".into()));
  assert_eq!(owned[1], ("X-A".into(), "1".into()));
}

/// Signature lock: `request` / `request_with_config` accept `Option<&[u8]>` / `None::<&[u8]>`.
///
/// Never executed; referenced so rustc type-checks the body.
#[allow(dead_code)]
fn api_request_accepts_option_slice(client: &HttpClient<OsBlockingSocket, OsDnsResolver>) {
  let headers = Headers::new();
  let _ = client.request(Method::Get, "http://example.com/", &headers, None::<&[u8]>);
  let _ = client.request(Method::Post, "http://example.com/", &headers, Some(&b"hi"[..]));
  let _ = client.request_with_config(
    client.config(),
    Method::Get,
    "http://example.com/",
    &headers,
    None::<&[u8]>,
  );
  let owned = vec![1u8, 2];
  let _ = client.request(Method::Put, "http://example.com/", &headers, Some(owned.as_slice()));
}

#[test]
fn request_body_option_slice_signature_compiles() {
  let _ = api_request_accepts_option_slice as fn(&HttpClient<OsBlockingSocket, OsDnsResolver>);
}

#[test]
fn primary_type_names_and_undeprecated_aliases() {
  // Primaries exist at the crate root.
  let _: HttpClient<OsBlockingSocket, OsDnsResolver> = HttpClient::new();
  let _: ClientRequestBuilder<OsBlockingSocket, OsDnsResolver> = HttpClient::new().get("http://example.com/");
  // Ureq-like synonyms stay undeprecated (type aliases).
  let _: barehttp::Agent = barehttp::agent();
  let _: barehttp::RequestBuilder = barehttp::get("http://example.com/");
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[test]
fn body_exceeds_limit_lifts_out_of_parse_error() {
  let err: Error = ParseError::BodyExceedsLimit(64).into();
  assert!(matches!(err, Error::BodyExceedsLimit(64)));
  // Must not be nested under `Error::Parse`.
  assert!(!matches!(err, Error::Parse(_)));
}

#[test]
fn decompression_parse_error_exposes_source() {
  let err = ParseError::Decompression(DecompressError::InvalidInput);
  let source = StdError::source(&err).expect("Decompression must expose source()");
  assert!(source.is::<DecompressError>());
  assert_eq!(
    source.downcast_ref::<DecompressError>(),
    Some(&DecompressError::InvalidInput)
  );

  let limit = ParseError::Decompression(DecompressError::LimitExceeded);
  // LimitExceeded as decompress source is still `Decompression(_)` at ParseError;
  // client boundary lifts only `ParseError::BodyExceedsLimit`.
  assert!(StdError::source(&limit).is_some());
}

#[test]
fn response_to_text_and_into_string_error_shapes() {
  let ok = Response::parse(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok").expect("parse");
  assert_eq!(ok.status_code(), 200);
  assert_eq!(ok.body(), b"ok");
  let text: Result<&str, core::str::Utf8Error> = ok.to_text();
  assert_eq!(text.expect("utf8"), "ok");

  let bad = Response::parse(b"HTTP/1.1 201 Created\r\nContent-Length: 1\r\n\r\n\xff").expect("parse");
  let utf8_err = bad.to_text().expect_err("invalid utf8");
  let _ = utf8_err; // Type is `Result<&str, Utf8Error>`.

  let bad = Response::parse(b"HTTP/1.1 201 Created\r\nContent-Length: 1\r\n\r\n\xff").expect("parse");
  let into_err: IntoStringError = bad.into_string().expect_err("into_string");
  assert_eq!(into_err.response().status_code(), 201);
  assert_eq!(into_err.response().body(), [0xff]);
  let recovered = into_err.into_response();
  assert_eq!(recovered.status_code(), 201);
}

#[test]
fn primary_accessors_status_code_and_body() {
  let r = Response::parse(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n").expect("parse");
  assert_eq!(r.status_code(), 204);
  assert_eq!(r.body(), b"");
  // Deprecated aliases must still exist (compat).
  #[allow(deprecated)]
  {
    assert_eq!(r.status(), 204);
    assert_eq!(r.as_bytes(), b"");
  }
}

#[test]
fn into_bytes_returns_vec_u8() {
  let r = Response::parse(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nxyz").expect("parse");
  let body: Vec<u8> = r.into_bytes();
  assert_eq!(body, b"xyz");
}

#[test]
fn response_is_hash() {
  assert_hash::<Response>();
  let a = Response::parse(b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\n\r\nx").expect("parse");
  let b = Response::parse(b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\n\r\nx").expect("parse");
  assert_eq!(hash_value(&a), hash_value(&b));
}

#[test]
fn http_status_error_recovers_boxed_response() {
  let resp = Response::parse(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 4\r\n\r\ndown").expect("parse");
  let err = Error::HttpStatus(503, Box::new(resp));
  match err {
    Error::HttpStatus(code, boxed) => {
      assert_eq!(code, 503);
      assert_eq!(boxed.status_code(), 503);
      assert_eq!(boxed.body(), b"down");
    },
    other => panic!("unexpected: {other:?}"),
  }
}

// ---------------------------------------------------------------------------
// Cookie jar (`cookie-jar`)
// ---------------------------------------------------------------------------

#[cfg(feature = "cookie-jar")]
mod cookie_jar_api {
  use super::{assert_eq_, assert_hash, hash_value};
  use barehttp::cookie_jar::{CookieJar, CookieStore, StoredCookie};
  use barehttp::{ParseError, Uri};

  #[test]
  fn stored_cookie_partial_eq_eq_hash() {
    assert_eq_::<StoredCookie>();
    assert_hash::<StoredCookie>();

    let store = CookieStore::new();
    store
      .store_response_cookies("http://example.com/", ["id=1"])
      .expect("uri");
    let mut iter = store.iter();
    let first = iter.next().expect("cookie").clone();
    drop(iter);
    let _ = hash_value(&first);
    assert_eq!(first.name(), "id");
    assert_eq!(first.value(), "1");
  }

  #[test]
  fn store_response_cookies_accepts_str_slices() {
    let store = CookieStore::new();
    // Non-`String` items: `&[&str]` via `AsRef<str>`.
    let headers: &[&str] = &["session=abc; Path=/", "theme=dark"];
    store
      .store_response_cookies("http://example.com/", headers)
      .expect("uri");
    assert_eq!(
      store.request_cookie_header("http://example.com/"),
      "session=abc; theme=dark"
    );
  }

  #[test]
  fn store_response_cookies_invalid_uri_is_err() {
    let store = CookieStore::new();
    assert_eq!(
      store.store_response_cookies("://bad", ["a=1"]),
      Err(ParseError::InvalidUri)
    );
  }

  #[test]
  fn cookie_jar_alias_is_cookie_store() {
    let jar: CookieJar = CookieStore::new();
    jar
      .store_response_cookies("http://example.com/", ["x=1"])
      .expect("uri");
    assert_eq!(jar.request_cookie_header("http://example.com/"), "x=1");
  }

  #[test]
  fn cookie_store_accessor_returns_store_ref_not_arc() {
    use barehttp::HttpClient;
    let client = HttpClient::new();
    // Signature lock: `&CookieStore`, not `&Arc<CookieStore>`.
    let store: &CookieStore = client.cookie_store();
    assert!(store.iter().next().is_none());
  }

  #[test]
  fn request_cookie_header_one_arg_https_enables_secure() {
    let store = CookieStore::new();
    store
      .store_response_cookies("https://example.com/", ["token=secret; Secure"])
      .expect("uri");

    // One-arg API: scheme of `uri` decides Secure eligibility (no bool).
    let https = store.request_cookie_header("https://example.com/");
    assert_eq!(https, "token=secret");

    let http = store.request_cookie_header("http://example.com/");
    assert_eq!(http, "");

    // Type-level: callable with a single `&str` (URI string).
    let uri = Uri::parse("https://example.com/").expect("uri");
    let _ = store.request_cookie_header("https://example.com/");
    let _ = uri; // documents that callers pass URI strings, not (uri, bool)
  }

  #[test]
  fn same_site_is_public_and_stored() {
    use barehttp::cookie_jar::SameSite;
    let store = CookieStore::new();
    store
      .store_response_cookies("https://example.com/", ["x=1; SameSite=Strict; Secure"])
      .expect("uri");
    let c = store.iter().next().expect("cookie");
    assert_eq!(c.same_site(), SameSite::Strict);
    assert!(!c.http_only());
  }
}

// ---------------------------------------------------------------------------
// Send + Sync: covered by `src/lib.rs` const asserts; smoke that Agent still is.
// ---------------------------------------------------------------------------

#[test]
fn agent_still_send_sync() {
  assert_send_sync::<barehttp::Agent>();
  assert_send_sync::<HttpClient<OsBlockingSocket, OsDnsResolver>>();
  assert_send_sync::<Error>();
  assert_send_sync::<Response>();
  assert_send_sync::<Config>();
}
