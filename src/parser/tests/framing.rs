use crate::parser::has_complete_headers;
use alloc::format;
use alloc::vec::Vec;

#[test]
fn test_has_complete_headers() {
  assert!(has_complete_headers(b"HTTP/1.1 200 OK\r\n\r\n"));
  assert!(has_complete_headers(b"HTTP/1.1 200 OK\r\n\r\nBody"));
  assert!(!has_complete_headers(b"HTTP/1.1 200 OK\r\n"));
  assert!(!has_complete_headers(b"HTTP/1.1"));
  assert!(!has_complete_headers(b""));
}

#[test]
fn test_incremental_header_detection() {
  let mut buffer = Vec::new();

  buffer.extend_from_slice(b"HTTP/1.1 200 OK\r\n");
  assert!(!has_complete_headers(&buffer));

  buffer.extend_from_slice(b"Content-Length: ");
  assert!(!has_complete_headers(&buffer));

  buffer.extend_from_slice(b"5\r\n");
  assert!(!has_complete_headers(&buffer));

  buffer.extend_from_slice(b"\r\n");
  assert!(has_complete_headers(&buffer));
}

#[test]
fn test_has_complete_headers_long() {
  let mut long_headers = b"HTTP/1.1 200 OK\r\n".to_vec();
  for i in 0..100 {
    long_headers.extend_from_slice(format!("X-Custom-{i}: value{i}\r\n").as_bytes());
  }
  long_headers.extend_from_slice(b"\r\n");
  assert!(has_complete_headers(&long_headers));
}
