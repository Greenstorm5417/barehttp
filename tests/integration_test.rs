//! Integration tests for barehttp
//!
//! These tests verify the complete HTTP client functionality end-to-end.

use barehttp::response::ResponseExt;
use barehttp::{Error, HttpClient};

#[test]
fn test_simple_get_request() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let response = client.get("http://httpbin.org/get").call()?;

  assert!(response.is_success());
  assert_eq!(response.status_code, 200);
  assert!(!response.body.as_bytes().is_empty());

  Ok(())
}

#[test]
fn test_get_with_query_params() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let response = client
    .get("http://httpbin.org/get")
    .query("foo", "bar")
    .query("test", "123")
    .call()?;

  assert!(response.is_success());
  let body = response.text()?;
  assert!(body.contains("foo"));
  assert!(body.contains("bar"));

  Ok(())
}

#[test]
fn test_post_with_body() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let body_data = b"test data";

  let response = client
    .post("http://httpbin.org/post")
    .header("Content-Type", "text/plain")
    .send(body_data.to_vec())?;

  assert!(response.is_success());
  assert_eq!(response.status_code, 200);

  Ok(())
}

#[test]
fn test_custom_headers() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let response = client
    .get("http://httpbin.org/headers")
    .header("X-Custom-Header", "test-value")
    .header("User-Agent", "barehttp-test/1.0")
    .call()?;

  assert!(response.is_success());
  let body = response.text()?;
  assert!(body.contains("X-Custom-Header"));

  Ok(())
}

#[test]
fn test_put_request() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let response = client
    .put("http://httpbin.org/put")
    .send(b"update data".to_vec())?;

  assert!(response.is_success());
  assert_eq!(response.status_code, 200);

  Ok(())
}

#[test]
fn test_delete_request() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let response = client.delete("http://httpbin.org/delete").call()?;

  assert!(response.is_success());
  assert_eq!(response.status_code, 200);

  Ok(())
}

#[test]
fn test_head_request() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let response = client.head("http://httpbin.org/get").call()?;

  assert!(response.is_success());
  assert_eq!(response.status_code, 200);
  // HEAD responses should have empty body
  assert!(response.body.as_bytes().is_empty());

  Ok(())
}

#[test]
fn test_convenience_get_function() -> Result<(), Error> {
  let response = barehttp::get("http://httpbin.org/get")?;

  assert!(response.is_success());
  assert_eq!(response.status_code, 200);

  Ok(())
}

#[test]
fn test_convenience_post_function() -> Result<(), Error> {
  let response = barehttp::post("http://httpbin.org/post", b"test".to_vec())?;

  assert!(response.is_success());
  assert_eq!(response.status_code, 200);

  Ok(())
}

#[test]
fn test_response_helpers() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let response = client.get("http://httpbin.org/get").call()?;

  assert!(response.is_success());
  assert!(!response.is_redirect());
  assert!(!response.is_client_error());
  assert!(!response.is_server_error());

  let text = response.text()?;
  assert!(!text.is_empty());

  Ok(())
}

#[test]
fn test_json_content_type() -> Result<(), Error> {
  let mut client = HttpClient::new()?;
  let json_body = br#"{"key":"value"}"#;

  let response = client
    .post("http://httpbin.org/post")
    .header("Content-Type", "application/json")
    .send(json_body.to_vec())?;

  assert!(response.is_success());
  let body = response.text()?;
  assert!(body.contains("application/json"));

  Ok(())
}
