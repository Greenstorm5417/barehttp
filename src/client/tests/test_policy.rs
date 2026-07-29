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
  let (body, trailers) = if method == Method::Head {
    (Vec::new(), Vec::new())
  } else {
    crate::parser::Response::parse_body_from_bytes(&raw.body_bytes, &raw.headers, raw.status_code, raw.version).unwrap()
  };
  crate::parser::Response {
    status_code: raw.status_code,
    reason: raw.reason,
    headers: raw.headers,
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
) -> Result<Option<(String, Method, Option<Vec<u8>>, bool)>, Error> {
  if config.http_status_as_error && (400..600).contains(&raw.status_code) {
    return Err(Error::HttpStatus(raw.status_code));
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
  assert!(matches!(validate_protocol(&config, &uri), Err(Error::HttpsRequired)));
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
    Err(Error::HttpsRequired)
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
fn post_3xx_redirect_becomes_get() {
  for status in [301_u16, 302, 303] {
    let mut visited = Vec::new();
    let mut count = 0;
    let next = process(
      &Config::default(),
      &mut visited,
      &mut count,
      make_redirect_response(status, "/next"),
      "http://a.com",
      Method::Post,
      Some(vec![1, 2, 3]),
    )
    .unwrap()
    .expect("expected redirect");
    assert_eq!(next.1, Method::Get, "POST {status} should become GET");
    assert!(next.2.is_none(), "GET should not have body");
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
    let raw = RawResponse {
      status_code: status,
      reason: String::from("err"),
      headers: Headers::new(),
      version: Version::HTTP_11,
      body_bytes: Vec::new(),
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
    assert!(matches!(err, Error::HttpStatus(s) if s == status));
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
fn same_origin_redirect_keeps_credentials_flag_clear() {
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
  assert!(!next.3);
  assert_eq!(next.0, "http://a.com/next");
}

#[test]
fn cross_origin_redirect_sets_flag() {
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
  assert!(next.3);
}

#[test]
fn different_port_is_cross_origin() {
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
  assert!(next.3);
}

#[test]
fn sanitize_strips_credentials_cross_origin_and_hop_by_hop() {
  let mut headers = Headers::new();
  headers.insert("Authorization", "Bearer secret");
  headers.insert("Cookie", "sid=1");
  headers.insert("Connection", "keep-alive");
  headers.insert("X-Custom", "keep");

  sanitize_redirect_headers(&mut headers, true, false);
  assert!(!headers.contains("Authorization"));
  assert!(!headers.contains("Cookie"));
  assert!(!headers.contains("Connection"));
  assert_eq!(headers.get("X-Custom"), Some("keep"));

  let mut same_origin = Headers::new();
  same_origin.insert("Authorization", "Bearer secret");
  same_origin.insert("TE", "trailers");
  sanitize_redirect_headers(&mut same_origin, false, false);
  assert_eq!(same_origin.get("Authorization"), Some("Bearer secret"));
  assert!(!same_origin.contains("TE"));

  let mut drop_body = Headers::new();
  drop_body.insert("Content-Length", "99");
  drop_body.insert("Content-Type", "application/json");
  sanitize_redirect_headers(&mut drop_body, false, true);
  assert!(!drop_body.contains("Content-Length"));
  assert!(!drop_body.contains("Content-Type"));
}

#[test]
fn no_follow_policy_returns_redirect_response() {
  let mut visited = Vec::new();
  let mut count = 0;
  let config = Config {
    follow_redirects: false,
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
