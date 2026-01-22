use crate::parser::*;
extern crate alloc;
use alloc::vec::Vec;

#[test]
fn test_body_with_content_length() {
  let input = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nHello";
  let result = Response::parse(input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.body.as_bytes(), b"Hello");
}

#[test]
fn test_body_content_length_zero() {
  let input = b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert!(response.body.is_empty());
}

#[test]
fn test_body_incomplete_content_length() {
  let input = b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nShort";
  let result = Response::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_body_1xx_status_no_body() {
  let input = b"HTTP/1.1 100 Continue\r\nContent-Length: 5\r\n\r\nHello";
  let result = Response::parse(input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert!(response.body.is_empty());
}

#[test]
fn test_body_204_no_content_ignores_body() {
  let input = b"HTTP/1.1 204 No Content\r\nContent-Length: 5\r\n\r\nHello";
  let result = Response::parse(input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert!(response.body.is_empty());
}

#[test]
fn test_body_304_not_modified_no_body() {
  let input = b"HTTP/1.1 304 Not Modified\r\nContent-Length: 10\r\n\r\nIgnoredXXX";
  let result = Response::parse(input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert!(response.body.is_empty());
}

#[test]
fn test_body_both_transfer_encoding_and_content_length() {
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Length: 100\r\n\r\n5\r\nHello\r\n0\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.body.as_bytes(), b"Hello");
}

#[test]
fn test_body_invalid_content_length_non_numeric() {
  let input = b"HTTP/1.1 200 OK\r\nContent-Length: abc\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert!(response.body.is_empty());
}

#[test]
fn test_body_content_length_with_extra_data() {
  let input = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nHelloExtraData";
  let result = Response::parse(input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.body.as_bytes(), b"Hello");
}

#[test]
fn test_body_no_content_length_no_transfer_encoding() {
  let input = b"HTTP/1.1 200 OK\r\n\r\nSome body content";
  let result = Response::parse(input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert!(response.body.is_empty());
}

#[test]
fn test_body_content_length_whitespace() {
  let input = b"HTTP/1.1 200 OK\r\nContent-Length:  5  \r\n\r\nHello";
  let result = Response::parse(input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.body.as_bytes(), b"Hello");
}

#[test]
fn test_body_multiple_content_length_same_value() {
  let input = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nContent-Length: 5\r\n\r\nHello";
  let result = Response::parse(input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.body.as_bytes(), b"Hello");
}

#[test]
fn test_body_transfer_encoding_not_chunked() {
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip\r\n\r\nCompressedData";
  let result = Response::parse(input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.body.as_bytes(), b"CompressedData");
}

#[test]
fn test_body_content_length_larger_than_available() {
  let input = b"HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\nShortBody";
  let result = Response::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_body_with_binary_data() {
  let mut input = Vec::from(&b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\n"[..]);
  input.extend_from_slice(&[0x00, 0xFF, 0xAA, 0x55, 0xCC]);
  let result = Response::parse(&input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.body.as_bytes(), &[0x00, 0xFF, 0xAA, 0x55, 0xCC]);
}

#[test]
fn test_body_head_request_response() {
  let input = b"HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_err());
}
