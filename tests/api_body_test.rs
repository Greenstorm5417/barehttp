//! Integration tests for Body struct and its methods

use barehttp::Body;

#[test]
fn test_body_empty() {
  let body = Body::empty();
  assert!(body.is_empty());
  assert_eq!(body.len(), 0);
  assert_eq!(body.as_bytes(), &[]);
}

#[test]
fn test_body_from_bytes() {
  let data = b"test data".to_vec();
  let body = Body::from_bytes(data.clone());
  assert_eq!(body.as_bytes(), data.as_slice());
  assert_eq!(body.len(), 9);
  assert!(!body.is_empty());
}

#[test]
fn test_body_from_string() {
  let s = String::from("hello world");
  let body = Body::from_string(s.clone());
  assert_eq!(body.as_bytes(), s.as_bytes());
  assert_eq!(body.len(), 11);
}

#[test]
fn test_body_as_bytes() {
  let body = Body::from_bytes(b"test".to_vec());
  let bytes = body.as_bytes();
  assert_eq!(bytes, b"test");
}

#[test]
fn test_body_len() {
  let body = Body::from_bytes(b"12345".to_vec());
  assert_eq!(body.len(), 5);
}

#[test]
fn test_body_is_empty() {
  let empty = Body::empty();
  assert!(empty.is_empty());

  let non_empty = Body::from_bytes(b"x".to_vec());
  assert!(!non_empty.is_empty());
}

#[test]
fn test_body_into_bytes() {
  let body = Body::from_bytes(b"data".to_vec());
  let bytes = body.into_bytes();
  assert_eq!(bytes, b"data");
}

#[test]
fn test_body_to_string() {
  let body = Body::from_bytes(b"hello".to_vec());
  let result = body.to_string();
  assert!(result.is_ok());
  assert_eq!(result.unwrap(), "hello");
}

#[test]
fn test_body_to_string_invalid_utf8() {
  let invalid_utf8 = vec![0xFF, 0xFE, 0xFD];
  let body = Body::from_bytes(invalid_utf8);
  let result = body.to_string();
  assert!(result.is_err());
}

#[test]
fn test_body_into_string() {
  let body = Body::from_bytes(b"world".to_vec());
  let result = body.into_string();
  assert!(result.is_ok());
  assert_eq!(result.unwrap(), "world");
}

#[test]
fn test_body_into_string_invalid_utf8() {
  let invalid_utf8 = vec![0xFF, 0xFE, 0xFD];
  let body = Body::from_bytes(invalid_utf8);
  let result = body.into_string();
  assert!(result.is_err());
}

#[test]
fn test_body_as_bytes_mut() {
  let mut body = Body::from_bytes(b"test".to_vec());
  let bytes_mut = body.as_bytes_mut();
  bytes_mut.push(b'!');
  assert_eq!(body.as_bytes(), b"test!");
}

#[test]
fn test_body_from_vec_u8() {
  let data = vec![1, 2, 3, 4, 5];
  let body: Body = data.clone().into();
  assert_eq!(body.as_bytes(), data.as_slice());
}

#[test]
fn test_body_from_string_trait() {
  let s = String::from("test");
  let body: Body = s.clone().into();
  assert_eq!(body.as_bytes(), s.as_bytes());
}

#[test]
fn test_body_from_str() {
  let body: Body = "test string".into();
  assert_eq!(body.as_bytes(), b"test string");
}

#[test]
fn test_body_as_ref() {
  let body = Body::from_bytes(b"reference".to_vec());
  let bytes_ref: &[u8] = body.as_ref();
  assert_eq!(bytes_ref, b"reference");
}

#[test]
fn test_body_clone() {
  let body1 = Body::from_bytes(b"original".to_vec());
  let body2 = body1.clone();
  assert_eq!(body1.as_bytes(), body2.as_bytes());
}

#[test]
fn test_body_equality() {
  let body1 = Body::from_bytes(b"same".to_vec());
  let body2 = Body::from_bytes(b"same".to_vec());
  let body3 = Body::from_bytes(b"different".to_vec());
  
  assert_eq!(body1, body2);
  assert_ne!(body1, body3);
}

#[test]
fn test_body_default() {
  let body: Body = Default::default();
  assert!(body.is_empty());
  assert_eq!(body.len(), 0);
}
