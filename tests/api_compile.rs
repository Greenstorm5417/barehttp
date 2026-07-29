//! Additional compile-pass API smoke (no trybuild).
//! Complements `tests/ui/pass` when trybuild is awkward on edition 2024.

use barehttp::config::Config;
use barehttp::{Headers, Method, Version, agent, get, post};

#[test]
fn construct_agent_get_post() {
  let _ = agent();
  let _ = get("http://example.com");
  let _ = post("http://example.com/api");
}

#[test]
fn config_builder_chain() {
  let _ = Config::builder()
    .user_agent("barehttp-test/0.1")
    .max_redirects(0)
    .max_response_body_size(1024)
    .build();
}

#[test]
fn headers_insert_and_get() {
  let mut h = Headers::new();
  h.insert("Content-Type", "text/plain");
  assert_eq!(h.get("content-type"), Some("text/plain"));
}

#[test]
fn method_and_version() {
  assert_eq!(Method::Get.as_str(), "GET");
  assert_eq!(Version::HTTP_11, Version::default());
}
