use crate::parser::*;

#[test]
fn test_leading_crlf_before_status_line() {
  let input = b"\r\n\r\nHTTP/1.1 200 OK\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
}

#[test]
fn test_multiple_leading_crlf_before_status_line() {
  let input = b"\r\n\r\n\r\n\r\nHTTP/1.1 404 Not Found\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 404);
}

#[test]
fn test_leading_lf_only_before_status_line() {
  let input = b"\n\nHTTP/1.1 201 Created\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 201);
}

#[test]
fn test_bare_cr_in_status_line_rejected() {
  let input = b"HTTP/1.1 200\rOK\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_bare_cr_in_header_name_rejected() {
  let input = b"HTTP/1.1 200 OK\r\nCon\rtent-Length: 0\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_lf_without_cr_accepted() {
  let input = b"HTTP/1.1 200 OK\nContent-Length: 5\n\nHello";
  let result = Response::parse(input);
  assert!(result.is_ok());
}

#[test]
fn test_whitespace_between_start_line_and_headers_rejected() {
  let input = b"HTTP/1.1 200 OK\r\n \r\nContent-Length: 0\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_empty_response_minimal() {
  let input = b"HTTP/1.1 204 No Content\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 204);
  assert!(response.body.is_empty());
}

#[test]
fn test_http_version_case_sensitive() {
  let input = b"http/1.1 200 OK\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_status_line_with_empty_reason_phrase() {
  let input = b"HTTP/1.1 200 \r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
  assert_eq!(response.reason.len(), 0);
}

#[test]
fn test_status_line_missing_space_after_code() {
  let input = b"HTTP/1.1 200\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_http_1_0_version() {
  let input = b"HTTP/1.0 200 OK\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
}

#[test]
fn test_invalid_http_version_format() {
  let input = b"HTTP/2.0 200 OK\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_ok());
}

#[test]
fn test_status_line_with_obs_text_in_reason() {
  let input = b"HTTP/1.1 200 \xE9\xE8\xEA\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_ok());
}

#[test]
fn test_incomplete_status_line() {
  let input = b"HTTP/1.1 200";
  let result = Response::parse(input);
  assert!(result.is_err());
}
