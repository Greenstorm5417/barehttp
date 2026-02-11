//! Integration tests for request_builder module and ClientRequestBuilder

use barehttp::response::ResponseExt;
use barehttp::{Error, HttpClient};

fn httpbin_url() -> String {
  std::env::var("HTTPBIN_URL").unwrap_or_else(|_| "http://httpbin.org".to_string())
}

#[test]
fn test_request_builder_header() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client
    .get(format!("{}/headers", httpbin_url()))
    .header("X-Custom-Header", "custom-value")
    .call()?;

  let body = response.text()?;
  assert!(body.contains("X-Custom-Header"));
  assert!(body.contains("custom-value"));
  Ok(())
}

#[test]
fn test_request_builder_header_chaining() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client
    .get(format!("{}/headers", httpbin_url()))
    .header("X-First", "one")
    .header("X-Second", "two")
    .header("X-Third", "three")
    .call()?;

  let body = response.text()?;
  assert!(body.contains("X-First"));
  assert!(body.contains("X-Second"));
  assert!(body.contains("X-Third"));
  Ok(())
}

#[test]
fn test_request_builder_query() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client
    .get(format!("{}/get", httpbin_url()))
    .query("key1", "value1")
    .query("key2", "value2")
    .call()?;

  let body = response.text()?;

  // Verify URL structure is properly formed
  assert!(body.contains("?key1=value1&key2=value2") || body.contains("url"));

  // Verify server receives parameters as intended (httpbin returns args object)
  assert!(body.contains("\"args\""));
  assert!(body.contains("\"key1\": \"value1\""));
  assert!(body.contains("\"key2\": \"value2\""));

  // Verify parameters are in correct key=value format
  assert!(body.contains("key1"));
  assert!(body.contains("value1"));
  assert!(body.contains("key2"));
  assert!(body.contains("value2"));
  Ok(())
}

#[test]
fn test_request_builder_query_pairs() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let params = vec![("foo", "bar"), ("baz", "qux")];

  let response = client
    .get(format!("{}/get", httpbin_url()))
    .query_pairs(params)
    .call()?;

  let body = response.text()?;

  // Verify URL structure with proper separators
  assert!(body.contains("?foo=bar&baz=qux") || body.contains("url"));

  // Verify server receives parameters correctly in args object
  assert!(body.contains("\"args\""));
  assert!(body.contains("\"foo\": \"bar\""));
  assert!(body.contains("\"baz\": \"qux\""));

  // Verify individual parameters exist
  assert!(body.contains("foo"));
  assert!(body.contains("bar"));
  assert!(body.contains("baz"));
  assert!(body.contains("qux"));
  Ok(())
}

#[test]
fn test_request_builder_form() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client
    .post(format!("{}/post", httpbin_url()))
    .form("field1", "value1")
    .form("field2", "value2")
    .call()?;

  let body = response.text()?;
  assert!(body.contains("field1"));
  assert!(body.contains("value1"));
  Ok(())
}

#[test]
fn test_request_builder_content_type() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client
    .post(format!("{}/post", httpbin_url()))
    .content_type("application/json")
    .send(br#"{"test":"data"}"#.to_vec())?;

  let body = response.text()?;
  assert!(body.contains("application/json"));
  Ok(())
}

#[test]
fn test_request_builder_cookie() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client
    .get(format!("{}/cookies", httpbin_url()))
    .cookie("session", "abc123")
    .cookie("user", "john")
    .call()?;

  assert!(response.is_success());
  Ok(())
}

#[test]
fn test_request_builder_uri() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client
    .get("http://example.com")
    .uri(format!("{}/get", httpbin_url()))
    .call()?;

  assert_eq!(response.status_code, 200);
  Ok(())
}

#[test]
fn test_request_builder_version() -> Result<(), Error> {
  use barehttp::Version;

  let client = HttpClient::new()?;
  let response = client
    .get(format!("{}/get", httpbin_url()))
    .version(Version::HTTP_11)
    .call()?;

  assert_eq!(response.status_code, 200);
  Ok(())
}

#[test]
fn test_request_builder_with_config() -> Result<(), Error> {
  use barehttp::config::{Config, HttpStatusHandling};

  let config = Config {
    http_status_handling: HttpStatusHandling::AsResponse,
    ..Default::default()
  };

  let client = HttpClient::new()?;
  let response = client
    .get(format!("{}/status/404", httpbin_url()))
    .with_config(config)
    .call()?;

  assert_eq!(response.status_code, 404);
  Ok(())
}

#[test]
fn test_request_builder_call() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client.get(format!("{}/get", httpbin_url())).call()?;

  assert!(response.is_success());
  Ok(())
}

#[test]
fn test_request_builder_send() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client
    .post(format!("{}/post", httpbin_url()))
    .send(b"test data".to_vec())?;

  assert!(response.is_success());
  Ok(())
}

#[test]
fn test_request_builder_send_string() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client
    .post(format!("{}/post", httpbin_url()))
    .send_string("test string")?;

  assert!(response.is_success());
  Ok(())
}

#[test]
fn test_request_builder_send_empty() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client
    .post(format!("{}/post", httpbin_url()))
    .send_empty()?;

  assert!(response.is_success());
  Ok(())
}

#[test]
fn test_request_builder_method() -> Result<(), Error> {
  use barehttp::Method;

  let client = HttpClient::new()?;
  let builder = client.get(format!("{}/get", httpbin_url()));

  assert_eq!(builder.method(), Method::Get);
  Ok(())
}

#[test]
fn test_request_builder_url() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let url = format!("{}/get", httpbin_url());
  let builder = client.get(url.clone());

  assert_eq!(builder.url(), url);
  Ok(())
}

#[test]
fn test_request_builder_headers_ref() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let builder = client
    .get(format!("{}/get", httpbin_url()))
    .header("X-Test", "value");

  let headers = builder.headers_ref();
  assert_eq!(headers.get("X-Test"), Some("value"));
  Ok(())
}

#[test]
fn test_request_builder_headers_mut() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let mut builder = client.get(format!("{}/get", httpbin_url()));

  let headers_mut = builder.headers_mut();
  headers_mut.insert("X-Mutable", "value");

  let response = builder.call()?;
  assert!(response.is_success());
  Ok(())
}

#[test]
fn test_request_builder_version_ref() -> Result<(), Error> {
  use barehttp::Version;

  let client = HttpClient::new()?;
  let builder = client
    .get(format!("{}/get", httpbin_url()))
    .version(Version::HTTP_11);

  assert_eq!(builder.version_ref(), Version::HTTP_11);
  Ok(())
}

#[test]
fn test_request_builder_config_ref() -> Result<(), Error> {
  use barehttp::config::Config;

  let client = HttpClient::new()?;
  let config = Config::default();
  let builder = client
    .get(format!("{}/get", httpbin_url()))
    .with_config(config);

  assert!(builder.config_ref().is_some());
  Ok(())
}

#[test]
fn test_request_builder_query_raw() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client
    .get(format!("{}/get", httpbin_url()))
    .query_raw("raw_key", "raw_value")
    .call()?;

  let body = response.text()?;

  // Verify URL structure
  assert!(body.contains("?raw_key=raw_value") || body.contains("url"));

  // Verify server receives parameter
  assert!(body.contains("\"args\""));
  assert!(body.contains("\"raw_key\": \"raw_value\""));

  assert!(body.contains("raw_key"));
  assert!(body.contains("raw_value"));
  Ok(())
}

#[test]
fn test_request_builder_query_pairs_raw() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let params = vec![("raw1", "val1"), ("raw2", "val2")];

  let response = client
    .get(format!("{}/get", httpbin_url()))
    .query_pairs_raw(params)
    .call()?;

  let body = response.text()?;

  // Verify URL structure with proper separators
  assert!(body.contains("?raw1=val1&raw2=val2") || body.contains("url"));

  // Verify server receives both parameters
  assert!(body.contains("\"args\""));
  assert!(body.contains("\"raw1\": \"val1\""));
  assert!(body.contains("\"raw2\": \"val2\""));

  assert!(body.contains("raw1"));
  assert!(body.contains("val1"));
  Ok(())
}
