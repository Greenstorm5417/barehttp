use crate::parser::http::StatusLine;

#[test]
fn test_status_200_with_reason() {
  let input = b"HTTP/1.1 200 OK\r\n";
  let result = StatusLine::parse(input);
  assert!(result.is_ok());
  let (line, _) = result.unwrap();
  assert_eq!(line.status.code(), 200);
  assert_eq!(line.reason, b"OK");
}

#[test]
fn test_status_404_with_reason() {
  let input = b"HTTP/1.1 404 Not Found\r\n";
  let result = StatusLine::parse(input);
  assert!(result.is_ok());
  let (line, _) = result.unwrap();
  assert_eq!(line.status.code(), 404);
  assert_eq!(line.reason, b"Not Found");
}

#[test]
fn test_status_with_no_reason_phrase() {
  let input = b"HTTP/1.1 204 \r\n";
  let result = StatusLine::parse(input);
  assert!(result.is_ok());
  let (line, _) = result.unwrap();
  assert_eq!(line.status.code(), 204);
  assert_eq!(line.reason.len(), 0);
}

#[test]
fn test_status_line_reason_with_multiple_spaces() {
  let input = b"HTTP/1.1 500 Internal  Server  Error\r\n";
  let result = StatusLine::parse(input);
  assert!(result.is_ok());
  let (line, _) = result.unwrap();
  assert_eq!(line.reason, b"Internal  Server  Error");
}

#[test]
fn test_informational_status_100() {
  let input = b"HTTP/1.1 100 Continue\r\n";
  let result = StatusLine::parse(input);
  assert!(result.is_ok());
  let (line, _) = result.unwrap();
  assert_eq!(line.status.code(), 100);
}

#[test]
fn test_redirection_status_301() {
  let input = b"HTTP/1.1 301 Moved Permanently\r\n";
  let result = StatusLine::parse(input);
  assert!(result.is_ok());
  let (line, _) = result.unwrap();
  assert_eq!(line.status.code(), 301);
}

#[test]
fn test_client_error_status_400() {
  let input = b"HTTP/1.1 400 Bad Request\r\n";
  let result = StatusLine::parse(input);
  assert!(result.is_ok());
  let (line, _) = result.unwrap();
  assert_eq!(line.status.code(), 400);
}

#[test]
fn test_server_error_status_503() {
  let input = b"HTTP/1.1 503 Service Unavailable\r\n";
  let result = StatusLine::parse(input);
  assert!(result.is_ok());
  let (line, _) = result.unwrap();
  assert_eq!(line.status.code(), 503);
}

#[test]
fn test_custom_status_code_299() {
  let input = b"HTTP/1.1 299 Custom Status\r\n";
  let result = StatusLine::parse(input);
  assert!(result.is_ok());
  let (line, _) = result.unwrap();
  assert_eq!(line.status.code(), 299);
}

#[test]
fn test_status_code_invalid_two_digits() {
  let input = b"HTTP/1.1 20 OK\r\n";
  let result = StatusLine::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_status_code_invalid_four_digits() {
  let input = b"HTTP/1.1 2000 OK\r\n";
  let result = StatusLine::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_status_code_non_numeric() {
  let input = b"HTTP/1.1 ABC OK\r\n";
  let result = StatusLine::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_missing_space_after_version() {
  let input = b"HTTP/1.1200 OK\r\n";
  let result = StatusLine::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_missing_space_after_status_code() {
  let input = b"HTTP/1.1 200OK\r\n";
  let result = StatusLine::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_reason_phrase_with_tab() {
  let input = b"HTTP/1.1 200 OK\tStatus\r\n";
  let result = StatusLine::parse(input);
  assert!(result.is_ok());
  let (line, _) = result.unwrap();
  assert_eq!(line.reason, b"OK\tStatus");
}

#[test]
fn test_reason_phrase_with_extended_ascii() {
  let input = b"HTTP/1.1 200 \xC9tat OK\r\n";
  let result = StatusLine::parse(input);
  assert!(result.is_ok());
}

#[test]
fn test_http_1_0_status_line() {
  let input = b"HTTP/1.0 200 OK\r\n";
  let result = StatusLine::parse(input);
  assert!(result.is_ok());
}

#[test]
fn test_status_line_with_lf_only() {
  let input = b"HTTP/1.1 200 OK\n";
  let result = StatusLine::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_status_line_incomplete_missing_crlf() {
  let input = b"HTTP/1.1 200 OK";
  let result = StatusLine::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_websocket_upgrade_101() {
  let input = b"HTTP/1.1 101 Switching Protocols\r\n";
  let result = StatusLine::parse(input);
  assert!(result.is_ok());
  let (line, _) = result.unwrap();
  assert_eq!(line.status.code(), 101);
}
