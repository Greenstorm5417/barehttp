//! Integration tests for Config and ConfigBuilder

use barehttp::config::{
  Config, ConfigBuilder, HttpStatusHandling, ProtocolRestriction, RedirectAuthHeaders,
  RedirectPolicy,
};
use core::time::Duration;

#[test]
fn test_config_default() {
  let config = Config::default();
  
  assert!(config.timeout.is_none());
  assert_eq!(config.user_agent, Some(String::from("barehttp/1.0")));
  assert_eq!(config.redirect_policy, RedirectPolicy::Follow);
  assert_eq!(config.max_redirects, 10);
  assert_eq!(config.http_status_handling, HttpStatusHandling::AsError);
  assert_eq!(config.redirect_auth_headers, RedirectAuthHeaders::Never);
  assert_eq!(config.max_response_header_size, 64 * 1024);
  assert!(config.timeout_connect.is_none());
  assert!(config.timeout_read.is_none());
  assert_eq!(config.accept, Some(String::from("*/*")));
  assert_eq!(config.protocol_restriction, ProtocolRestriction::Any);
  assert_eq!(config.connection_pooling, true);
  assert_eq!(config.max_idle_per_host, 5);
  assert_eq!(config.idle_timeout, Some(Duration::from_secs(90)));
}

#[test]
fn test_config_builder_new() {
  let builder = ConfigBuilder::new();
  let config = builder.build();
  
  assert_eq!(config.redirect_policy, RedirectPolicy::Follow);
}

#[test]
fn test_config_builder_timeout() {
  let config = ConfigBuilder::new()
    .timeout(Duration::from_secs(30))
    .build();
  
  assert_eq!(config.timeout, Some(Duration::from_secs(30)));
}

#[test]
fn test_config_builder_user_agent() {
  let config = ConfigBuilder::new()
    .user_agent("MyApp/1.0")
    .build();
  
  assert_eq!(config.user_agent, Some(String::from("MyApp/1.0")));
}

#[test]
fn test_config_builder_redirect_policy() {
  let config = ConfigBuilder::new()
    .redirect_policy(RedirectPolicy::NoFollow)
    .build();
  
  assert_eq!(config.redirect_policy, RedirectPolicy::NoFollow);
}

#[test]
fn test_config_builder_max_redirects() {
  let config = ConfigBuilder::new()
    .max_redirects(5)
    .build();
  
  assert_eq!(config.max_redirects, 5);
}

#[test]
fn test_config_builder_http_status_handling() {
  let config = ConfigBuilder::new()
    .http_status_handling(HttpStatusHandling::AsResponse)
    .build();
  
  assert_eq!(config.http_status_handling, HttpStatusHandling::AsResponse);
}

#[test]
fn test_config_builder_redirect_auth_headers() {
  let config = ConfigBuilder::new()
    .redirect_auth_headers(RedirectAuthHeaders::SameHost)
    .build();
  
  assert_eq!(config.redirect_auth_headers, RedirectAuthHeaders::SameHost);
}

#[test]
fn test_config_builder_max_response_header_size() {
  let config = ConfigBuilder::new()
    .max_response_header_size(128 * 1024)
    .build();
  
  assert_eq!(config.max_response_header_size, 128 * 1024);
}

#[test]
fn test_config_builder_timeout_connect() {
  let config = ConfigBuilder::new()
    .timeout_connect(Duration::from_secs(10))
    .build();
  
  assert_eq!(config.timeout_connect, Some(Duration::from_secs(10)));
}

#[test]
fn test_config_builder_timeout_read() {
  let config = ConfigBuilder::new()
    .timeout_read(Duration::from_secs(60))
    .build();
  
  assert_eq!(config.timeout_read, Some(Duration::from_secs(60)));
}

#[test]
fn test_config_builder_accept() {
  let config = ConfigBuilder::new()
    .accept("application/json")
    .build();
  
  assert_eq!(config.accept, Some(String::from("application/json")));
}

#[test]
fn test_config_builder_protocol_restriction() {
  let config = ConfigBuilder::new()
    .protocol_restriction(ProtocolRestriction::HttpsOnly)
    .build();
  
  assert_eq!(config.protocol_restriction, ProtocolRestriction::HttpsOnly);
}

#[test]
fn test_config_builder_connection_pooling() {
  let config = ConfigBuilder::new()
    .connection_pooling(false)
    .build();
  
  assert_eq!(config.connection_pooling, false);
}

#[test]
fn test_config_builder_max_idle_per_host() {
  let config = ConfigBuilder::new()
    .max_idle_per_host(10)
    .build();
  
  assert_eq!(config.max_idle_per_host, 10);
}

#[test]
fn test_config_builder_idle_timeout() {
  let config = ConfigBuilder::new()
    .idle_timeout(Duration::from_secs(120))
    .build();
  
  assert_eq!(config.idle_timeout, Some(Duration::from_secs(120)));
}

#[test]
fn test_config_builder_chaining() {
  let config = ConfigBuilder::new()
    .timeout(Duration::from_secs(30))
    .user_agent("TestClient/1.0")
    .max_redirects(3)
    .http_status_handling(HttpStatusHandling::AsResponse)
    .redirect_policy(RedirectPolicy::NoFollow)
    .build();
  
  assert_eq!(config.timeout, Some(Duration::from_secs(30)));
  assert_eq!(config.user_agent, Some(String::from("TestClient/1.0")));
  assert_eq!(config.max_redirects, 3);
  assert_eq!(config.http_status_handling, HttpStatusHandling::AsResponse);
  assert_eq!(config.redirect_policy, RedirectPolicy::NoFollow);
}

#[test]
fn test_config_builder_default() {
  let builder: ConfigBuilder = Default::default();
  let config = builder.build();
  
  assert_eq!(config.redirect_policy, RedirectPolicy::Follow);
}

#[test]
fn test_redirect_policy_variants() {
  assert_eq!(RedirectPolicy::Follow, RedirectPolicy::Follow);
  assert_eq!(RedirectPolicy::FollowReturnLast, RedirectPolicy::FollowReturnLast);
  assert_eq!(RedirectPolicy::NoFollow, RedirectPolicy::NoFollow);
  assert_ne!(RedirectPolicy::Follow, RedirectPolicy::NoFollow);
}

#[test]
fn test_http_status_handling_variants() {
  assert_eq!(HttpStatusHandling::AsError, HttpStatusHandling::AsError);
  assert_eq!(HttpStatusHandling::AsResponse, HttpStatusHandling::AsResponse);
  assert_ne!(HttpStatusHandling::AsError, HttpStatusHandling::AsResponse);
}

#[test]
fn test_redirect_auth_headers_variants() {
  assert_eq!(RedirectAuthHeaders::Never, RedirectAuthHeaders::Never);
  assert_eq!(RedirectAuthHeaders::SameHost, RedirectAuthHeaders::SameHost);
  assert_ne!(RedirectAuthHeaders::Never, RedirectAuthHeaders::SameHost);
}

#[test]
fn test_protocol_restriction_variants() {
  assert_eq!(ProtocolRestriction::HttpsOnly, ProtocolRestriction::HttpsOnly);
  assert_eq!(ProtocolRestriction::Any, ProtocolRestriction::Any);
  assert_ne!(ProtocolRestriction::HttpsOnly, ProtocolRestriction::Any);
}

#[test]
fn test_config_clone() {
  let config1 = ConfigBuilder::new()
    .timeout(Duration::from_secs(30))
    .build();
  
  let config2 = config1.clone();
  
  assert_eq!(config1.timeout, config2.timeout);
  assert_eq!(config1.max_redirects, config2.max_redirects);
}

#[test]
fn test_redirect_policy_debug() {
  let policy = RedirectPolicy::Follow;
  let debug_str = format!("{:?}", policy);
  assert!(debug_str.contains("Follow"));
}

#[test]
fn test_http_status_handling_debug() {
  let handling = HttpStatusHandling::AsError;
  let debug_str = format!("{:?}", handling);
  assert!(debug_str.contains("AsError"));
}

#[test]
fn test_redirect_auth_headers_debug() {
  let policy = RedirectAuthHeaders::Never;
  let debug_str = format!("{:?}", policy);
  assert!(debug_str.contains("Never"));
}

#[test]
fn test_protocol_restriction_debug() {
  let restriction = ProtocolRestriction::HttpsOnly;
  let debug_str = format!("{:?}", restriction);
  assert!(debug_str.contains("HttpsOnly"));
}
