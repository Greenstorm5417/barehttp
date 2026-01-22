use crate::parser::framing::FramingDetector;
use alloc::format;
use alloc::vec::Vec;

#[test]
fn test_rfc9112_header_end_detection() {
  // RFC 9112 Section 2: Message Format
  // Header section ends with CRLF CRLF

  let minimal = b"HTTP/1.1 200 OK\r\n\r\n";
  assert_eq!(FramingDetector::find_header_end(minimal), Some(19));

  let with_headers = b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
  assert_eq!(FramingDetector::find_header_end(with_headers), Some(38));

  let incomplete = b"HTTP/1.1 200 OK\r\n";
  assert_eq!(FramingDetector::find_header_end(incomplete), None);
}

#[test]
fn test_rfc9112_header_end_with_body() {
  // Headers should be detected even when body is present
  let with_body = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nHello";
  assert_eq!(FramingDetector::find_header_end(with_body), Some(38));

  let (headers, remaining) = FramingDetector::split_headers(with_body).unwrap();
  assert_eq!(headers, b"HTTP/1.1 200 OK\r\nContent-Length: 5");
  assert_eq!(remaining, b"Hello");
}

#[test]
fn test_rfc9112_section_7_1_chunked_terminator() {
  // RFC 9112 Section 7.1: Chunked Transfer Coding
  // Last chunk is "0" followed by CRLF and optional trailer

  // Minimal terminator
  let minimal = b"0\r\n\r\n";
  assert!(FramingDetector::has_chunked_terminator(minimal));

  // Complete chunked message
  let complete = b"5\r\nhello\r\n0\r\n\r\n";
  assert!(FramingDetector::has_chunked_terminator(complete));

  // Incomplete - missing final CRLF
  let incomplete = b"5\r\nhello\r\n";
  assert!(!FramingDetector::has_chunked_terminator(incomplete));

  // Incomplete - has size but not terminator
  let incomplete2 = b"5\r\nhello\r\n3\r\n";
  assert!(!FramingDetector::has_chunked_terminator(incomplete2));
}

#[test]
fn test_rfc9112_section_6_2_content_length_parsing() {
  // RFC 9112 Section 6.2: Content-Length
  // The Content-Length field value consists of one or more digits

  // Valid values
  assert_eq!(FramingDetector::parse_content_length(b"0"), Ok(0));
  assert_eq!(FramingDetector::parse_content_length(b"123"), Ok(123));
  assert_eq!(FramingDetector::parse_content_length(b"9999"), Ok(9999));

  // With whitespace (should be trimmed)
  assert_eq!(FramingDetector::parse_content_length(b"  42  "), Ok(42));
  assert_eq!(FramingDetector::parse_content_length(b"\t100\t"), Ok(100));

  // Invalid values
  assert!(FramingDetector::parse_content_length(b"abc").is_err());
  assert!(FramingDetector::parse_content_length(b"-5").is_err());
  assert!(FramingDetector::parse_content_length(b"12.5").is_err());
  assert!(FramingDetector::parse_content_length(b"").is_err());
}

#[test]
fn test_has_complete_headers_efficiency() {
  // Test that has_complete_headers works correctly
  assert!(FramingDetector::has_complete_headers(
    b"HTTP/1.1 200 OK\r\n\r\n"
  ));
  assert!(FramingDetector::has_complete_headers(
    b"HTTP/1.1 200 OK\r\n\r\nBody"
  ));
  assert!(!FramingDetector::has_complete_headers(
    b"HTTP/1.1 200 OK\r\n"
  ));
  assert!(!FramingDetector::has_complete_headers(b"HTTP/1.1"));
}

#[test]
fn test_split_headers_various_responses() {
  // Test splitting headers from body for various response types

  // No body
  let no_body = b"HTTP/1.1 204 No Content\r\n\r\n";
  let (headers1, remaining1) = FramingDetector::split_headers(no_body).unwrap();
  assert_eq!(headers1, b"HTTP/1.1 204 No Content");
  assert_eq!(remaining1, b"");

  // With multiple headers
  let multi_header =
    b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 4\r\n\r\nTest";
  let (headers2, remaining2) = FramingDetector::split_headers(multi_header).unwrap();
  assert_eq!(
    headers2,
    b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 4"
  );
  assert_eq!(remaining2, b"Test");
}

#[test]
fn test_edge_cases() {
  // Empty input
  assert_eq!(FramingDetector::find_header_end(b""), None);
  assert!(!FramingDetector::has_complete_headers(b""));
  assert!(FramingDetector::split_headers(b"").is_err());

  // Just CRLF CRLF
  let just_separator = b"\r\n\r\n";
  assert_eq!(FramingDetector::find_header_end(just_separator), Some(4));

  // Very long headers (but valid)
  let mut long_headers = b"HTTP/1.1 200 OK\r\n".to_vec();
  for i in 0..100 {
    long_headers.extend_from_slice(format!("X-Custom-{i}: value{i}\r\n").as_bytes());
  }
  long_headers.extend_from_slice(b"\r\n");
  assert!(FramingDetector::has_complete_headers(&long_headers));
}

#[test]
fn test_chunked_minimal_terminator_in_stream() {
  // RFC 9112 Section 7.1: Chunked encoding terminator
  // Our detector looks for the minimal "0\r\n\r\n" pattern

  // The 0\r\n\r\n sequence should be found in a larger stream
  let minimal_in_larger = b"3\r\nabc\r\n5\r\nhello\r\n0\r\n\r\nExtra";
  assert!(FramingDetector::has_chunked_terminator(minimal_in_larger));

  // Multiple chunks ending with terminator
  let multi_chunk = b"5\r\nhello\r\n5\r\nworld\r\n0\r\n\r\n";
  assert!(FramingDetector::has_chunked_terminator(multi_chunk));
}

#[test]
fn test_incremental_header_detection() {
  // Simulate incremental reading
  let mut buffer = Vec::new();

  // First chunk
  buffer.extend_from_slice(b"HTTP/1.1 200 OK\r\n");
  assert!(!FramingDetector::has_complete_headers(&buffer));

  // Second chunk
  buffer.extend_from_slice(b"Content-Length: ");
  assert!(!FramingDetector::has_complete_headers(&buffer));

  // Third chunk
  buffer.extend_from_slice(b"5\r\n");
  assert!(!FramingDetector::has_complete_headers(&buffer));

  // Final chunk
  buffer.extend_from_slice(b"\r\n");
  assert!(FramingDetector::has_complete_headers(&buffer));
}

#[test]
fn test_incremental_chunked_detection() {
  // Simulate incremental reading of chunked body
  let mut buffer = Vec::new();

  buffer.extend_from_slice(b"5\r\nhello\r\n");
  assert!(!FramingDetector::has_chunked_terminator(&buffer));

  buffer.extend_from_slice(b"3\r\nabc\r\n");
  assert!(!FramingDetector::has_chunked_terminator(&buffer));

  buffer.extend_from_slice(b"0\r\n\r\n");
  assert!(FramingDetector::has_chunked_terminator(&buffer));
}

#[test]
fn test_rfc9112_robustness() {
  // RFC 9112 recommends robustness in parsing
  // Our detector should work with well-formed messages

  // Standard CRLF
  assert!(FramingDetector::has_complete_headers(
    b"HTTP/1.1 200 OK\r\n\r\n"
  ));

  // Headers with various field names and values
  let complex = b"HTTP/1.1 200 OK\r\n\
                   Date: Mon, 27 Jul 2009 12:28:53 GMT\r\n\
                   Server: Apache\r\n\
                   Last-Modified: Wed, 22 Jul 2009 19:15:56 GMT\r\n\
                   Content-Type: text/html\r\n\
                   Content-Length: 88\r\n\
                   \r\n";
  assert!(FramingDetector::has_complete_headers(complex));

  let (headers, _) = FramingDetector::split_headers(complex).unwrap();
  assert!(!headers.is_empty());
}
