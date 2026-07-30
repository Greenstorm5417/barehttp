use crate::error::Error;
use crate::headers::Headers;
use crate::parser::version::Version;
use crate::transport::connection::{Connection, RawResponse};
use crate::transport::tests::mock_socket::MockSocket;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;

#[test]
fn send_request_writes_to_socket() {
  let mut socket = MockSocket::with_response("");
  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);

  let request = b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n";
  let result = conn.send_request(request, &[]);

  assert!(result.is_ok());
  assert_eq!(socket.get_written(), "GET / HTTP/1.1\r\nHost: example.com\r\n\r\n");
}

#[test]
fn send_request_writes_head_and_body_without_concat() {
  let mut socket = MockSocket::with_response("");
  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);

  let head = b"POST / HTTP/1.1\r\nHost: example.com\r\nContent-Length: 11\r\n\r\n";
  let body = b"hello world";
  assert!(conn.send_request(head, body).is_ok());
  assert_eq!(
    socket.get_written(),
    "POST / HTTP/1.1\r\nHost: example.com\r\nContent-Length: 11\r\n\r\nhello world"
  );
}

#[test]
fn read_response_with_content_length() {
  let response = "HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nHello";
  let mut socket = MockSocket::with_response(response);
  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);

  let result = conn.read_raw_response(true);

  assert!(result.is_ok());
  let raw = result.unwrap();
  assert_eq!(raw.status_code, 200);
  assert_eq!(raw.reason, "OK");
  assert_eq!(&raw.body_bytes[..], b"Hello");
}

#[test]
fn read_response_no_body_expectation_ignores_content() {
  let response = "HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nHello";
  let mut socket = MockSocket::with_response(response);
  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);

  let result = conn.read_raw_response(false);

  assert!(result.is_ok());
  let raw = result.unwrap();
  assert_eq!(raw.status_code, 200);
  assert!(raw.body_bytes.is_empty(), "NoBody expectation should skip reading body");
}

#[test]
fn read_response_chunked_encoding() {
  let response = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nHello\r\n0\r\n\r\n";
  let mut socket = MockSocket::with_response(response);
  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);

  let result = conn.read_raw_response(true);

  assert!(result.is_ok());
  let raw = result.unwrap();
  assert_eq!(raw.status_code, 200);
  assert_eq!(&raw.body_bytes[..], b"Hello");
  assert!(raw.decoded_chunked_trailers.is_some());
}

#[test]
fn read_response_204_no_content() {
  let response = "HTTP/1.1 204 No Content\r\n\r\n";
  let mut socket = MockSocket::with_response(response);
  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);

  let result = conn.read_raw_response(true);

  assert!(result.is_ok());
  let raw = result.unwrap();
  assert_eq!(raw.status_code, 204);
  assert!(raw.body_bytes.is_empty());
}

#[test]
fn read_response_304_not_modified() {
  let response = "HTTP/1.1 304 Not Modified\r\n\r\n";
  let mut socket = MockSocket::with_response(response);
  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);

  let result = conn.read_raw_response(true);

  assert!(result.is_ok());
  let raw = result.unwrap();
  assert_eq!(raw.status_code, 304);
  assert!(raw.body_bytes.is_empty());
}

#[test]
fn header_size_limit_enforced() {
  let large_header = "HTTP/1.1 200 OK\r\n".to_string() + "X-Large: " + &"A".repeat(10000) + "\r\n\r\n";
  let mut socket = MockSocket::with_response(&large_header);
  let mut conn = Connection::new(&mut socket, 1024, usize::MAX);

  let result = conn.read_raw_response(true);

  assert!(result.is_err());
  assert!(matches!(result.unwrap_err(), Error::ResponseHeaderTooLarge));
}

#[test]
fn read_response_with_multiple_headers() {
  let response = "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 2\r\n\r\nOK";
  let mut socket = MockSocket::with_response(response);
  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);

  let result = conn.read_raw_response(true);

  assert!(result.is_ok());
  let raw = result.unwrap();
  assert_eq!(raw.status_code, 200);
  assert_eq!(raw.headers.get("Content-Type"), Some("text/plain"));
  assert_eq!(raw.headers.get("Content-Length"), Some("2"));
  assert_eq!(&raw.body_bytes[..], b"OK");
}

#[test]
fn read_response_empty_body_with_content_length_zero() {
  let response = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
  let mut socket = MockSocket::with_response(response);
  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);

  let result = conn.read_raw_response(true);

  assert!(result.is_ok());
  let raw = result.unwrap();
  assert_eq!(raw.status_code, 200);
  assert!(raw.body_bytes.is_empty());
}

#[test]
fn read_response_handles_body_in_header_buffer() {
  let response = "HTTP/1.1 200 OK\r\nContent-Length: 11\r\n\r\nHello World";
  let mut socket = MockSocket::with_response(response);
  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);

  let result = conn.read_raw_response(true);

  assert!(result.is_ok());
  let raw = result.unwrap();
  assert_eq!(&raw.body_bytes[..], b"Hello World");
}

#[test]
fn raw_response_can_be_cloned() {
  let mut headers = Headers::new();
  headers.insert("Content-Type", "text/plain");

  let response = RawResponse {
    status_code: 200,
    reason: String::from("OK"),
    headers,
    version: Version::HTTP_11,
    body_bytes: bytes::Bytes::from(vec![1, 2, 3]),
    decoded_chunked_trailers: None,
  };

  let cloned = response.clone();
  assert_eq!(response.status_code, 200);
  assert_eq!(cloned.status_code, 200);
  assert_eq!(cloned.reason, "OK");
  assert_eq!(&cloned.body_bytes[..], &[1, 2, 3]);
}

#[test]
fn read_response_1xx_informational_skipped() {
  let response = "HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nHello";
  let mut socket = MockSocket::with_response(response);
  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);

  let result = conn.read_raw_response(true);

  assert!(result.is_ok());
  let raw = result.unwrap();
  assert_eq!(raw.status_code, 200);
  assert_eq!(&raw.body_bytes[..], b"Hello");
}

#[test]
fn read_response_redirect_with_location() {
  let response = "HTTP/1.1 302 Found\r\nLocation: /new-url\r\n\r\n";
  let mut socket = MockSocket::with_response(response);
  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);

  let result = conn.read_raw_response(true);

  assert!(result.is_ok());
  let raw = result.unwrap();
  assert_eq!(raw.status_code, 302);
  assert_eq!(raw.headers.get("Location"), Some("/new-url"));
}

#[test]
fn read_response_large_body_content_length() {
  let body = "A".repeat(10000);
  let response = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}", body.len(), body);
  let mut socket = MockSocket::with_response(&response);
  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);

  let result = conn.read_raw_response(true);

  assert!(result.is_ok());
  let raw = result.unwrap();
  assert_eq!(raw.body_bytes.len(), 10000);
}

#[test]
fn read_response_chunked_multiple_chunks() {
  let response = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nTest\r\n5\r\nChunk\r\n0\r\n\r\n";
  let mut socket = MockSocket::with_response(response);
  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);

  let result = conn.read_raw_response(true);

  assert!(result.is_ok());
  let raw = result.unwrap();
  assert_eq!(&raw.body_bytes[..], b"TestChunk");
  assert!(raw.decoded_chunked_trailers.is_some());
}

#[test]
fn send_request_retries_short_writes() {
  let mut socket = MockSocket::with_max_write("", 7);
  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);

  let request = b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n";
  assert!(conn.send_request(request, &[]).is_ok());
  assert_eq!(socket.get_written().as_bytes(), request);
}

#[test]
fn connection_close_token_in_list_marks_non_reusable() {
  let response = "HTTP/1.1 200 OK\r\nConnection: keep-alive, Close\r\nContent-Length: 0\r\n\r\n";
  let mut socket = MockSocket::with_response(response);
  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);

  assert!(conn.read_raw_response(true).is_ok());
  assert!(!conn.is_reusable());
}

#[test]
fn connection_keep_alive_alone_stays_reusable() {
  let response = "HTTP/1.1 200 OK\r\nConnection: keep-alive\r\nContent-Length: 0\r\n\r\n";
  let mut socket = MockSocket::with_response(response);
  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);

  assert!(conn.read_raw_response(true).is_ok());
  assert!(conn.is_reusable());
}

#[test]
fn no_body_connection_close_still_marks_non_reusable() {
  let response = "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 5\r\n\r\n";
  let mut socket = MockSocket::with_response(response);
  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);

  assert!(conn.read_raw_response(false).is_ok());
  assert!(!conn.is_reusable());
}

#[test]
fn http11_without_connection_header_stays_reusable() {
  // RFC 9112 §9.3: HTTP/1.1 defaults to persistent; must not require keep-alive
  let response = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
  let mut socket = MockSocket::with_response(response);
  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);

  assert!(conn.read_raw_response(true).is_ok());
  assert!(conn.is_reusable());
}

#[test]
fn http10_without_keep_alive_not_reusable() {
  let response = "HTTP/1.0 200 OK\r\nContent-Length: 0\r\n\r\n";
  let mut socket = MockSocket::with_response(response);
  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);

  assert!(conn.read_raw_response(true).is_ok());
  assert!(!conn.is_reusable());
}

#[test]
fn http10_with_keep_alive_reusable() {
  let response = "HTTP/1.0 200 OK\r\nConnection: keep-alive\r\nContent-Length: 0\r\n\r\n";
  let mut socket = MockSocket::with_response(response);
  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);

  assert!(conn.read_raw_response(true).is_ok());
  assert!(conn.is_reusable());
}

#[test]
fn connection_close_on_second_field_line_marks_non_reusable() {
  // RFC 9110 list combining: close on any Connection field line ends persistence
  let response = "HTTP/1.1 200 OK\r\nConnection: keep-alive\r\nConnection: close\r\nContent-Length: 0\r\n\r\n";
  let mut socket = MockSocket::with_response(response);
  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);

  assert!(conn.read_raw_response(true).is_ok());
  assert!(!conn.is_reusable());
}

#[test]
fn request_connection_close_marks_non_reusable() {
  let mut socket = MockSocket::with_response("HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);
  let request = b"GET / HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n";
  assert!(conn.send_request(request, &[]).is_ok());
  assert!(!conn.is_reusable());
}

#[test]
fn header_limit_ignores_body_bytes_past_complete_headers() {
  // Headers fit under 64; body would push total past the limit if counted.
  let response = "HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n".to_string() + &"B".repeat(100);
  let mut socket = MockSocket::with_response(&response);
  let mut conn = Connection::new(&mut socket, 64, usize::MAX);

  let result = conn.read_raw_response(true);
  assert!(result.is_ok());
  assert_eq!(result.unwrap().body_bytes.len(), 100);
}

#[test]
fn chunked_read_keeps_payload_that_looks_like_terminator() {
  // First chunk payload is literally `0\r\n\r\n` (5 bytes); decoder must not stop early.
  let response = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\n0\r\n\r\n\r\n5\r\nHello\r\n0\r\n\r\n";
  let mut socket = MockSocket::with_response(response);
  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);

  let raw = conn.read_raw_response(true).unwrap();
  assert_eq!(&raw.body_bytes[..], b"0\r\n\r\nHello");
  assert!(raw.decoded_chunked_trailers.is_some());
}

#[test]
fn chunked_read_decodes_trailers_on_wire() {
  let response = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\nX-Trailer: value\r\n\r\n";
  let mut socket = MockSocket::with_response(response);
  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);

  let raw = conn.read_raw_response(true).unwrap();
  assert_eq!(&raw.body_bytes[..], b"hello");
  let trailers = raw.decoded_chunked_trailers.expect("decoded trailers");
  assert_eq!(trailers.get("X-Trailer"), Some("value"));
}

#[test]
fn read_response_headers_adopt_wire_section_zero_copy() {
  // Arena must be the frozen header section (status line + fields + blank line),
  // not a packed name/value copy. Body may share the same allocation.
  let response = "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 5\r\n\r\nHello";
  let header_section_len = response.find("\r\n\r\n").unwrap() + 4;
  let mut socket = MockSocket::with_response(response);
  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);

  let mut raw = conn.read_raw_response(true).unwrap();
  assert_eq!(raw.status_code, 200);
  assert_eq!(raw.headers.get("content-type"), Some("text/plain"));
  assert_eq!(raw.headers.get("content-length"), Some("5"));
  assert_eq!(&raw.body_bytes[..], b"Hello");
  assert_eq!(raw.headers.arena_len(), header_section_len);

  // Mutation COW must not corrupt the body Bytes that may share the allocation.
  raw.headers.set("Content-Type", "application/json");
  assert_eq!(raw.headers.get("content-type"), Some("application/json"));
  assert_eq!(&raw.body_bytes[..], b"Hello");
}

#[test]
fn read_response_obs_text_header_falls_back_to_copy() {
  let response = b"HTTP/1.1 200 OK\r\nX-Bin: \xff\xfe\r\nContent-Length: 0\r\n\r\n";
  let mut socket = MockSocket::with_read_sizes(response, &[]);
  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);

  let raw = conn.read_raw_response(true).unwrap();
  assert_eq!(raw.headers.get("x-bin"), Some("\u{fffd}\u{fffd}"));
  // Packed copy is smaller than the full wire header section.
  let header_section_len = response.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
  assert!(raw.headers.arena_len() < header_section_len);
}

#[test]
fn content_length_body_exceeds_limit() {
  let response = "HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n";
  let mut socket = MockSocket::with_response(response);
  let mut conn = Connection::new(&mut socket, 8192, 10);

  let err = conn.read_raw_response(true).unwrap_err();
  assert!(matches!(err, Error::BodyExceedsLimit(10)));
}
