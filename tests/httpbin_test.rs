//! Live httpbin checks. Ignored by default — needs network (or local httpbin).
//!
//! Run manually: `HTTPBIN_URL=http://127.0.0.1 cargo test --test httpbin_test -- --ignored`

use barehttp::config::Config;
use barehttp::{HttpClient, get, post};

fn httpbin_url() -> String {
  std::env::var("HTTPBIN_URL").unwrap_or_else(|_| "http://httpbin.org".to_string())
}

#[test]
#[ignore = "needs network / httpbin"]
fn get_ok() {
  let response = get(format!("{}/get", httpbin_url())).call().unwrap();
  assert_eq!(response.status_code(), 200);
  assert!(response.is_success());
}

#[test]
#[ignore = "needs network / httpbin"]
fn post_ok() {
  let response = post(format!("{}/post", httpbin_url()))
    .send(b"test")
    .unwrap();
  assert_eq!(response.status_code(), 200);
}

#[test]
#[ignore = "needs network / httpbin"]
fn delete_ok() {
  let response = barehttp::delete(format!("{}/delete", httpbin_url()))
    .call()
    .unwrap();
  assert_eq!(response.status_code(), 200);
}

#[test]
#[ignore = "needs network / httpbin"]
fn client_query_and_headers() {
  let client = HttpClient::new();
  let response = client
    .get(format!("{}/get", httpbin_url()))
    .query("foo", "bar")
    .header("X-Custom-Header", "test-value")
    .call()
    .unwrap();
  assert!(response.is_success());
  let body = response.to_text().unwrap();
  assert!(body.contains("foo"));
  assert!(body.contains("bar"));
}

#[test]
#[ignore = "needs network / httpbin"]
fn status_as_response() {
  let config = Config::builder().http_status_as_error(false).build();
  let client = HttpClient::with_config(config);
  let response = client
    .get(format!("{}/status/404", httpbin_url()))
    .call()
    .unwrap();
  assert_eq!(response.status_code(), 404);
  assert!(response.is_client_error());
}

#[test]
#[ignore = "needs network / httpbin"]
fn redirect_no_follow() {
  let config = Config::builder().max_redirects(0).build();
  let client = HttpClient::with_config(config);
  let response = client
    .get(format!("{}/redirect/1", httpbin_url()))
    .call()
    .unwrap();
  assert!(response.is_redirect());
}
