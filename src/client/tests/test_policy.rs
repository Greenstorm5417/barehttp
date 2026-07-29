use crate::client::http_client::{follow_redirect, sanitize_redirect_headers, validate_protocol};
use crate::config::Config;
use crate::error::Error;
use crate::headers::Headers;
use crate::method::Method;
use crate::parser::uri::Uri;
use crate::parser::version::Version;
use crate::transport::RawResponse;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
extern crate alloc;

fn make_redirect_response(
  status: u16,
  location: &str,
) -> RawResponse {
  let mut headers = Headers::new();
  headers.insert("Location", location);
  RawResponse {
    status_code: status,
    reason: String::from("Redirect"),
    headers,
    version: Version::HTTP_11,
    body_bytes: Vec::new(),
  }
}

fn raw_to_response_for_test(
  raw: RawResponse,
  method: Method,
) -> crate::parser::Response {
  let RawResponse {
    status_code,
    reason,
    mut headers,
    version,
    body_bytes,
  } = raw;
  let (body, trailers) = if method == Method::Head {
    (Vec::new(), Vec::new())
  } else {
    crate::parser::Response::parse_body_from_bytes(&body_bytes, &mut headers, status_code, version, usize::MAX).unwrap()
  };
  crate::parser::Response {
    status_code,
    reason,
    headers,
    body,
    trailers,
  }
}

fn process(
  config: &Config,
  visited: &mut Vec<String>,
  redirect_count: &mut u32,
  raw: RawResponse,
  current_url: &str,
  method: Method,
  body: Option<Vec<u8>>,
) -> Result<Option<(String, Method, Option<Vec<u8>>)>, Error> {
  if config.http_status_as_error && (400..600).contains(&raw.status_code) {
    let response = raw_to_response_for_test(raw, method);
    return Err(Error::HttpStatus(response.status_code, response));
  }
  let uri = Uri::parse(current_url).unwrap();
  let response = raw_to_response_for_test(raw, method);
  follow_redirect(
    config,
    visited,
    redirect_count,
    &response,
    &uri,
    current_url,
    method,
    body,
  )
}

#[test]
fn https_only_policy_rejects_http() {
  let config = Config {
    https_only: true,
    assume_tls_socket: true,
    ..Default::default()
  };
  let uri = Uri::parse("http://example.com").unwrap();
  assert!(matches!(validate_protocol(&config, &uri), Err(Error::HttpsOnly)));
}

#[test]
fn https_only_policy_allows_https_with_tls_socket() {
  let config = Config {
    https_only: true,
    assume_tls_socket: true,
    ..Default::default()
  };
  let uri = Uri::parse("https://example.com").unwrap();
  assert!(validate_protocol(&config, &uri).is_ok());
}

#[test]
fn default_rejects_https_without_tls_socket() {
  let uri = Uri::parse("https://example.com").unwrap();
  assert!(matches!(
    validate_protocol(&Config::default(), &uri),
    Err(Error::TlsNotConfigured)
  ));
}

#[test]
fn assume_tls_socket_allows_https() {
  let config = Config {
    assume_tls_socket: true,
    ..Default::default()
  };
  let uri = Uri::parse("https://example.com").unwrap();
  assert!(validate_protocol(&config, &uri).is_ok());
}

#[test]
fn default_allows_http() {
  let uri = Uri::parse("http://example.com").unwrap();
  assert!(validate_protocol(&Config::default(), &uri).is_ok());
}

#[test]
fn policy_drops_body_for_head_requests() {
  let mut headers = Headers::new();
  headers.insert("Content-Length", "10");
  let raw = RawResponse {
    status_code: 200,
    reason: String::from("OK"),
    headers,
    version: Version::HTTP_11,
    body_bytes: b"1234567890".to_vec(),
  };
  let resp = raw_to_response_for_test(raw, Method::Head);
  assert_eq!(resp.status_code, 200);
  assert!(resp.body.as_slice().is_empty(), "HEAD response body should be empty");
}

#[test]
fn redirect_method_table_301_302_303() {
  // ureq: GET/HEAD keep; all others → GET, drop body
  let statuses = [301_u16, 302, 303];
  let cases: &[(Method, Option<Vec<u8>>, Method, bool)] = &[
    (Method::Get, None, Method::Get, false),
    (Method::Head, None, Method::Head, false),
    (Method::Post, Some(vec![1]), Method::Get, true),
    (Method::Put, Some(vec![1]), Method::Get, true),
    (Method::Patch, Some(vec![1]), Method::Get, true),
    (Method::Delete, None, Method::Get, true),
  ];

  for status in statuses {
    for &(method, ref body, expect_method, drop_body) in cases {
      let mut visited = Vec::new();
      let mut count = 0;
      let next = process(
        &Config::default(),
        &mut visited,
        &mut count,
        make_redirect_response(status, "/next"),
        "http://a.com",
        method,
        body.clone(),
      )
      .unwrap()
      .expect("expected redirect");
      assert_eq!(next.1, expect_method, "{method:?} {status} → method");
      if drop_body {
        assert!(next.2.is_none(), "{method:?} {status} should drop body");
      }
    }
  }
}

#[test]
fn redirect_method_table_307_308() {
  // GET/HEAD follow keeping method; body methods → RedirectFailed
  for status in [307_u16, 308] {
    for method in [Method::Get, Method::Head] {
      let mut visited = Vec::new();
      let mut count = 0;
      let next = process(
        &Config::default(),
        &mut visited,
        &mut count,
        make_redirect_response(status, "/next"),
        "http://a.com",
        method,
        None,
      )
      .unwrap()
      .expect("expected redirect");
      assert_eq!(next.1, method, "{method:?} {status} keeps method");
    }

    for method in [Method::Post, Method::Put, Method::Patch, Method::Delete] {
      let mut visited = Vec::new();
      let mut count = 0;
      let body = if method == Method::Delete {
        None
      } else {
        Some(vec![1, 2, 3])
      };
      let err = process(
        &Config::default(),
        &mut visited,
        &mut count,
        make_redirect_response(status, "/next"),
        "http://a.com",
        method,
        body,
      )
      .unwrap_err();
      assert!(
        matches!(err, Error::RedirectFailed),
        "{method:?} {status} → RedirectFailed"
      );
    }
  }
}

#[test]
fn non_followable_3xx_is_returned() {
  // 304 and other 3xx (not 301/302/303/307/308) are not followed
  for status in [300_u16, 304, 305, 306, 399] {
    let mut visited = Vec::new();
    let mut count = 0;
    let mut headers = Headers::new();
    headers.insert("Location", "/next");
    let raw = RawResponse {
      status_code: status,
      reason: String::from("x"),
      headers,
      version: Version::HTTP_11,
      body_bytes: Vec::new(),
    };
    assert!(
      process(
        &Config::default(),
        &mut visited,
        &mut count,
        raw,
        "http://a.com",
        Method::Get,
        None,
      )
      .unwrap()
      .is_none(),
      "status {status} must not be followed"
    );
  }
}

#[test]
fn get_redirect_stays_get() {
  let mut visited = Vec::new();
  let mut count = 0;
  let next = process(
    &Config::default(),
    &mut visited,
    &mut count,
    make_redirect_response(302, "/next"),
    "http://a.com",
    Method::Get,
    None,
  )
  .unwrap()
  .expect("expected redirect");
  assert_eq!(next.1, Method::Get);
}

#[test]
fn redirect_loop_is_detected() {
  let mut visited = Vec::new();
  let mut count = 0;
  let config = Config::default();
  let raw = make_redirect_response(301, "http://a.com");

  process(
    &config,
    &mut visited,
    &mut count,
    raw.clone(),
    "http://a.com",
    Method::Get,
    None,
  )
  .unwrap();

  let err = process(
    &config,
    &mut visited,
    &mut count,
    raw,
    "http://a.com",
    Method::Get,
    None,
  )
  .unwrap_err();
  assert!(matches!(err, Error::RedirectLoop));
}

#[test]
fn status_error_when_configured() {
  for status in [404_u16, 500] {
    let mut visited = Vec::new();
    let mut count = 0;
    let config = Config {
      http_status_as_error: true,
      ..Default::default()
    };
    let mut headers = Headers::new();
    headers.insert("X-Err", "yes");
    let raw = RawResponse {
      status_code: status,
      reason: String::from("err"),
      headers,
      version: Version::HTTP_11,
      body_bytes: b"fail".to_vec(),
    };
    let err = process(
      &config,
      &mut visited,
      &mut count,
      raw,
      "http://example.com",
      Method::Get,
      None,
    )
    .unwrap_err();
    match err {
      Error::HttpStatus(code, resp) => {
        assert_eq!(code, status);
        assert_eq!(resp.status_code, status);
        assert_eq!(resp.body.as_slice(), b"fail");
        assert_eq!(resp.get_header("X-Err"), Some("yes"));
      },
      other => panic!("expected HttpStatus, got {other:?}"),
    }
  }
}

#[test]
fn status_4xx_is_ok_when_configured_as_response() {
  let mut visited = Vec::new();
  let mut count = 0;
  let config = Config {
    http_status_as_error: false,
    ..Default::default()
  };
  let raw = RawResponse {
    status_code: 404,
    reason: String::from("Not Found"),
    headers: Headers::new(),
    version: Version::HTTP_11,
    body_bytes: Vec::new(),
  };
  assert!(
    process(
      &config,
      &mut visited,
      &mut count,
      raw,
      "http://example.com",
      Method::Get,
      None
    )
    .unwrap()
    .is_none()
  );
}

#[test]
fn too_many_redirects_is_error() {
  let mut visited = Vec::new();
  let mut count = 0;
  let config = Config {
    max_redirects: 2,
    ..Default::default()
  };
  let raw = make_redirect_response(301, "/next");

  process(
    &config,
    &mut visited,
    &mut count,
    raw.clone(),
    "http://a.com",
    Method::Get,
    None,
  )
  .unwrap();
  process(
    &config,
    &mut visited,
    &mut count,
    raw.clone(),
    "http://b.com",
    Method::Get,
    None,
  )
  .unwrap();
  let err = process(
    &config,
    &mut visited,
    &mut count,
    raw,
    "http://c.com",
    Method::Get,
    None,
  )
  .unwrap_err();
  assert!(matches!(err, Error::TooManyRedirects));
}

#[test]
fn same_origin_redirect_follows() {
  let mut visited = Vec::new();
  let mut count = 0;
  let next = process(
    &Config::default(),
    &mut visited,
    &mut count,
    make_redirect_response(302, "/next"),
    "http://a.com/path",
    Method::Get,
    None,
  )
  .unwrap()
  .expect("expected redirect");
  assert_eq!(next.0, "http://a.com/next");
}

#[test]
fn cross_origin_redirect_follows() {
  let mut visited = Vec::new();
  let mut count = 0;
  let next = process(
    &Config::default(),
    &mut visited,
    &mut count,
    make_redirect_response(302, "http://b.com/next"),
    "http://a.com/path",
    Method::Get,
    None,
  )
  .unwrap()
  .expect("expected redirect");
  assert_eq!(next.0, "http://b.com/next");
}

#[test]
fn different_port_redirect_follows() {
  let mut visited = Vec::new();
  let mut count = 0;
  let next = process(
    &Config::default(),
    &mut visited,
    &mut count,
    make_redirect_response(302, "http://a.com:9090/next"),
    "http://a.com:8080/path",
    Method::Get,
    None,
  )
  .unwrap()
  .expect("expected redirect");
  assert_eq!(next.0, "http://a.com:9090/next");
}

#[test]
fn sanitize_strips_credentials_on_every_hop() {
  let mut headers = Headers::new();
  headers.insert("Authorization", "Bearer secret");
  headers.insert("Cookie", "sid=1");
  headers.insert("Connection", "keep-alive");
  headers.insert("Content-Length", "3");
  headers.insert("Host", "old.example.com");
  headers.insert("X-Custom", "keep");

  // Same-origin hop still strips Auth/Cookie (RedirectAuthHeaders::Never)
  sanitize_redirect_headers(&mut headers, false);
  assert!(!headers.contains("Authorization"));
  assert!(!headers.contains("Cookie"));
  assert!(!headers.contains("Connection"));
  assert!(!headers.contains("Content-Length"));
  assert!(!headers.contains("Host"));
  assert_eq!(headers.get("X-Custom"), Some("keep"));

  let mut drop_body = Headers::new();
  drop_body.insert("Content-Length", "99");
  drop_body.insert("Content-Type", "application/json");
  sanitize_redirect_headers(&mut drop_body, true);
  assert!(!drop_body.contains("Content-Length"));
  assert!(!drop_body.contains("Content-Type"));
}

#[test]
fn max_redirects_zero_does_not_follow() {
  let mut visited = Vec::new();
  let mut count = 0;
  let config = Config {
    max_redirects: 0,
    ..Default::default()
  };
  assert!(
    process(
      &config,
      &mut visited,
      &mut count,
      make_redirect_response(302, "/next"),
      "http://a.com",
      Method::Get,
      None,
    )
    .unwrap()
    .is_none()
  );
}

#[test]
fn chunked_trailers_reach_response() {
  let mut headers = Headers::new();
  headers.insert("Transfer-Encoding", "chunked");
  let raw = RawResponse {
    status_code: 200,
    reason: String::from("OK"),
    headers,
    version: Version::HTTP_11,
    body_bytes: b"5\r\nhello\r\n0\r\nX-Trailer: value\r\n\r\n".to_vec(),
  };
  let resp = raw_to_response_for_test(raw, Method::Get);
  assert_eq!(resp.body.as_slice(), b"hello");
  assert_eq!(resp.trailers, vec![(String::from("X-Trailer"), String::from("value"))]);
}
