use crate::client::policy::{PolicyDecision, RequestPolicy};
use crate::config::{Config, HttpStatusHandling, ProtocolRestriction, RedirectPolicy};
use crate::error::Error;
use crate::headers::Headers;
use crate::method::Method;
use crate::parser::uri::Uri;
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
    body_bytes: Vec::new(),
  }
}

#[test]
fn https_only_policy_rejects_http() {
  let policy = RequestPolicy::new(&Config {
    protocol_restriction: ProtocolRestriction::HttpsOnly,
    ..Default::default()
  });

  let uri = Uri::parse("http://example.com").unwrap();
  let result = policy.validate_protocol(&uri);

  assert!(matches!(result, Err(Error::HttpsRequired)));
}

#[test]
fn https_only_policy_allows_https() {
  let policy = RequestPolicy::new(&Config {
    protocol_restriction: ProtocolRestriction::HttpsOnly,
    ..Default::default()
  });

  let uri = Uri::parse("https://example.com").unwrap();
  assert!(policy.validate_protocol(&uri).is_ok());
}

#[test]
fn policy_drops_body_for_head_requests() {
  let mut policy = RequestPolicy::new(&Config::default());

  let mut headers = Headers::new();
  headers.insert("Content-Length", "10");

  let raw = RawResponse {
    status_code: 200,
    reason: String::from("OK"),
    headers,
    body_bytes: b"1234567890".to_vec(),
  };

  let decision = policy
    .process_raw_response(
      raw,
      &Uri::parse("http://example.com").unwrap(),
      "http://example.com",
      Method::Head,
      None,
    )
    .unwrap();

  match decision {
    PolicyDecision::Return(resp) => {
      assert_eq!(resp.status_code, 200);
      assert!(resp.body.as_bytes().is_empty(), "HEAD response body should be empty");
    },
    PolicyDecision::Redirect { .. } => panic!("Expected PolicyDecision::Return"),
  }
}

#[test]
fn post_302_redirect_becomes_get() {
  let mut policy = RequestPolicy::new(&Config::default());

  let raw = make_redirect_response(302, "/next");

  let decision = policy
    .process_raw_response(
      raw,
      &Uri::parse("http://a.com").unwrap(),
      "http://a.com",
      Method::Post,
      Some(vec![1, 2, 3]),
    )
    .unwrap();

  match decision {
    PolicyDecision::Redirect {
      next_method, next_body, ..
    } => {
      assert_eq!(next_method, Method::Get, "POST 302 should become GET");
      assert!(next_body.is_none(), "GET should not have body");
    },
    PolicyDecision::Return(_) => panic!("Expected PolicyDecision::Redirect"),
  }
}

#[test]
fn post_301_redirect_becomes_get() {
  let mut policy = RequestPolicy::new(&Config::default());

  let raw = make_redirect_response(301, "/next");

  let decision = policy
    .process_raw_response(
      raw,
      &Uri::parse("http://a.com").unwrap(),
      "http://a.com",
      Method::Post,
      Some(vec![1, 2, 3]),
    )
    .unwrap();

  match decision {
    PolicyDecision::Redirect {
      next_method, next_body, ..
    } => {
      assert_eq!(next_method, Method::Get);
      assert!(next_body.is_none());
    },
    PolicyDecision::Return(_) => panic!("Expected PolicyDecision::Redirect"),
  }
}

#[test]
fn post_303_redirect_becomes_get() {
  let mut policy = RequestPolicy::new(&Config::default());

  let raw = make_redirect_response(303, "/next");

  let decision = policy
    .process_raw_response(
      raw,
      &Uri::parse("http://a.com").unwrap(),
      "http://a.com",
      Method::Post,
      Some(vec![1, 2, 3]),
    )
    .unwrap();

  match decision {
    PolicyDecision::Redirect {
      next_method, next_body, ..
    } => {
      assert_eq!(next_method, Method::Get);
      assert!(next_body.is_none());
    },
    PolicyDecision::Return(_) => panic!("Expected PolicyDecision::Redirect"),
  }
}

#[test]
fn get_redirect_stays_get() {
  let mut policy = RequestPolicy::new(&Config::default());

  let raw = make_redirect_response(302, "/next");

  let decision = policy
    .process_raw_response(
      raw,
      &Uri::parse("http://a.com").unwrap(),
      "http://a.com",
      Method::Get,
      None,
    )
    .unwrap();

  match decision {
    PolicyDecision::Redirect { next_method, .. } => {
      assert_eq!(next_method, Method::Get);
    },
    PolicyDecision::Return(_) => panic!("Expected PolicyDecision::Redirect"),
  }
}

#[test]
fn redirect_loop_is_detected() {
  let mut policy = RequestPolicy::new(&Config::default());

  let raw = make_redirect_response(301, "http://a.com");
  let uri = Uri::parse("http://a.com").unwrap();

  policy
    .process_raw_response(raw.clone(), &uri, "http://a.com", Method::Get, None)
    .unwrap();

  let err = policy
    .process_raw_response(raw, &uri, "http://a.com", Method::Get, None)
    .unwrap_err();

  assert!(matches!(err, Error::RedirectLoop));
}

#[test]
fn status_4xx_is_error_when_configured() {
  let mut policy = RequestPolicy::new(&Config {
    http_status_handling: HttpStatusHandling::AsError,
    ..Default::default()
  });

  let raw = RawResponse {
    status_code: 404,
    reason: String::from("Not Found"),
    headers: Headers::new(),
    body_bytes: Vec::new(),
  };

  let err = policy
    .process_raw_response(
      raw,
      &Uri::parse("http://example.com").unwrap(),
      "http://example.com",
      Method::Get,
      None,
    )
    .unwrap_err();

  assert!(matches!(err, Error::HttpStatus(404)));
}

#[test]
fn status_5xx_is_error_when_configured() {
  let mut policy = RequestPolicy::new(&Config {
    http_status_handling: HttpStatusHandling::AsError,
    ..Default::default()
  });

  let raw = RawResponse {
    status_code: 500,
    reason: String::from("Internal Server Error"),
    headers: Headers::new(),
    body_bytes: Vec::new(),
  };

  let err = policy
    .process_raw_response(
      raw,
      &Uri::parse("http://example.com").unwrap(),
      "http://example.com",
      Method::Get,
      None,
    )
    .unwrap_err();

  assert!(matches!(err, Error::HttpStatus(500)));
}

#[test]
fn status_4xx_is_ok_when_configured_as_response() {
  let mut policy = RequestPolicy::new(&Config {
    http_status_handling: HttpStatusHandling::AsResponse,
    ..Default::default()
  });

  let raw = RawResponse {
    status_code: 404,
    reason: String::from("Not Found"),
    headers: Headers::new(),
    body_bytes: Vec::new(),
  };

  let result = policy.process_raw_response(
    raw,
    &Uri::parse("http://example.com").unwrap(),
    "http://example.com",
    Method::Get,
    None,
  );

  assert!(result.is_ok());
  match result.unwrap() {
    PolicyDecision::Return(resp) => assert_eq!(resp.status_code, 404),
    PolicyDecision::Redirect { .. } => panic!("Expected PolicyDecision::Return"),
  }
}

#[test]
fn too_many_redirects_is_error() {
  let mut policy = RequestPolicy::new(&Config {
    max_redirects: 2,
    ..Default::default()
  });

  let raw = make_redirect_response(301, "/next");

  policy
    .process_raw_response(
      raw.clone(),
      &Uri::parse("http://a.com").unwrap(),
      "http://a.com",
      Method::Get,
      None,
    )
    .unwrap();

  policy
    .process_raw_response(
      raw.clone(),
      &Uri::parse("http://b.com").unwrap(),
      "http://b.com",
      Method::Get,
      None,
    )
    .unwrap();

  let err = policy
    .process_raw_response(
      raw,
      &Uri::parse("http://c.com").unwrap(),
      "http://c.com",
      Method::Get,
      None,
    )
    .unwrap_err();

  assert!(matches!(err, Error::TooManyRedirects));
}

#[test]
fn no_follow_policy_returns_redirect_response() {
  let mut policy = RequestPolicy::new(&Config {
    redirect_policy: RedirectPolicy::NoFollow,
    ..Default::default()
  });

  let raw = make_redirect_response(302, "/next");

  let result = policy.process_raw_response(
    raw,
    &Uri::parse("http://a.com").unwrap(),
    "http://a.com",
    Method::Get,
    None,
  );

  match result.unwrap() {
    PolicyDecision::Return(resp) => assert_eq!(resp.status_code, 302),
    PolicyDecision::Redirect { .. } => {
      panic!("Should not follow redirect with NoFollow policy")
    },
  }
}
