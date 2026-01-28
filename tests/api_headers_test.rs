//! Integration tests for Headers struct and HeaderName

use barehttp::{HeaderName, Headers};

#[test]
fn test_headers_new() {
  let headers = Headers::new();
  assert!(headers.is_empty());
  assert_eq!(headers.len(), 0);
}

#[test]
fn test_headers_from_vec() {
  let vec = vec![
    (String::from("Content-Type"), String::from("text/plain")),
    (String::from("Accept"), String::from("*/*")),
  ];
  let headers = Headers::from_vec(vec);
  assert_eq!(headers.len(), 2);
  assert_eq!(headers.get("Content-Type"), Some("text/plain"));
}

#[test]
fn test_headers_insert() {
  let mut headers = Headers::new();
  headers.insert("X-Custom", "value");
  assert_eq!(headers.len(), 1);
  assert_eq!(headers.get("X-Custom"), Some("value"));
}

#[test]
fn test_headers_get() {
  let mut headers = Headers::new();
  headers.insert("Host", "example.com");
  assert_eq!(headers.get("Host"), Some("example.com"));
  assert_eq!(headers.get("Missing"), None);
}

#[test]
fn test_headers_get_case_insensitive() {
  let mut headers = Headers::new();
  headers.insert("Content-Type", "application/json");
  assert_eq!(headers.get("content-type"), Some("application/json"));
  assert_eq!(headers.get("CONTENT-TYPE"), Some("application/json"));
  assert_eq!(headers.get("CoNtEnT-tYpE"), Some("application/json"));
}

#[test]
fn test_headers_get_all() {
  let mut headers = Headers::new();
  headers.insert("Set-Cookie", "session=abc");
  headers.insert("Set-Cookie", "user=john");
  headers.insert("Set-Cookie", "theme=dark");
  
  let cookies = headers.get_all("Set-Cookie");
  assert_eq!(cookies.len(), 3);
  assert!(cookies.contains(&"session=abc"));
  assert!(cookies.contains(&"user=john"));
  assert!(cookies.contains(&"theme=dark"));
}

#[test]
fn test_headers_get_all_case_insensitive() {
  let mut headers = Headers::new();
  headers.insert("Accept", "text/html");
  headers.insert("accept", "application/json");
  
  let values = headers.get_all("ACCEPT");
  assert_eq!(values.len(), 2);
}

#[test]
fn test_headers_contains() {
  let mut headers = Headers::new();
  headers.insert("Authorization", "Bearer token");
  
  assert!(headers.contains("Authorization"));
  assert!(headers.contains("authorization"));
  assert!(headers.contains("AUTHORIZATION"));
  assert!(!headers.contains("Content-Type"));
}

#[test]
fn test_headers_remove() {
  let mut headers = Headers::new();
  headers.insert("X-Test", "value1");
  headers.insert("Content-Type", "text/plain");
  
  headers.remove("X-Test");
  
  assert!(!headers.contains("X-Test"));
  assert!(headers.contains("Content-Type"));
  assert_eq!(headers.len(), 1);
}

#[test]
fn test_headers_remove_case_insensitive() {
  let mut headers = Headers::new();
  headers.insert("Cache-Control", "no-cache");
  headers.insert("cache-control", "no-store");
  
  headers.remove("CACHE-CONTROL");
  
  assert_eq!(headers.len(), 0);
}

#[test]
fn test_headers_iter() {
  let mut headers = Headers::new();
  headers.insert("Host", "example.com");
  headers.insert("Accept", "*/*");
  
  let count = headers.iter().count();
  assert_eq!(count, 2);
}

#[test]
fn test_headers_len() {
  let mut headers = Headers::new();
  assert_eq!(headers.len(), 0);
  
  headers.insert("X-Test", "value");
  assert_eq!(headers.len(), 1);
  
  headers.insert("X-Another", "value2");
  assert_eq!(headers.len(), 2);
}

#[test]
fn test_headers_is_empty() {
  let mut headers = Headers::new();
  assert!(headers.is_empty());
  
  headers.insert("X-Test", "value");
  assert!(!headers.is_empty());
}

#[test]
fn test_headers_as_vec() {
  let mut headers = Headers::new();
  headers.insert("Host", "example.com");
  
  let vec = headers.as_vec();
  assert_eq!(vec.len(), 1);
  assert_eq!(vec[0].0, "Host");
  assert_eq!(vec[0].1, "example.com");
}

#[test]
fn test_headers_as_vec_mut() {
  let mut headers = Headers::new();
  headers.insert("X-Test", "value");
  
  let vec_mut = headers.as_vec_mut();
  vec_mut.push((String::from("X-New"), String::from("new-value")));
  
  assert_eq!(headers.len(), 2);
  assert_eq!(headers.get("X-New"), Some("new-value"));
}

#[test]
fn test_headers_into_vec() {
  let mut headers = Headers::new();
  headers.insert("Host", "example.com");
  headers.insert("Accept", "*/*");
  
  let vec = headers.into_vec();
  assert_eq!(vec.len(), 2);
}

#[test]
fn test_headers_from_vec_trait() {
  let vec = vec![
    (String::from("X-Test"), String::from("value")),
  ];
  let headers: Headers = vec.into();
  assert_eq!(headers.len(), 1);
}

#[test]
fn test_headers_into_iterator_ref() {
  let mut headers = Headers::new();
  headers.insert("Host", "example.com");
  headers.insert("Accept", "*/*");
  
  let mut count = 0;
  for _ in &headers {
    count += 1;
  }
  assert_eq!(count, 2);
}

#[test]
fn test_headers_into_iterator_owned() {
  let mut headers = Headers::new();
  headers.insert("Host", "example.com");
  headers.insert("Accept", "*/*");
  
  let mut count = 0;
  for _ in headers {
    count += 1;
  }
  assert_eq!(count, 2);
}

#[test]
fn test_headers_clone() {
  let mut headers1 = Headers::new();
  headers1.insert("X-Test", "value");
  
  let headers2 = headers1.clone();
  assert_eq!(headers1.len(), headers2.len());
  assert_eq!(headers1.get("X-Test"), headers2.get("X-Test"));
}

#[test]
fn test_headers_equality() {
  let mut headers1 = Headers::new();
  headers1.insert("Host", "example.com");
  
  let mut headers2 = Headers::new();
  headers2.insert("Host", "example.com");
  
  assert_eq!(headers1, headers2);
}

#[test]
fn test_headers_default() {
  let headers: Headers = Default::default();
  assert!(headers.is_empty());
}

#[test]
fn test_header_name_constants() {
  assert_eq!(HeaderName::ACCEPT, "accept");
  assert_eq!(HeaderName::CONTENT_TYPE, "content-type");
  assert_eq!(HeaderName::AUTHORIZATION, "authorization");
  assert_eq!(HeaderName::USER_AGENT, "user-agent");
  assert_eq!(HeaderName::HOST, "host");
  assert_eq!(HeaderName::COOKIE, "cookie");
  assert_eq!(HeaderName::SET_COOKIE, "set-cookie");
  assert_eq!(HeaderName::CONTENT_LENGTH, "content-length");
  assert_eq!(HeaderName::LOCATION, "location");
  assert_eq!(HeaderName::CACHE_CONTROL, "cache-control");
}

#[test]
fn test_header_name_more_constants() {
  assert_eq!(HeaderName::ACCEPT_ENCODING, "accept-encoding");
  assert_eq!(HeaderName::ACCEPT_LANGUAGE, "accept-language");
  assert_eq!(HeaderName::CONNECTION, "connection");
  assert_eq!(HeaderName::DATE, "date");
  assert_eq!(HeaderName::ETAG, "etag");
  assert_eq!(HeaderName::EXPIRES, "expires");
  assert_eq!(HeaderName::IF_MODIFIED_SINCE, "if-modified-since");
  assert_eq!(HeaderName::IF_NONE_MATCH, "if-none-match");
  assert_eq!(HeaderName::LAST_MODIFIED, "last-modified");
  assert_eq!(HeaderName::REFERER, "referer");
}
