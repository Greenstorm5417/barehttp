//! Integration tests for OsBlockingSocket and OsDnsResolver

use barehttp::{OsBlockingSocket, OsDnsResolver};

#[test]
fn test_os_dns_resolver_new() {
  let _resolver = OsDnsResolver::new();
}

#[test]
fn test_os_blocking_socket_type_exists() {
  let _phantom = std::marker::PhantomData::<OsBlockingSocket>;
}
