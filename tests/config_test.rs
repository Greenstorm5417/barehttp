//! Configuration integration tests

use barehttp::config::{ConfigBuilder, HttpStatusHandling, RedirectPolicy};
use barehttp::response::ResponseExt;
use barehttp::{Error, HttpClient};
use core::time::Duration;

fn httpbin_url() -> String {
  std::env::var("HTTPBIN_URL").unwrap_or_else(|_| "http://httpbin.org".to_string())
}

#[test]
fn test_config_with_timeout() -> Result<(), Error> {
  let config = ConfigBuilder::new()
    .timeout(Duration::from_secs(30))
    .build();

  let client = HttpClient::with_config(config)?;
  let response = client.get(format!("{}/delay/1", httpbin_url())).call()?;

  assert!(response.is_success());

  Ok(())
}

#[test]
fn test_config_custom_user_agent() -> Result<(), Error> {
  let config = ConfigBuilder::new()
    .user_agent("barehttp-integration-test/1.0")
    .build();

  let client = HttpClient::with_config(config)?;
  let response = client.get(format!("{}/user-agent", httpbin_url())).call()?;

  assert!(response.is_success());
  let body = response.text()?;
  assert!(body.contains("barehttp-integration-test"));

  Ok(())
}

#[test]
fn test_config_http_status_as_response() -> Result<(), Error> {
  let config = ConfigBuilder::new()
    .http_status_handling(HttpStatusHandling::AsResponse)
    .build();

  let client = HttpClient::with_config(config)?;

  // This should NOT return an error, even though it's a 404
  let response = client.get(format!("{}/status/404", httpbin_url())).call()?;
  assert_eq!(response.status_code, 404);
  assert!(response.is_client_error());

  Ok(())
}

#[test]
fn test_config_redirect_no_follow() -> Result<(), Error> {
  let config = ConfigBuilder::new()
    .redirect_policy(RedirectPolicy::NoFollow)
    .build();

  let client = HttpClient::with_config(config)?;
  let response = client.get(format!("{}/redirect/1", httpbin_url())).call()?;

  // Should get the redirect response, not the final destination
  assert!(response.is_redirect());
  assert!(response.status_code >= 300 && response.status_code < 400);

  Ok(())
}

#[test]
fn test_config_max_redirects() -> Result<(), Error> {
  let config = ConfigBuilder::new()
    .max_redirects(5)
    .redirect_policy(RedirectPolicy::Follow)
    .build();

  let client = HttpClient::with_config(config)?;

  // Follow 2 redirects (within limit)
  let response = client.get(format!("{}/redirect/2", httpbin_url())).call()?;
  assert!(response.is_success());

  Ok(())
}

#[test]
fn test_multiple_requests_same_client() -> Result<(), Error> {
  let client = HttpClient::new()?;

  // First request
  let response1 = client.get(format!("{}/get", httpbin_url())).call()?;
  assert!(response1.is_success());

  // Second request with different path
  let response2 = client.get(format!("{}/user-agent", httpbin_url())).call()?;
  assert!(response2.is_success());

  // Third request with POST
  let response3 = client
    .post(format!("{}/post", httpbin_url()))
    .send(b"data".to_vec())?;
  assert!(response3.is_success());

  Ok(())
}
