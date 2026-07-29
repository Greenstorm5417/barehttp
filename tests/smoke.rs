//! Offline public-API smoke. Deep coverage lives in `src/` unit tests.

use barehttp::config::{Config, ConfigBuilder, RedirectPolicy};
use barehttp::{Headers, HttpClient, Method, OsBlockingSocket, OsDnsResolver, StatusCode};
use core::time::Duration;

#[test]
fn client_constructors() {
  let _ = HttpClient::new();

  let config = ConfigBuilder::new()
    .timeout(Duration::from_secs(30))
    .max_redirects(5)
    .user_agent("smoke/1.0")
    .redirect_policy(RedirectPolicy::NoFollow)
    .build();
  let _ = HttpClient::with_config(config);

  let _client: HttpClient<OsBlockingSocket, _> =
    HttpClient::new_with_adapters(OsDnsResolver::new());
  let _client: HttpClient<OsBlockingSocket, _> =
    HttpClient::with_adapters_and_config(OsDnsResolver::new(), Config::default());
}

#[test]
fn config_builder_basics() {
  let config = ConfigBuilder::new()
    .timeout(Duration::from_secs(10))
    .user_agent("app/1.0")
    .max_redirects(3)
    .redirect_policy(RedirectPolicy::NoFollow)
    .build();

  assert_eq!(config.timeout, Some(Duration::from_secs(10)));
  assert_eq!(config.user_agent.as_deref(), Some("app/1.0"));
  assert_eq!(config.max_redirects, 3);
  assert_eq!(config.redirect_policy, RedirectPolicy::NoFollow);
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

  headers.remove("x-custom");
  assert!(!headers.contains("X-Custom"));
}

#[test]
fn method_and_status() {
  assert_eq!(Method::Get.as_str(), "GET");
  assert_eq!(Method::Post.as_str(), "POST");
  assert!(Method::Post.has_body());
  assert!(Method::Get.without_body());
  assert_eq!("DELETE".parse::<Method>().unwrap(), Method::Delete);

  let ok = StatusCode::new(200).unwrap();
  assert!(ok.is_successful());
  assert_eq!(ok.as_u16(), 200);
  assert!(StatusCode::new(404).unwrap().is_client_error());
  assert!(StatusCode::new(99).is_none());
}
