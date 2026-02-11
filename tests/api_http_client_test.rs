//! Integration tests for HttpClient struct and its methods

use barehttp::{Error, HttpClient};

fn httpbin_url() -> String {
  std::env::var("HTTPBIN_URL").unwrap_or_else(|_| "http://httpbin.org".to_string())
}

#[test]
fn test_http_client_new() -> Result<(), Error> {
  let _client = HttpClient::new()?;
  Ok(())
}

#[test]
fn test_http_client_with_config() -> Result<(), Error> {
  use barehttp::config::ConfigBuilder;
  use core::time::Duration;

  let config = ConfigBuilder::new()
    .timeout(Duration::from_secs(30))
    .build();

  let _client = HttpClient::with_config(config)?;
  Ok(())
}

#[test]
fn test_http_client_new_with_adapters() {
  use barehttp::{OsBlockingSocket, OsDnsResolver};

  let dns = OsDnsResolver::new();
  let _client: HttpClient<OsBlockingSocket, _> = HttpClient::new_with_adapters(dns);
}

#[test]
fn test_http_client_with_adapters_and_config() {
  use barehttp::config::Config;
  use barehttp::{OsBlockingSocket, OsDnsResolver};

  let dns = OsDnsResolver::new();
  let config = Config::default();
  let _client: HttpClient<OsBlockingSocket, _> = HttpClient::with_adapters_and_config(dns, config);
}

#[test]
fn test_http_client_get() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client.get(format!("{}/get", httpbin_url())).call()?;
  assert_eq!(response.status_code, 200);
  Ok(())
}

#[test]
fn test_http_client_post() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client
    .post(format!("{}/post", httpbin_url()))
    .send(b"test data".to_vec())?;
  assert_eq!(response.status_code, 200);
  Ok(())
}

#[test]
fn test_http_client_put() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client
    .put(format!("{}/put", httpbin_url()))
    .send(b"test data".to_vec())?;
  assert_eq!(response.status_code, 200);
  Ok(())
}

#[test]
fn test_http_client_delete() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client.delete(format!("{}/delete", httpbin_url())).call()?;
  assert_eq!(response.status_code, 200);
  Ok(())
}

#[test]
fn test_http_client_head() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client.head(format!("{}/get", httpbin_url())).call()?;
  assert_eq!(response.status_code, 200);
  assert!(response.body.as_bytes().is_empty());
  Ok(())
}

#[test]
fn test_http_client_options() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client.options(format!("{}/get", httpbin_url())).call()?;
  assert_eq!(response.status_code, 200);
  Ok(())
}

#[test]
fn test_http_client_patch() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let response = client
    .patch(format!("{}/patch", httpbin_url()))
    .send(b"test data".to_vec())?;
  assert_eq!(response.status_code, 200);
  Ok(())
}

#[test]
fn test_http_client_trace() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let _ = client.trace(format!("{}/get", httpbin_url())).call();
  Ok(())
}

#[test]
fn test_http_client_connect() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let _ = client.connect(format!("{}/get", httpbin_url())).call();
  Ok(())
}

#[test]
fn test_http_client_run() -> Result<(), Error> {
  use barehttp::Request;

  let client = HttpClient::new()?;
  let request = Request::get(format!("{}/get", httpbin_url()));
  let response = client.run(request)?;
  assert_eq!(response.status_code, 200);
  Ok(())
}

#[test]
fn test_http_client_clone() -> Result<(), Error> {
  let client1 = HttpClient::new()?;
  let client2 = client1.clone();

  let response1 = client1.get(format!("{}/get", httpbin_url())).call()?;
  let response2 = client2.get(format!("{}/get", httpbin_url())).call()?;

  assert_eq!(response1.status_code, 200);
  assert_eq!(response2.status_code, 200);
  Ok(())
}

#[test]
#[cfg(feature = "cookie-jar")]
fn test_http_client_cookie_store() -> Result<(), Error> {
  let client = HttpClient::new()?;
  let _cookie_store = client.cookie_store();
  Ok(())
}
