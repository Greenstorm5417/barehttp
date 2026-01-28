//! Integration tests for Method enum

use barehttp::Method;

#[test]
fn test_method_as_str() {
  assert_eq!(Method::Get.as_str(), "GET");
  assert_eq!(Method::Post.as_str(), "POST");
  assert_eq!(Method::Put.as_str(), "PUT");
  assert_eq!(Method::Delete.as_str(), "DELETE");
  assert_eq!(Method::Head.as_str(), "HEAD");
  assert_eq!(Method::Options.as_str(), "OPTIONS");
  assert_eq!(Method::Patch.as_str(), "PATCH");
  assert_eq!(Method::Trace.as_str(), "TRACE");
  assert_eq!(Method::Connect.as_str(), "CONNECT");
}

#[test]
fn test_method_has_body() {
  assert!(!Method::Get.has_body());
  assert!(Method::Post.has_body());
  assert!(Method::Put.has_body());
  assert!(!Method::Delete.has_body());
  assert!(!Method::Head.has_body());
  assert!(!Method::Options.has_body());
  assert!(Method::Patch.has_body());
  assert!(!Method::Trace.has_body());
  assert!(!Method::Connect.has_body());
}

#[test]
fn test_method_without_body() {
  assert!(Method::Get.without_body());
  assert!(!Method::Post.without_body());
  assert!(!Method::Put.without_body());
  assert!(!Method::Delete.without_body());
  assert!(Method::Head.without_body());
  assert!(Method::Options.without_body());
  assert!(!Method::Patch.without_body());
  assert!(Method::Trace.without_body());
  assert!(Method::Connect.without_body());
}

#[test]
fn test_method_clone() {
  let method1 = Method::Get;
  let method2 = method1.clone();
  assert_eq!(method1, method2);
}

#[test]
fn test_method_copy() {
  let method1 = Method::Post;
  let method2 = method1;
  assert_eq!(method1, method2);
}

#[test]
fn test_method_equality() {
  assert_eq!(Method::Get, Method::Get);
  assert_ne!(Method::Get, Method::Post);
  assert_eq!(Method::Put, Method::Put);
}

#[test]
fn test_method_debug() {
  let method = Method::Get;
  let debug_str = format!("{:?}", method);
  assert!(debug_str.contains("Get"));
}

#[test]
fn test_method_hash() {
  use std::collections::HashSet;
  
  let mut set = HashSet::new();
  set.insert(Method::Get);
  set.insert(Method::Post);
  set.insert(Method::Get);
  
  assert_eq!(set.len(), 2);
  assert!(set.contains(&Method::Get));
  assert!(set.contains(&Method::Post));
}
