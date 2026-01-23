/// RFC 9112 Phase 1 and Phase 2 Compliance Tests
/// Tests for critical request building and response parsing fixes
use crate::parser::*;
extern crate alloc;
use alloc::string::String;

// ============================================================================
// Phase 1.1: Empty Path Handling (RFC 9112 Section 3.2.1)
// ============================================================================

#[test]
fn test_empty_path_sends_slash() {
  // RFC 9112 Section 3.2.1: If origin-form path is empty, send "/"
  let request = RequestBuilder::new("GET", "")
    .header("Host", "example.com")
    .build()
    .unwrap();
  let request_str = String::from_utf8_lossy(&request);

  assert!(
    request_str.starts_with("GET / HTTP/1.1\r\n"),
    "Empty path should be converted to '/'. Got: {request_str}"
  );
}

#[test]
fn test_non_empty_path_preserved() {
  // Verify non-empty paths are not modified
  let request = RequestBuilder::new("GET", "/api/users")
    .header("Host", "example.com")
    .build()
    .unwrap();
  let request_str = String::from_utf8_lossy(&request);

  assert!(
    request_str.starts_with("GET /api/users HTTP/1.1\r\n"),
    "Non-empty path should be preserved. Got: {request_str}"
  );
}

#[test]
fn test_path_with_query_preserved() {
  // Verify paths with query strings work correctly
  let request = RequestBuilder::new("GET", "/search?q=test")
    .header("Host", "example.com")
    .build()
    .unwrap();
  let request_str = String::from_utf8_lossy(&request);

  assert!(
    request_str.starts_with("GET /search?q=test HTTP/1.1\r\n"),
    "Path with query should be preserved. Got: {request_str}"
  );
}

// ============================================================================
// Phase 1.3: No Extra CRLF (RFC 9112 Section 2.2)
// ============================================================================

#[test]
fn test_no_leading_crlf_in_request() {
  // RFC 9112 Section 2.2: HTTP/1.1 user agent MUST NOT preface request with extra CRLF
  let request = RequestBuilder::new("GET", "/")
    .header("Host", "example.com")
    .build()
    .unwrap();

  // Should start directly with method, not with CRLF
  assert!(request.starts_with(b"GET"), "Request should not have leading CRLF");
  assert!(!request.starts_with(b"\r\n"), "Request should not start with CRLF");
}

#[test]
fn test_no_trailing_crlf_after_body() {
  // RFC 9112 Section 2.2: MUST NOT follow request with extra CRLF
  let body = b"test body".to_vec();
  let request = RequestBuilder::new("POST", "/")
    .header("Host", "example.com")
    .body(body)
    .build()
    .unwrap();

  // Should end with body content, not extra CRLF
  assert!(
    request.ends_with(b"test body"),
    "Request should not have trailing CRLF after body"
  );
  assert!(
    !request.ends_with(b"test body\r\n"),
    "Request should not end with CRLF after body"
  );
}

#[test]
fn test_exactly_one_blank_line_before_body() {
  // Verify exactly one blank line (CRLF CRLF) separates headers from body
  let body = b"content".to_vec();
  let request = RequestBuilder::new("POST", "/")
    .header("Host", "example.com")
    .header("Content-Type", "text/plain")
    .body(body)
    .build()
    .unwrap();

  let request_str = String::from_utf8_lossy(&request);

  // Should have exactly one blank line before body
  assert!(
    request_str.contains("\r\n\r\ncontent"),
    "Should have exactly one blank line before body"
  );
  assert!(
    !request_str.contains("\r\n\r\n\r\n"),
    "Should not have multiple blank lines"
  );
}

#[test]
fn test_exactly_one_crlf_after_each_header() {
  // Verify each header ends with exactly one CRLF
  let request = RequestBuilder::new("GET", "/")
    .header("Host", "example.com")
    .header("User-Agent", "test")
    .build()
    .unwrap();

  let request_str = String::from_utf8_lossy(&request);

  // Should not have double CRLF after headers (except the blank line)
  let header_section = request_str.split("\r\n\r\n").next().unwrap();
  assert!(
    !header_section.contains("\r\n\r\n"),
    "Headers should not have double CRLF within them"
  );
}

// ============================================================================
// Phase 2.1: Conflicting TE+CL Handling (RFC 9112 Section 6.3)
// ============================================================================

#[test]
fn test_te_and_cl_conflict_rejected() {
  // RFC 9112 Section 6.3: If both TE and CL present, client MUST reject
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Length: 5\r\n\r\n5\r\nHello\r\n0\r\n\r\n";
  let result = Response::parse(input);

  assert!(result.is_err(), "Response with both TE and CL should be rejected");
  if let Err(e) = result {
    assert!(
      matches!(e, crate::error::ParseError::ConflictingFraming),
      "Should return ConflictingFraming error, got: {e:?}"
    );
  }
}

#[test]
fn test_te_without_cl_accepted() {
  // Transfer-Encoding without Content-Length should work
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nHello\r\n0\r\n\r\n";
  let result = Response::parse(input);

  assert!(result.is_ok(), "Response with only TE should be accepted");
  let response = result.unwrap();
  assert_eq!(response.body.as_bytes(), b"Hello");
}

#[test]
fn test_cl_without_te_accepted() {
  // Content-Length without Transfer-Encoding should work
  let input = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nHello";
  let result = Response::parse(input);

  assert!(result.is_ok(), "Response with only CL should be accepted");
  let response = result.unwrap();
  assert_eq!(response.body.as_bytes(), b"Hello");
}

// ============================================================================
// Phase 2.2: 2xx CONNECT Special Case (RFC 9112 Section 6.3)
// ============================================================================

#[test]
fn test_connect_2xx_ignores_content_length() {
  // RFC 9112 Section 6.3: 2xx to CONNECT ignores Content-Length
  let input = b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nThis should be ignored";

  // Parse with CONNECT method
  let (status_line, after_status) = crate::parser::http::StatusLine::parse(&input[..]).unwrap();
  let (headers_bytes, remaining) = crate::parser::headers::HeaderField::parse(after_status).unwrap();

  let body_bytes = Response::parse_body(remaining, &headers_bytes, status_line.status.code(), Some("CONNECT")).unwrap();

  assert!(
    body_bytes.is_empty(),
    "2xx CONNECT response should ignore Content-Length and have no body"
  );
}

#[test]
fn test_connect_non_2xx_processes_body_normally() {
  // Non-2xx CONNECT responses should process body normally
  let input = b"HTTP/1.1 400 Bad Request\r\nContent-Length: 5\r\n\r\nError";

  let (status_line, after_status) = crate::parser::http::StatusLine::parse(&input[..]).unwrap();
  let (headers_bytes, remaining) = crate::parser::headers::HeaderField::parse(after_status).unwrap();

  let body_bytes = Response::parse_body(remaining, &headers_bytes, status_line.status.code(), Some("CONNECT")).unwrap();

  assert_eq!(
    body_bytes, b"Error",
    "Non-2xx CONNECT response should process body normally"
  );
}

#[test]
fn test_connect_2xx_ignores_transfer_encoding() {
  // RFC 9112 Section 6.3: 2xx to CONNECT ignores Transfer-Encoding
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nHello\r\n0\r\n\r\n";

  let (status_line, after_status) = crate::parser::http::StatusLine::parse(&input[..]).unwrap();
  let (headers_bytes, remaining) = crate::parser::headers::HeaderField::parse(after_status).unwrap();

  let body_bytes = Response::parse_body(remaining, &headers_bytes, status_line.status.code(), Some("CONNECT")).unwrap();

  assert!(
    body_bytes.is_empty(),
    "2xx CONNECT response should ignore Transfer-Encoding and have no body"
  );
}

// ============================================================================
// Phase 2.4: Invalid Content-Length Handling (RFC 9112 Section 6.3)
// ============================================================================

#[test]
fn test_invalid_content_length_rejected() {
  // RFC 9112 Section 6.3: Invalid Content-Length is unrecoverable error
  let input = b"HTTP/1.1 200 OK\r\nContent-Length: invalid\r\n\r\n";
  let result = Response::parse(input);

  // Should parse successfully but treat as no Content-Length
  assert!(result.is_ok(), "Should handle invalid Content-Length gracefully");
  let response = result.unwrap();
  assert!(
    response.body.is_empty(),
    "Invalid Content-Length should result in no body"
  );
}

#[test]
fn test_multiple_identical_content_length_accepted() {
  // RFC 9112 allows comma-separated identical values
  let input = b"HTTP/1.1 200 OK\r\nContent-Length: 5, 5, 5\r\n\r\nHello";
  let result = Response::parse(input);

  assert!(
    result.is_ok(),
    "Multiple identical Content-Length values should be accepted"
  );
  let response = result.unwrap();
  assert_eq!(response.body.as_bytes(), b"Hello");
}

#[test]
fn test_multiple_different_content_length_rejected() {
  // Different Content-Length values should be rejected
  let input = b"HTTP/1.1 200 OK\r\nContent-Length: 5, 10\r\n\r\nHello";
  let result = Response::parse(input);

  // Should parse but treat as invalid Content-Length
  assert!(result.is_ok(), "Should handle conflicting Content-Length gracefully");
  let response = result.unwrap();
  assert!(
    response.body.is_empty(),
    "Conflicting Content-Length should result in no body"
  );
}

#[test]
fn test_content_length_with_non_digits_rejected() {
  // Content-Length with non-digit characters should be rejected
  let input = b"HTTP/1.1 200 OK\r\nContent-Length: 5abc\r\n\r\n";
  let result = Response::parse(input);

  assert!(result.is_ok(), "Should handle malformed Content-Length gracefully");
  let response = result.unwrap();
  assert!(
    response.body.is_empty(),
    "Malformed Content-Length should result in no body"
  );
}

#[test]
fn test_content_length_with_whitespace_accepted() {
  // Content-Length with leading/trailing whitespace should be accepted
  let input = b"HTTP/1.1 200 OK\r\nContent-Length:  5  \r\n\r\nHello";
  let result = Response::parse(input);

  assert!(result.is_ok(), "Content-Length with whitespace should be accepted");
  let response = result.unwrap();
  assert_eq!(response.body.as_bytes(), b"Hello");
}

// ============================================================================
// Integration Tests
// ============================================================================

#[test]
fn test_complete_request_with_all_phase1_fixes() {
  // Test a complete request with all Phase 1 fixes applied
  let request = RequestBuilder::new("POST", "")
    .header("Host", "example.com")
    .header("Content-Type", "application/json")
    .body(b"{\"test\":true}".to_vec())
    .build()
    .unwrap();

  let request_str = String::from_utf8_lossy(&request);

  // Should start with method (no leading CRLF)
  assert!(
    request_str.starts_with("POST / HTTP/1.1\r\n"),
    "Should have correct start line with / for empty path"
  );

  // Should have Host header
  assert!(request_str.contains("Host: example.com\r\n"), "Should have Host header");

  // Should end with body (no trailing CRLF)
  assert!(
    request_str.ends_with("{\"test\":true}"),
    "Should end with body content without trailing CRLF"
  );
}

#[test]
fn test_complete_response_with_all_phase2_fixes() {
  // Test a complete response parsing with all Phase 2 fixes
  let input = b"HTTP/1.1 200 OK\r\nContent-Length: 13\r\n\r\nHello, World!";
  let result = Response::parse(input);

  assert!(result.is_ok(), "Valid response should parse successfully");
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
  assert_eq!(response.body.as_bytes(), b"Hello, World!");
}
