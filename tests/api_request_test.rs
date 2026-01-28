//! Integration tests for Request struct

use barehttp::{Error, Request};

fn httpbin_url() -> String {
  std::env::var("HTTPBIN_URL").unwrap_or_else(|_| "http://httpbin.org".to_string())
}

#[test]
fn test_request_get() {
  let request = Request::get("http://example.com");
  let (method, url, _, _) = request.into_parts();
  assert_eq!(method, barehttp::Method::Get);
  assert_eq!(url, "http://example.com");
}

#[test]
fn test_request_post() {
  let request = Request::post("http://example.com");
  let (method, _, _, _) = request.into_parts();
  assert_eq!(method, barehttp::Method::Post);
}

#[test]
fn test_request_put() {
  let request = Request::put("http://example.com");
  let (method, _, _, _) = request.into_parts();
  assert_eq!(method, barehttp::Method::Put);
}

#[test]
fn test_request_delete() {
  let request = Request::delete("http://example.com");
  let (method, _, _, _) = request.into_parts();
  assert_eq!(method, barehttp::Method::Delete);
}

#[test]
fn test_request_head() {
  let request = Request::head("http://example.com");
  let (method, _, _, _) = request.into_parts();
  assert_eq!(method, barehttp::Method::Head);
}

#[test]
fn test_request_patch() {
  let request = Request::patch("http://example.com");
  let (method, _, _, _) = request.into_parts();
  assert_eq!(method, barehttp::Method::Patch);
}

#[test]
fn test_request_options() {
  let request = Request::options("http://example.com");
  let (method, _, _, _) = request.into_parts();
  assert_eq!(method, barehttp::Method::Options);
}

#[test]
fn test_request_new() {
  let request = Request::new(barehttp::Method::Get, "http://example.com");
  let (method, url, headers, body) = request.into_parts();
  assert_eq!(method, barehttp::Method::Get);
  assert_eq!(url, "http://example.com");
  assert!(headers.is_empty());
  assert!(body.is_none());
}

#[test]
fn test_request_header() {
  let request = Request::get("http://example.com")
    .header("X-Custom", "value");
  
  let (_, _, headers, _) = request.into_parts();
  assert_eq!(headers.get("X-Custom"), Some("value"));
}

#[test]
fn test_request_header_chaining() {
  let request = Request::get("http://example.com")
    .header("X-First", "one")
    .header("X-Second", "two")
    .header("X-Third", "three");
  
  let (_, _, headers, _) = request.into_parts();
  assert_eq!(headers.get("X-First"), Some("one"));
  assert_eq!(headers.get("X-Second"), Some("two"));
  assert_eq!(headers.get("X-Third"), Some("three"));
}

#[test]
fn test_request_body() {
  let body_data = barehttp::Body::from_bytes(b"test data".to_vec());
  let request = Request::post("http://example.com")
    .body(body_data);
  
  let (_, _, _, body) = request.into_parts();
  assert!(body.is_some());
  assert_eq!(body.unwrap().as_bytes(), b"test data");
}

#[test]
fn test_request_body_from_string() {
  let request = Request::post("http://example.com")
    .body("string body");
  
  let (_, _, _, body) = request.into_parts();
  assert!(body.is_some());
  assert_eq!(body.unwrap().as_bytes(), b"string body");
}

#[test]
fn test_request_into_parts() {
  let request = Request::post("http://example.com/api")
    .header("Content-Type", "application/json")
    .body(barehttp::Body::from_bytes(b"{}".to_vec()));
  
  let (method, url, headers, body) = request.into_parts();
  
  assert_eq!(method, barehttp::Method::Post);
  assert_eq!(url, "http://example.com/api");
  assert_eq!(headers.get("Content-Type"), Some("application/json"));
  assert!(body.is_some());
}

#[test]
fn test_request_send() -> Result<(), Error> {
  let request = Request::get(format!("{}/get", httpbin_url()));
  let response = request.send()?;
  assert_eq!(response.status_code, 200);
  Ok(())
}

#[test]
fn test_request_send_with_headers() -> Result<(), Error> {
  let request = Request::get(format!("{}/headers", httpbin_url()))
    .header("X-Test-Header", "test-value");
  
  let response = request.send()?;
  assert_eq!(response.status_code, 200);
  Ok(())
}

#[test]
fn test_request_send_post_with_body() -> Result<(), Error> {
  let request = Request::post(format!("{}/post", httpbin_url()))
    .header("Content-Type", "text/plain")
    .body("test data");
  
  let response = request.send()?;
  assert_eq!(response.status_code, 200);
  Ok(())
}

#[test]
fn test_request_send_with_client() -> Result<(), Error> {
  let mut client = barehttp::HttpClient::new()?;
  let request = Request::get(format!("{}/get", httpbin_url()));
  let response = request.send_with(&mut client)?;
  assert_eq!(response.status_code, 200);
  Ok(())
}
