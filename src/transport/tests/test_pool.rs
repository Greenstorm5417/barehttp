use crate::transport::PoolKey;
use alloc::string::String;

#[test]
fn pool_key_scheme_distinguishes_same_host_port() {
  let http = PoolKey::new(String::from("http"), String::from("example.com"), 443);
  let https = PoolKey::new(String::from("https"), String::from("example.com"), 443);
  assert_ne!(http, https);
}
