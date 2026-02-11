//! Integration tests for response module and ResponseExt trait

use barehttp::response::ResponseExt;
use barehttp::{Error, HttpClient};

fn httpbin_url() -> String {
  std::env::var("HTTPBIN_URL").unwrap_or_else(|_| "http://httpbin.org".to_string())
}

#[test]
fn test_response_status() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client.get(format!("{}/status/200", httpbin_url())).call()?;
  assert_eq!(response.status(), 200);
  Ok(())
}

#[test]
fn test_response_cookies() -> Result<(), Error> {
  use barehttp::config::{Config, RedirectPolicy};

  let config = Config {
    redirect_policy: RedirectPolicy::NoFollow,
    ..Default::default()
  };
  let client = HttpClient::with_config(config)?;

  let response = client
    .get(format!("{}/cookies/set?test=value", httpbin_url()))
    .call()?;

  let cookies = response.cookies();
  assert!(response.status_code == 302 || !cookies.is_empty());
  Ok(())
}

#[test]
fn test_response_is_success() -> Result<(), Error> {
  let client = HttpClient::new()?;

  let response = client.get(format!("{}/status/200", httpbin_url())).call()?;
  assert!(response.is_success());

  let response = client.get(format!("{}/status/201", httpbin_url())).call()?;
  assert!(response.is_success());

  let response = client.get(format!("{}/status/299", httpbin_url())).call()?;
  assert!(response.is_success());

  Ok(())
}

#[test]
fn test_response_is_redirect() -> Result<(), Error> {
  use barehttp::config::{Config, RedirectPolicy};

  let config = Config {
    redirect_policy: RedirectPolicy::NoFollow,
    ..Default::default()
  };
  let client = HttpClient::with_config(config)?;

  let response = client.get(format!("{}/redirect/1", httpbin_url())).call()?;
  assert!(response.is_redirect());
  Ok(())
}

#[test]
fn test_response_is_client_error() -> Result<(), Error> {
  use barehttp::config::{Config, HttpStatusHandling};

  let config = Config {
    http_status_handling: HttpStatusHandling::AsResponse,
    ..Default::default()
  };
  let client = HttpClient::with_config(config)?;

  let response = client.get(format!("{}/status/404", httpbin_url())).call()?;
  assert!(response.is_client_error());

  let response = client.get(format!("{}/status/400", httpbin_url())).call()?;
  assert!(response.is_client_error());

  Ok(())
}

#[test]
fn test_response_is_server_error() -> Result<(), Error> {
  use barehttp::config::{Config, HttpStatusHandling};

  let config = Config {
    http_status_handling: HttpStatusHandling::AsResponse,
    ..Default::default()
  };
  let client = HttpClient::with_config(config)?;

  let response = client.get(format!("{}/status/500", httpbin_url())).call()?;
  assert!(response.is_server_error());

  let response = client.get(format!("{}/status/503", httpbin_url())).call()?;
  assert!(response.is_server_error());

  Ok(())
}

#[test]
fn test_response_text() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client.get(format!("{}/get", httpbin_url())).call()?;
  let text = response.text()?;
  assert!(!text.is_empty());
  assert!(text.contains("url"));
  Ok(())
}

#[test]
fn test_response_bytes() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client.get(format!("{}/bytes/100", httpbin_url())).call()?;
  let bytes = response.bytes();
  assert_eq!(bytes.len(), 100);
  Ok(())
}

#[test]
fn test_response_into_bytes() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client.get(format!("{}/bytes/50", httpbin_url())).call()?;
  let bytes = response.into_bytes();
  assert_eq!(bytes.len(), 50);
  Ok(())
}

#[test]
fn test_response_field_status_code() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client.get(format!("{}/status/201", httpbin_url())).call()?;
  assert_eq!(response.status_code, 201);
  Ok(())
}

#[test]
fn test_response_field_reason() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client.get(format!("{}/get", httpbin_url())).call()?;
  assert!(!response.reason.is_empty());
  Ok(())
}

#[test]
fn test_response_field_headers() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client.get(format!("{}/get", httpbin_url())).call()?;
  assert!(!response.headers.is_empty());

  let content_type = response.headers.get("content-type");
  assert!(content_type.is_some());
  Ok(())
}

#[test]
fn test_response_field_body() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client.get(format!("{}/get", httpbin_url())).call()?;
  assert!(!response.body.as_bytes().is_empty());
  Ok(())
}

#[test]
fn test_response_field_body_mut() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let mut response = client.get(format!("{}/get", httpbin_url())).call()?;

  let body_bytes = response.body_mut().as_bytes_mut();
  assert!(!body_bytes.is_empty());
  Ok(())
}

#[test]
fn test_response_get_header() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client.get(format!("{}/get", httpbin_url())).call()?;

  let content_type = response.get_header("content-type");
  assert!(content_type.is_some());
  assert!(content_type.unwrap().contains("json"));

  let nonexistent = response.get_header("X-Does-Not-Exist");
  assert!(nonexistent.is_none());
  Ok(())
}

#[test]
fn test_response_headers_accessor() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client.get(format!("{}/get", httpbin_url())).call()?;

  let headers = response.headers();
  assert!(!headers.is_empty());
  assert!(headers.get("content-type").is_some());
  Ok(())
}

#[test]
fn test_response_headers_mut_accessor() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let mut response = client.get(format!("{}/get", httpbin_url())).call()?;

  let headers_mut = response.headers_mut();
  let original_count = headers_mut.len();

  headers_mut.insert("X-Test", "value");
  assert_eq!(headers_mut.len(), original_count + 1);
  Ok(())
}

#[test]
fn test_response_body_accessor() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client.get(format!("{}/get", httpbin_url())).call()?;

  let body = response.body();
  assert!(!body.as_bytes().is_empty());
  Ok(())
}

#[test]
fn test_response_body_mut_accessor() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let mut response = client.get(format!("{}/get", httpbin_url())).call()?;

  let body_mut = response.body_mut();
  let bytes = body_mut.as_bytes_mut();
  assert!(!bytes.is_empty());
  Ok(())
}
