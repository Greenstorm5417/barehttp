use crate::parser::BodyReadStrategy;
use crate::parser::response_reader::ResponseReader;
use alloc::format;

#[test]
fn test_rfc9112_content_length_response() {
  // RFC 9112 Section 6.2: Content-Length
  // Body length is determined by Content-Length header

  let mut reader = ResponseReader::new();

  let response = b"HTTP/1.1 200 OK\r\nContent-Length: 13\r\n\r\nHello, World!";
  reader.feed(response).expect("feed failed");

  assert!(reader.has_complete_headers());

  let (status, reason, headers, strategy) = reader.parse_headers().expect("parse failed");
  assert_eq!(status, 200);
  assert_eq!(reason, "OK");
  assert_eq!(strategy, BodyReadStrategy::ContentLength(13));
  assert!(reader.is_body_complete());

  let body = reader.finish(&headers, status).expect("finish failed");
  assert_eq!(body.as_bytes(), b"Hello, World!");
}

#[test]
fn test_rfc9112_section_6_3_chunked_response() {
  // RFC 9112 Section 6.3: Transfer-Encoding overrides Content-Length
  // RFC 9112 Section 7.1: Chunked Transfer Coding

  let mut reader = ResponseReader::new();

  reader
    .feed(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n")
    .expect("feed failed");
  assert!(reader.has_complete_headers());

  let (status, _reason, headers, strategy) = reader.parse_headers().expect("parse failed");
  assert_eq!(strategy, BodyReadStrategy::Chunked);

  // Feed chunked body: "hello"
  reader.feed(b"5\r\nhello\r\n").expect("feed failed");
  assert!(!reader.is_body_complete());

  // Feed terminating chunk
  reader.feed(b"0\r\n\r\n").expect("feed failed");
  assert!(reader.is_body_complete());

  let body = reader.finish(&headers, status).expect("finish failed");
  assert_eq!(body.as_bytes(), b"hello");
}

#[test]
fn test_rfc9112_section_6_no_body_responses() {
  // RFC 9112 Section 6.3: Certain responses never have a body
  // 1xx (Informational), 204 (No Content), 304 (Not Modified)

  // Test 204 No Content
  let mut reader = ResponseReader::new();
  reader
    .feed(b"HTTP/1.1 204 No Content\r\n\r\n")
    .expect("feed failed");

  let (status, _reason, headers, strategy) = reader.parse_headers().expect("parse failed");
  assert_eq!(status, 204);
  assert_eq!(strategy, BodyReadStrategy::NoBody);
  assert!(reader.is_body_complete());

  let body = reader.finish(&headers, status).expect("finish failed");
  assert_eq!(body.as_bytes(), b"");

  // Test 304 Not Modified
  let mut reader2 = ResponseReader::new();
  reader2
    .feed(b"HTTP/1.1 304 Not Modified\r\n\r\n")
    .expect("feed failed");

  let (status2, _reason2, headers2, strategy2) = reader2.parse_headers().expect("parse failed");
  assert_eq!(status2, 304);
  assert_eq!(strategy2, BodyReadStrategy::NoBody);

  let body2 = reader2.finish(&headers2, status2).expect("finish failed");
  assert_eq!(body2.as_bytes(), b"");
}

#[test]
fn test_rfc9112_incremental_header_reading() {
  // RFC 9112 Section 2: Message Parsing
  // Headers arrive incrementally

  let mut reader = ResponseReader::new();

  reader.feed(b"HTTP/1.1 200 OK\r\n").expect("feed failed");
  assert!(!reader.has_complete_headers());

  reader
    .feed(b"Content-Type: text/plain\r\n")
    .expect("feed failed");
  assert!(!reader.has_complete_headers());

  reader.feed(b"Content-Length: 4\r\n").expect("feed failed");
  assert!(!reader.has_complete_headers());

  reader.feed(b"\r\n").expect("feed failed");
  assert!(reader.has_complete_headers());

  let (_status, _reason, _headers, strategy) = reader.parse_headers().expect("parse failed");
  assert_eq!(strategy, BodyReadStrategy::ContentLength(4));
}

#[test]
fn test_rfc9112_header_size_limit() {
  // RFC 9112 Section 2.1: Implementations ought to be careful about header size

  let mut reader = ResponseReader::with_max_header_size(50);

  // This should fit
  let small_headers = b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
  assert!(reader.feed(small_headers).is_ok());

  // Create a reader with tiny limit
  let mut small_reader = ResponseReader::with_max_header_size(20);
  let large_headers = b"HTTP/1.1 200 OK\r\nX-Large-Header: value\r\n\r\n";

  // Should exceed limit
  let result = small_reader.feed(large_headers);
  assert!(result.is_err());
}

#[test]
fn test_bytes_needed_tracking() {
  // Test that bytes_needed correctly tracks remaining bytes

  let mut reader = ResponseReader::new();

  reader
    .feed(b"HTTP/1.1 200 OK\r\nContent-Length: 20\r\n\r\n")
    .expect("feed failed");
  let (_status, _reason, _headers, strategy) = reader.parse_headers().expect("parse failed");
  assert_eq!(strategy, BodyReadStrategy::ContentLength(20));

  // No body yet
  assert_eq!(reader.bytes_needed(), Some(20));

  // Add 10 bytes
  reader.feed(b"1234567890").expect("feed failed");
  assert_eq!(reader.bytes_needed(), Some(10));
  assert!(!reader.is_body_complete());

  // Add remaining 10 bytes
  reader.feed(b"abcdefghij").expect("feed failed");
  assert_eq!(reader.bytes_needed(), Some(0));
  assert!(reader.is_body_complete());
}

#[test]
fn test_multiple_chunk_response() {
  // Test reading a response with multiple chunks

  let mut reader = ResponseReader::new();

  reader
    .feed(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n")
    .expect("feed failed");
  let (status, _reason, headers, strategy) = reader.parse_headers().expect("parse failed");
  assert_eq!(strategy, BodyReadStrategy::Chunked);

  // First chunk: "Hello"
  reader.feed(b"5\r\nHello\r\n").expect("feed failed");
  assert!(!reader.is_body_complete());

  // Second chunk: ", "
  reader.feed(b"2\r\n, \r\n").expect("feed failed");
  assert!(!reader.is_body_complete());

  // Third chunk: "World"
  reader.feed(b"5\r\nWorld\r\n").expect("feed failed");
  assert!(!reader.is_body_complete());

  // Terminating chunk
  reader.feed(b"0\r\n\r\n").expect("feed failed");
  assert!(reader.is_body_complete());

  let body = reader.finish(&headers, status).expect("finish failed");
  assert_eq!(body.as_bytes(), b"Hello, World");
}

#[test]
fn test_response_with_no_content_length() {
  // RFC 9112: Without Content-Length or Transfer-Encoding,
  // responses should have no body (for most cases)

  let mut reader = ResponseReader::new();

  reader
    .feed(b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n\r\n")
    .expect("feed failed");
  let (status, _reason, headers, strategy) = reader.parse_headers().expect("parse failed");

  // Without Content-Length or Transfer-Encoding, assume no body
  assert_eq!(strategy, BodyReadStrategy::NoBody);

  let body = reader.finish(&headers, status).expect("finish failed");
  assert_eq!(body.as_bytes(), b"");
}

#[test]
fn test_1xx_informational_responses() {
  // RFC 9112 Section 6.3: 1xx responses never have a body

  for code in 100..200 {
    let mut reader = ResponseReader::new();
    let response = format!("HTTP/1.1 {code} Continue\r\n\r\n");
    reader.feed(response.as_bytes()).expect("feed failed");

    let (status, _reason, headers, strategy) = reader.parse_headers().expect("parse failed");
    assert_eq!(status, code);
    assert_eq!(strategy, BodyReadStrategy::NoBody);

    let body = reader.finish(&headers, status).expect("finish failed");
    assert_eq!(body.as_bytes(), b"");
  }
}

#[test]
fn test_chunked_with_extensions() {
  // RFC 9112 Section 7.1.1: Chunk extensions are allowed but we ignore them

  let mut reader = ResponseReader::new();

  reader
    .feed(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n")
    .expect("feed failed");
  let (status, _reason, headers, _strategy) = reader.parse_headers().expect("parse failed");

  // Chunk with extension (should still work)
  reader
    .feed(b"5;name=value\r\nhello\r\n")
    .expect("feed failed");
  reader.feed(b"0\r\n\r\n").expect("feed failed");

  assert!(reader.is_body_complete());

  let body = reader.finish(&headers, status).expect("finish failed");
  // Body should still be decoded correctly despite extensions
  assert_eq!(body.as_bytes(), b"hello");
}
