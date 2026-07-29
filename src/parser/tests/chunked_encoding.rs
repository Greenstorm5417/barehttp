use crate::parser::*;
extern crate alloc;
use alloc::vec::Vec;

#[test]
fn test_chunked_simple_single_chunk() {
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nHello\r\n0\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.body(), b"Hello");
}

#[test]
fn test_chunked_multiple_chunks() {
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nHello\r\n6\r\n World\r\n0\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.body(), b"Hello World");
}

#[test]
fn test_chunked_zero_size_terminator() {
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert!(response.body().is_empty());
}

#[test]
fn test_chunked_hex_size() {
  assert_eq!(
    Response::parse(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nA\r\n0123456789\r\n0\r\n\r\n")
      .unwrap()
      .body(),
    b"0123456789"
  );
  assert_eq!(
    Response::parse(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\na\r\n0123456789\r\n0\r\n\r\n")
      .unwrap()
      .body(),
    b"0123456789"
  );
}

#[test]
fn test_chunked_hex_mixed_case() {
  let mut input = Vec::from(&b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n1F\r\n"[..]);
  input.extend_from_slice(&[b'x'; 31]);
  input.extend_from_slice(b"\r\n0\r\n\r\n");
  let result = Response::parse(&input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.body().len(), 31);
}

#[test]
fn test_chunked_with_chunk_extension() {
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5;ext=value\r\nHello\r\n0\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.body(), b"Hello");
}

#[test]
fn test_chunked_with_trailer_fields() {
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nHello\r\n0\r\nX-Trailer: value\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.body(), b"Hello");
}

#[test]
fn test_chunked_missing_final_crlf() {
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nHello\r\n0\r\n";
  let result = Response::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_chunked_missing_chunk_data_crlf() {
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nHello0\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_chunked_invalid_hex_size() {
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nXYZ\r\nHello\r\n0\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_chunked_size_too_large_for_available_data() {
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n64\r\nShort\r\n0\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_chunked_empty_chunks_between_data() {
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nfoo\r\n3\r\nbar\r\n0\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.body(), b"foobar");
}

#[test]
fn test_chunked_with_leading_zeros_in_size() {
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n0005\r\nHello\r\n0\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.body(), b"Hello");
}

#[test]
fn test_chunked_binary_data() {
  let mut input = Vec::from(&b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\n"[..]);
  input.extend_from_slice(&[0x00, 0xFF, 0xAA, 0x55]);
  input.extend_from_slice(b"\r\n0\r\n\r\n");
  let result = Response::parse(&input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.body(), &[0x00, 0xFF, 0xAA, 0x55]);
}

#[test]
fn test_chunked_case_insensitive_transfer_encoding() {
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: CHUNKED\r\n\r\n5\r\nHello\r\n0\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.body(), b"Hello");
}

#[test]
fn test_chunked_gzip_then_chunked() {
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip, chunked\r\n\r\n5\r\nHello\r\n0\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.body(), b"Hello");
}

#[test]
fn test_chunked_overflow_protection() {
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nFFFFFFFFFFFFFFFF\r\n";
  let result = Response::parse(input);
  assert!(result.is_err());
}
