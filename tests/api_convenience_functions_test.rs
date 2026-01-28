//! Integration tests for convenience functions (get, post, put, delete, etc.)

use barehttp::{self, Error};
use barehttp::response::ResponseExt;

fn httpbin_url() -> String {
  std::env::var("HTTPBIN_URL").unwrap_or_else(|_| "http://httpbin.org".to_string())
}

#[test]
fn test_convenience_get() -> Result<(), Error> {
  let response = barehttp::get(&format!("{}/get", httpbin_url()))?;
  assert_eq!(response.status_code, 200);
  assert!(response.is_success());
  Ok(())
}

#[test]
fn test_convenience_post() -> Result<(), Error> {
  let response = barehttp::post(&format!("{}/post", httpbin_url()), b"test data".to_vec())?;
  assert_eq!(response.status_code, 200);
  assert!(response.is_success());
  Ok(())
}

#[test]
fn test_convenience_post_with_string() -> Result<(), Error> {
  let response = barehttp::post(&format!("{}/post", httpbin_url()), "string data")?;
  assert_eq!(response.status_code, 200);
  Ok(())
}

#[test]
fn test_convenience_put() -> Result<(), Error> {
  let response = barehttp::put(&format!("{}/put", httpbin_url()), b"test data".to_vec())?;
  assert_eq!(response.status_code, 200);
  assert!(response.is_success());
  Ok(())
}

#[test]
fn test_convenience_delete() -> Result<(), Error> {
  let response = barehttp::delete(&format!("{}/delete", httpbin_url()))?;
  assert_eq!(response.status_code, 200);
  assert!(response.is_success());
  Ok(())
}

#[test]
fn test_convenience_head() -> Result<(), Error> {
  let response = barehttp::head(&format!("{}/get", httpbin_url()))?;
  assert_eq!(response.status_code, 200);
  assert!(response.body.as_bytes().is_empty());
  Ok(())
}

#[test]
fn test_convenience_options() -> Result<(), Error> {
  let response = barehttp::options(&format!("{}/get", httpbin_url()))?;
  assert_eq!(response.status_code, 200);
  Ok(())
}

#[test]
fn test_convenience_patch() -> Result<(), Error> {
  let response = barehttp::patch(&format!("{}/patch", httpbin_url()), b"test data".to_vec())?;
  assert_eq!(response.status_code, 200);
  assert!(response.is_success());
  Ok(())
}

#[test]
fn test_convenience_trace() -> Result<(), Error> {
  let _ = barehttp::trace(&format!("{}/get", httpbin_url()));
  Ok(())
}

#[test]
fn test_convenience_connect() -> Result<(), Error> {
  let _ = barehttp::connect(&format!("{}/get", httpbin_url()));
  Ok(())
}

#[test]
fn test_convenience_get_with_response_text() -> Result<(), Error> {
  let response = barehttp::get(&format!("{}/get", httpbin_url()))?;
  let text = response.text()?;
  assert!(!text.is_empty());
  assert!(text.contains("url"));
  Ok(())
}

#[test]
fn test_convenience_post_with_response_bytes() -> Result<(), Error> {
  let response = barehttp::post(&format!("{}/post", httpbin_url()), b"data".to_vec())?;
  let bytes = response.bytes();
  assert!(!bytes.is_empty());
  Ok(())
}
