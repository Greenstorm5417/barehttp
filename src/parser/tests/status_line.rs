//! Status-line coverage via [`Response::parse`] (`status_line` module inlined into version).
use crate::parser::Response;

#[test]
fn status_line_ok_cases() {
  let ok = Response::parse(b"HTTP/1.1 200 OK\r\n\r\n").unwrap();
  assert_eq!(ok.status_code(), 200);
  assert_eq!(ok.reason(), "OK");

  let empty_reason = Response::parse(b"HTTP/1.1 204 \r\n\r\n").unwrap();
  assert_eq!(empty_reason.status_code(), 204);
  assert!(empty_reason.reason().is_empty());

  let spaced = Response::parse(b"HTTP/1.1 500 Internal  Server  Error\r\n\r\n").unwrap();
  assert_eq!(spaced.reason(), "Internal  Server  Error");

  assert_eq!(
    Response::parse(b"HTTP/1.0 200 OK\r\n\r\n")
      .unwrap()
      .status_code(),
    200
  );
  assert_eq!(
    Response::parse(b"HTTP/1.1 101 Switching Protocols\r\n\r\n")
      .unwrap()
      .status_code(),
    101
  );
}

#[test]
fn status_line_rejects_bad_codes_and_spacing() {
  assert!(Response::parse(b"HTTP/1.1 20 OK\r\n\r\n").is_err());
  assert!(Response::parse(b"HTTP/1.1 2000 OK\r\n\r\n").is_err());
  assert!(Response::parse(b"HTTP/1.1 ABC OK\r\n\r\n").is_err());
  assert!(Response::parse(b"HTTP/1.1200 OK\r\n\r\n").is_err());
  assert!(Response::parse(b"HTTP/1.1 200OK\r\n\r\n").is_err());
  assert!(Response::parse(b"HTTP/1.1 200 OK").is_err());
}
