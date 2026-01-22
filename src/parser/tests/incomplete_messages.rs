use crate::parser::*;

#[test]
fn test_incomplete_status_line_no_crlf() {
  let input = b"HTTP/1.1 200 OK";
  let result = Response::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_incomplete_header_section() {
  let input = b"HTTP/1.1 200 OK\r\nContent-Type: text/html";
  let result = Response::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_incomplete_chunked_no_terminator() {
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nHello\r\n";
  let result = Response::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_incomplete_chunked_partial_chunk_data() {
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nA\r\n12345";
  let result = Response::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_incomplete_content_length_body() {
  let input = b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nPartialBody";
  let result = Response::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_incomplete_content_length_exact() {
  let input = b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\n\r\nPartialBody";
  let result = Response::parse(input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.body.as_bytes(), b"PartialBody");
}

#[test]
fn test_incomplete_chunked_missing_final_crlf() {
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nHello\r\n0\r\n";
  let result = Response::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_incomplete_header_no_value() {
  let input = b"HTTP/1.1 200 OK\r\nContent-Type:";
  let result = Response::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_truncated_http_version() {
  let input = b"HTTP/1";
  let result = Response::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_truncated_status_code() {
  let input = b"HTTP/1.1 20";
  let result = Response::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_incomplete_chunked_size_line() {
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5";
  let result = Response::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_response_with_only_headers_no_body_separator() {
  let input = b"HTTP/1.1 200 OK\r\nContent-Length: 0";
  let result = Response::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_zero_content_length_complete() {
  let input = b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert!(response.body.is_empty());
}

#[test]
fn test_chunked_incomplete_trailer() {
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nHello\r\n0\r\nX-Trailer: incomplete";
  let result = Response::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_header_without_final_empty_line() {
  let input = b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n";
  let result = Response::parse(input);
  assert!(result.is_err());
}
