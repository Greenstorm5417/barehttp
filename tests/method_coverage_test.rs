//! Comprehensive method coverage tests for barehttp
//!
//! This file tests every public method to catch breaking changes.

use barehttp::response::ResponseExt;
use barehttp::{Error, HttpClient, Request};

const HTTPBIN: &str = "http://httpbin.org";

// ============================================================================
// Request Builder Methods
// ============================================================================

#[test]
fn test_method_header() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let response = client
    .get(format!("{}/headers", HTTPBIN))
    .header("X-Test-Header", "test-value")
    .call()?;

  let body = response.text()?;
  assert!(body.contains("X-Test-Header"));
  assert!(body.contains("test-value"));
  Ok(())
}

#[test]
fn test_method_query() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let response = client
    .get(format!("{}/get", HTTPBIN))
    .query("key1", "value1")
    .query("key2", "value2")
    .call()?;

  let body = response.text()?;
  assert!(body.contains("key1"));
  assert!(body.contains("value1"));
  assert!(body.contains("key2"));
  assert!(body.contains("value2"));
  Ok(())
}

#[test]
fn test_method_query_pairs() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let params = vec![("foo", "bar"), ("baz", "qux")];

  let response = client
    .get(format!("{}/get", HTTPBIN))
    .query_pairs(params)
    .call()?;

  let body = response.text()?;
  assert!(body.contains("foo"));
  assert!(body.contains("bar"));
  assert!(body.contains("baz"));
  assert!(body.contains("qux"));
  Ok(())
}

#[test]
fn test_method_form() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let response = client
    .post(format!("{}/post", HTTPBIN))
    .form("field1", "value1")
    .form("field2", "value2")
    .call()?;

  let body = response.text()?;
  assert!(body.contains("field1"));
  assert!(body.contains("value1"));
  Ok(())
}

#[test]
fn test_method_content_type() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let response = client
    .post(format!("{}/post", HTTPBIN))
    .content_type("application/json")
    .send(br#"{"test":"data"}"#.to_vec())?;

  let body = response.text()?;
  assert!(body.contains("application/json"));
  Ok(())
}

#[test]
fn test_method_cookie() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let response = client
    .get(format!("{}/cookies", HTTPBIN))
    .cookie("session", "abc123")
    .cookie("user", "john")
    .call()?;

  // httpbin echoes cookies back
  assert!(response.is_success());
  Ok(())
}

// ============================================================================
// HTTP Method Tests
// ============================================================================

#[test]
fn test_http_method_get() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let response = client.get(format!("{}/get", HTTPBIN)).call()?;
  assert_eq!(response.status_code, 200);
  Ok(())
}

#[test]
fn test_http_method_post() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let response = client
    .post(format!("{}/post", HTTPBIN))
    .send(b"test".to_vec())?;
  assert_eq!(response.status_code, 200);
  Ok(())
}

#[test]
fn test_http_method_put() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let response = client
    .put(format!("{}/put", HTTPBIN))
    .send(b"test".to_vec())?;
  assert_eq!(response.status_code, 200);
  Ok(())
}

#[test]
fn test_http_method_delete() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let response = client.delete(format!("{}/delete", HTTPBIN)).call()?;
  assert_eq!(response.status_code, 200);
  Ok(())
}

#[test]
fn test_http_method_head() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let response = client.head(format!("{}/get", HTTPBIN)).call()?;
  assert_eq!(response.status_code, 200);
  assert!(response.body.as_bytes().is_empty());
  Ok(())
}

#[test]
fn test_http_method_patch() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let response = client
    .patch(format!("{}/patch", HTTPBIN))
    .send(b"test".to_vec())?;
  assert_eq!(response.status_code, 200);
  Ok(())
}

#[test]
fn test_http_method_options() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let response = client.options(format!("{}/get", HTTPBIN)).call()?;
  assert_eq!(response.status_code, 200);
  Ok(())
}

#[test]
fn test_http_method_trace() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  // TRACE may not be supported by httpbin, so we just check it doesn't panic
  let _ = client.trace(format!("{}/get", HTTPBIN)).call();
  Ok(())
}

// ============================================================================
// Request Terminators
// ============================================================================

#[test]
fn test_terminator_call() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let response = client.get(format!("{}/get", HTTPBIN)).call()?;
  assert!(response.is_success());
  Ok(())
}

#[test]
fn test_terminator_send() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let response = client
    .post(format!("{}/post", HTTPBIN))
    .send(b"test data".to_vec())?;
  assert!(response.is_success());
  Ok(())
}

#[test]
fn test_terminator_send_string() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let response = client
    .post(format!("{}/post", HTTPBIN))
    .send_string("test string")?;
  assert!(response.is_success());
  Ok(())
}

#[test]
fn test_terminator_send_empty() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let response = client
    .post(format!("{}/post", HTTPBIN))
    .send_empty()?;
  assert!(response.is_success());
  Ok(())
}

// ============================================================================
// Response Methods (ResponseExt trait)
// ============================================================================

#[test]
fn test_response_method_status() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let response = client.get(format!("{}/get", HTTPBIN)).call()?;
  assert_eq!(response.status(), 200);
  Ok(())
}

#[test]
fn test_response_method_cookies() -> Result<(), Error> {
  use barehttp::config::{Config, RedirectPolicy};

  let config = Config {
    redirect_policy: RedirectPolicy::NoFollow,
    ..Default::default()
  };
  let mut client = HttpClient::with_config(config)?;

  let response = client
    .get(format!("{}/cookies/set?test=value", HTTPBIN))
    .call()?;

  let cookies = response.cookies();
  // With NoFollow, we should get the 302 with Set-Cookie headers
  assert!(response.status_code == 302 || !cookies.is_empty());
  Ok(())
}

#[test]
fn test_response_method_is_success() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let response = client.get(format!("{}/status/200", HTTPBIN)).call()?;
  assert!(response.is_success());

  let response2 = client.get(format!("{}/status/201", HTTPBIN)).call()?;
  assert!(response2.is_success());
  Ok(())
}

#[test]
fn test_response_method_is_redirect() -> Result<(), Error> {
  use barehttp::config::{Config, RedirectPolicy};

  let config = Config {
    redirect_policy: RedirectPolicy::NoFollow,
    ..Default::default()
  };
  let mut client = HttpClient::with_config(config)?;

  let response = client.get(format!("{}/redirect/1", HTTPBIN)).call()?;
  assert!(response.is_redirect());
  Ok(())
}

#[test]
fn test_response_method_is_client_error() -> Result<(), Error> {
  use barehttp::config::{Config, HttpStatusHandling};

  let config = Config {
    http_status_handling: HttpStatusHandling::AsResponse,
    ..Default::default()
  };
  let mut client = HttpClient::with_config(config)?;

  let response = client.get(format!("{}/status/404", HTTPBIN)).call()?;
  assert!(response.is_client_error());
  Ok(())
}

#[test]
fn test_response_method_is_server_error() -> Result<(), Error> {
  use barehttp::config::{Config, HttpStatusHandling};

  let config = Config {
    http_status_handling: HttpStatusHandling::AsResponse,
    ..Default::default()
  };
  let mut client = HttpClient::with_config(config)?;

  let response = client.get(format!("{}/status/500", HTTPBIN)).call()?;
  assert!(response.is_server_error());
  Ok(())
}

#[test]
fn test_response_method_text() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let response = client.get(format!("{}/get", HTTPBIN)).call()?;
  let text = response.text()?;
  assert!(!text.is_empty());
  assert!(text.contains("url"));
  Ok(())
}

#[test]
fn test_response_method_bytes() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let response = client.get(format!("{}/bytes/100", HTTPBIN)).call()?;
  let bytes = response.bytes();
  assert_eq!(bytes.len(), 100);
  Ok(())
}

#[test]
fn test_response_method_into_bytes() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let response = client.get(format!("{}/bytes/50", HTTPBIN)).call()?;
  let bytes = response.into_bytes();
  assert_eq!(bytes.len(), 50);
  Ok(())
}

// ============================================================================
// Response Fields
// ============================================================================

#[test]
fn test_response_field_status_code() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let response = client.get(format!("{}/status/201", HTTPBIN)).call()?;
  assert_eq!(response.status_code, 201);
  Ok(())
}

#[test]
fn test_response_field_reason() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let response = client.get(format!("{}/get", HTTPBIN)).call()?;
  assert!(!response.reason.is_empty());
  Ok(())
}

#[test]
fn test_response_field_headers() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let response = client.get(format!("{}/get", HTTPBIN)).call()?;
  assert!(!response.headers.is_empty());

  // Check we can access headers
  let content_type = response.headers.get("content-type");
  assert!(content_type.is_some());
  Ok(())
}

#[test]
fn test_response_field_body() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let response = client.get(format!("{}/get", HTTPBIN)).call()?;
  assert!(!response.body.as_bytes().is_empty());
  Ok(())
}

#[test]
fn test_response_field_body_mut() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let mut response = client.get(format!("{}/get", HTTPBIN)).call()?;

  // Access body_mut
  let body_bytes = response.body_mut().as_bytes_mut();
  assert!(!body_bytes.is_empty());
  Ok(())
}

// ============================================================================
// Response Methods (non-ResponseExt)
// ============================================================================

#[test]
fn test_response_method_get_header() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let response = client.get(format!("{}/get", HTTPBIN)).call()?;

  // Test get_header method
  let content_type = response.get_header("content-type");
  assert!(content_type.is_some());
  assert!(content_type.unwrap().contains("json"));

  // Test non-existent header
  let nonexistent = response.get_header("X-Does-Not-Exist");
  assert!(nonexistent.is_none());
  Ok(())
}

#[test]
fn test_response_method_headers() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let response = client.get(format!("{}/get", HTTPBIN)).call()?;

  // Test headers() accessor
  let headers = response.headers();
  assert!(!headers.is_empty());
  assert!(headers.get("content-type").is_some());
  Ok(())
}

#[test]
fn test_response_method_headers_mut() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let mut response = client.get(format!("{}/get", HTTPBIN)).call()?;

  // Test headers_mut() accessor
  let headers_mut = response.headers_mut();
  let original_count = headers_mut.len();

  // Add a custom header
  headers_mut.insert("X-Test", "value");
  assert_eq!(headers_mut.len(), original_count + 1);
  Ok(())
}

#[test]
fn test_response_method_body() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let response = client.get(format!("{}/get", HTTPBIN)).call()?;

  // Test body() accessor
  let body = response.body();
  assert!(!body.as_bytes().is_empty());
  Ok(())
}

#[test]
fn test_response_method_body_mut_accessor() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let mut response = client.get(format!("{}/get", HTTPBIN)).call()?;

  // Test body_mut() accessor
  let body_mut = response.body_mut();
  let bytes = body_mut.as_bytes_mut();
  assert!(!bytes.is_empty());
  Ok(())
}

// ============================================================================
// Request Static Methods
// ============================================================================

#[test]
fn test_request_get() -> Result<(), Error> {
  let response = Request::get(format!("{}/get", HTTPBIN)).send()?;
  assert_eq!(response.status_code, 200);
  Ok(())
}

#[test]
fn test_request_post() -> Result<(), Error> {
  let response = Request::post(format!("{}/post", HTTPBIN))
    .body(b"test".to_vec())
    .send()?;
  assert_eq!(response.status_code, 200);
  Ok(())
}

#[test]
fn test_request_put() -> Result<(), Error> {
  let response = Request::put(format!("{}/put", HTTPBIN))
    .body(b"test".to_vec())
    .send()?;
  assert_eq!(response.status_code, 200);
  Ok(())
}

#[test]
fn test_request_delete() -> Result<(), Error> {
  let response = Request::delete(format!("{}/delete", HTTPBIN)).send()?;
  assert_eq!(response.status_code, 200);
  Ok(())
}

#[test]
fn test_request_head() -> Result<(), Error> {
  let response = Request::head(format!("{}/get", HTTPBIN)).send()?;
  assert_eq!(response.status_code, 200);
  Ok(())
}

#[test]
fn test_request_patch() -> Result<(), Error> {
  let response = Request::patch(format!("{}/patch", HTTPBIN))
    .body(b"test".to_vec())
    .send()?;
  assert_eq!(response.status_code, 200);
  Ok(())
}

#[test]
fn test_request_options() -> Result<(), Error> {
  let response = Request::options(format!("{}/get", HTTPBIN)).send()?;
  assert_eq!(response.status_code, 200);
  Ok(())
}

// ============================================================================
// Convenience Functions
// ============================================================================

#[test]
fn test_convenience_get() -> Result<(), Error> {
  let response = barehttp::get(&format!("{}/get", HTTPBIN))?;
  assert_eq!(response.status_code, 200);
  Ok(())
}

#[test]
fn test_convenience_post() -> Result<(), Error> {
  let response = barehttp::post(&format!("{}/post", HTTPBIN), b"test".to_vec())?;
  assert_eq!(response.status_code, 200);
  Ok(())
}

#[test]
fn test_convenience_put() -> Result<(), Error> {
  let response = barehttp::put(&format!("{}/put", HTTPBIN), b"test".to_vec())?;
  assert_eq!(response.status_code, 200);
  Ok(())
}

#[test]
fn test_convenience_delete() -> Result<(), Error> {
  let response = barehttp::delete(&format!("{}/delete", HTTPBIN))?;
  assert_eq!(response.status_code, 200);
  Ok(())
}

#[test]
fn test_convenience_head() -> Result<(), Error> {
  let response = barehttp::head(&format!("{}/get", HTTPBIN))?;
  assert_eq!(response.status_code, 200);
  Ok(())
}

#[test]
fn test_convenience_patch() -> Result<(), Error> {
  let response = barehttp::patch(&format!("{}/patch", HTTPBIN), b"test".to_vec())?;
  assert_eq!(response.status_code, 200);
  Ok(())
}

#[test]
fn test_convenience_options() -> Result<(), Error> {
  let response = barehttp::options(&format!("{}/get", HTTPBIN))?;
  assert_eq!(response.status_code, 200);
  Ok(())
}
