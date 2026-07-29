use crate::transport::PoolKey;
use alloc::string::String;

#[test]
fn pool_key_scheme_distinguishes_same_host_port() {
  let http = PoolKey::new(String::from("http"), "example.com", 443);
  let https = PoolKey::new(String::from("https"), "example.com", 443);
  assert_ne!(http, https);
}

#[test]
fn pool_key_lowercases_host() {
  let mixed = PoolKey::new(String::from("http"), "Example.COM", 80);
  let lower = PoolKey::new(String::from("http"), "example.com", 80);
  assert_eq!(mixed, lower);
}
