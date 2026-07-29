//! Offline public-API smoke.

use barehttp::config::Config;
use barehttp::{Headers, HttpClient, Method, OsBlockingSocket, OsDnsResolver};
use core::time::Duration;

#[test]
fn client_constructors() {
  let _ = HttpClient::new();

  let config = Config {
    timeout_read: Some(Duration::from_secs(30)),
    timeout_write: Some(Duration::from_secs(30)),
    max_redirects: 0,
    user_agent: String::from("smoke/1.0"),
    ..Default::default()
  };
  let _ = HttpClient::with_config(config);

  let _client: HttpClient<OsBlockingSocket, _> = HttpClient::with_adapters(OsDnsResolver, Config::default());
}

#[test]
fn config_struct_update() {
  let config = Config {
    timeout_read: Some(Duration::from_secs(10)),
    timeout_write: Some(Duration::from_secs(10)),
    user_agent: String::from("app/1.0"),
    max_redirects: 0,
    ..Default::default()
  };

  assert_eq!(config.timeout_read, Some(Duration::from_secs(10)));
  assert_eq!(config.timeout_write, Some(Duration::from_secs(10)));
  assert_eq!(config.user_agent, "app/1.0");
  assert_eq!(config.max_redirects, 0);
}

#[test]
fn headers_basics() {
  let mut headers = Headers::new();
  assert!(headers.is_empty());

  headers.insert("Content-Type", "application/json");
  headers.insert("X-Custom", "one");
  assert_eq!(headers.get("content-type"), Some("application/json"));
  assert!(headers.contains("X-Custom"));
  assert_eq!(headers.len(), 2);

  headers.set("content-type", "text/plain");
  assert_eq!(headers.get("Content-Type"), Some("text/plain"));
  assert_eq!(headers.get_all("content-type").len(), 1);

  headers.remove("x-custom");
  assert!(!headers.contains("X-Custom"));
}

#[test]
fn free_fn_builders() {
  let _ = barehttp::get("http://example.com");
  let _ = barehttp::post("http://example.com");
  let _ = barehttp::put("http://example.com");
  let _ = barehttp::delete("http://example.com");
  let _ = barehttp::head("http://example.com");
  let _ = barehttp::patch("http://example.com");
  let _ = barehttp::agent();
}

#[test]
fn method_basics() {
  assert_eq!(Method::Get.as_str(), "GET");
  assert_eq!(Method::Post.as_str(), "POST");
  assert_eq!(Method::Delete.as_str(), "DELETE");
}
