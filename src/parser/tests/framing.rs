use crate::parser::framing::{has_chunked_terminator, has_complete_headers};
use alloc::format;
use alloc::vec::Vec;

#[test]
fn test_rfc9112_section_7_1_chunked_terminator() {
  // RFC 9112 Section 7.1: Chunked Transfer Coding
  // Last chunk is "0" followed by CRLF and optional trailer

  // Minimal terminator
  let minimal = b"0\r\n\r\n";
  assert!(has_chunked_terminator(minimal));

  // Complete chunked message
  let complete = b"5\r\nhello\r\n0\r\n\r\n";
  assert!(has_chunked_terminator(complete));

  // Incomplete - missing final CRLF
  let incomplete = b"5\r\nhello\r\n";
  assert!(!has_chunked_terminator(incomplete));

  // Incomplete - has size but not terminator
  let incomplete2 = b"5\r\nhello\r\n3\r\n";
  assert!(!has_chunked_terminator(incomplete2));
}

#[test]
fn test_has_complete_headers() {
  assert!(has_complete_headers(b"HTTP/1.1 200 OK\r\n\r\n"));
  assert!(has_complete_headers(b"HTTP/1.1 200 OK\r\n\r\nBody"));
  assert!(!has_complete_headers(b"HTTP/1.1 200 OK\r\n"));
  assert!(!has_complete_headers(b"HTTP/1.1"));
  assert!(!has_complete_headers(b""));
}

#[test]
fn test_chunked_minimal_terminator_in_stream() {
  // End-anchored: terminator must finish the buffer (not appear mid-stream with trailing junk)
  let with_extra = b"3\r\nabc\r\n5\r\nhello\r\n0\r\n\r\nExtra";
  assert!(!has_chunked_terminator(with_extra));

  let multi_chunk = b"5\r\nhello\r\n5\r\nworld\r\n0\r\n\r\n";
  assert!(has_chunked_terminator(multi_chunk));

  // Mid-body "0\r\n\r\n" inside chunk data must not trip the heuristic
  let mid_body = b"A\r\nhello0\r\n\r\nx\r\n";
  assert!(!has_chunked_terminator(mid_body));

  // Trailers after last chunk
  let with_trailer = b"5\r\nhello\r\n0\r\nX-Trailer: v\r\n\r\n";
  assert!(has_chunked_terminator(with_trailer));
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
fn test_incremental_chunked_detection() {
  let mut buffer = Vec::new();

  buffer.extend_from_slice(b"5\r\nhello\r\n");
  assert!(!has_chunked_terminator(&buffer));

  buffer.extend_from_slice(b"3\r\nabc\r\n");
  assert!(!has_chunked_terminator(&buffer));

  buffer.extend_from_slice(b"0\r\n\r\n");
  assert!(has_chunked_terminator(&buffer));
}

#[test]
fn test_chunked_terminator_lf_only() {
  assert!(has_chunked_terminator(b"0\n\n"));
  assert!(has_chunked_terminator(b"5\nhello\n0\n\n"));
  assert!(has_chunked_terminator(b"5\nhello\n0\nX-Trailer: v\n\n"));
  assert!(!has_chunked_terminator(b"5\nhello\n"));
  assert!(!has_chunked_terminator(b"A\nhello0\n\nx\n"));
  assert!(!has_chunked_terminator(b"5\nhello\n0\n\nExtra"));
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
