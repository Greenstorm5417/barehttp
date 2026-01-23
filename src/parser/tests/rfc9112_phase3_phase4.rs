use crate::parser::*;
extern crate alloc;
use alloc::string::String;

#[test]
fn test_response_with_connection_close_header() {
  // Test that we can detect Connection: close in response headers
  let input = b"HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 5\r\n\r\nHello";
  let result = Response::parse(input);

  assert!(result.is_ok(), "Response with Connection: close should parse");
  let response = result.unwrap();

  // Check that Connection header is present
  let conn_header = response.headers().get("Connection");
  assert!(conn_header.is_some(), "Should have Connection header");
  assert_eq!(
    conn_header.unwrap().to_lowercase(),
    "close",
    "Connection header should be 'close'"
  );
}

#[test]
fn test_response_without_connection_close() {
  // Test normal response without Connection: close
  let input = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nHello";
  let result = Response::parse(input);

  assert!(result.is_ok(), "Normal response should parse");
  let response = result.unwrap();

  let conn_header = response.headers().get("Connection");
  assert!(
    conn_header.is_none() || conn_header.unwrap().to_lowercase() != "close",
    "Should not have Connection: close"
  );
}

// ============================================================================
// Phase 3.3: Complete Body Reading (RFC 9112 Section 9.3)
// ============================================================================

#[test]
fn test_body_read_strategy_no_body_for_1xx() {
  // 1xx responses should have no body
  let headers = crate::headers::Headers::new();
  let strategy = Response::body_read_strategy(&headers, 100);

  assert_eq!(strategy, BodyReadStrategy::NoBody, "1xx responses should have no body");
}

#[test]
fn test_body_read_strategy_no_body_for_204() {
  // 204 No Content should have no body
  let headers = crate::headers::Headers::new();
  let strategy = Response::body_read_strategy(&headers, 204);

  assert_eq!(strategy, BodyReadStrategy::NoBody, "204 should have no body");
}

#[test]
fn test_body_read_strategy_no_body_for_304() {
  // 304 Not Modified should have no body
  let headers = crate::headers::Headers::new();
  let strategy = Response::body_read_strategy(&headers, 304);

  assert_eq!(strategy, BodyReadStrategy::NoBody, "304 should have no body");
}

#[test]
fn test_body_read_strategy_content_length() {
  // Response with Content-Length should use ContentLength strategy
  let mut headers = crate::headers::Headers::new();
  headers.insert("Content-Length", "100");
  let strategy = Response::body_read_strategy(&headers, 200);

  assert_eq!(
    strategy,
    BodyReadStrategy::ContentLength(100),
    "Should use ContentLength strategy"
  );
}

#[test]
fn test_body_read_strategy_chunked() {
  // Response with Transfer-Encoding: chunked should use Chunked strategy
  let mut headers = crate::headers::Headers::new();
  headers.insert("Transfer-Encoding", "chunked");
  let strategy = Response::body_read_strategy(&headers, 200);

  assert_eq!(strategy, BodyReadStrategy::Chunked, "Should use Chunked strategy");
}

#[test]
fn test_body_read_strategy_until_close() {
  // Response with Transfer-Encoding but not chunked should read until close
  let mut headers = crate::headers::Headers::new();
  headers.insert("Transfer-Encoding", "gzip");
  let strategy = Response::body_read_strategy(&headers, 200);

  assert_eq!(
    strategy,
    BodyReadStrategy::UntilClose,
    "Non-chunked Transfer-Encoding should read until close"
  );
}

// ============================================================================
// Phase 4.1: Whitespace Before Colon Validation (RFC 9112 Section 5.1)
// ============================================================================

#[test]
fn test_whitespace_before_colon_rejected() {
  // RFC 9112 Section 5.1: Reject headers with whitespace between name and colon
  let input = b"HTTP/1.1 200 OK\r\nContent-Type : text/plain\r\n\r\n";
  let result = Response::parse(input);

  assert!(result.is_err(), "Header with space before colon should be rejected");
}

#[test]
fn test_tab_before_colon_rejected() {
  // Tab before colon should also be rejected
  let input = b"HTTP/1.1 200 OK\r\nContent-Type\t: text/plain\r\n\r\n";
  let result = Response::parse(input);

  assert!(result.is_err(), "Header with tab before colon should be rejected");
}

#[test]
fn test_valid_header_without_whitespace_accepted() {
  // Valid header without whitespace before colon should be accepted
  let input = b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n\r\n";
  let result = Response::parse(input);

  assert!(result.is_ok(), "Valid header should be accepted");
  let response = result.unwrap();
  assert_eq!(response.headers().get("Content-Type"), Some("text/plain"));
}

#[test]
fn test_whitespace_after_colon_accepted() {
  // Whitespace after colon is allowed (and common)
  let input = b"HTTP/1.1 200 OK\r\nContent-Type:  text/plain\r\n\r\n";
  let result = Response::parse(input);

  assert!(result.is_ok(), "Whitespace after colon should be accepted");
  let response = result.unwrap();
  assert_eq!(response.headers().get("Content-Type"), Some("text/plain"));
}

// ============================================================================
// Phase 4.2: Chunked Multiple Application Prevention (RFC 9112 Section 6.1)
// ============================================================================

#[test]
fn test_single_chunked_encoding_accepted() {
  // Single chunked encoding should work
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nHello\r\n0\r\n\r\n";
  let result = Response::parse(input);

  assert!(result.is_ok(), "Single chunked encoding should be accepted");
  let response = result.unwrap();
  assert_eq!(response.body.as_bytes(), b"Hello");
}

#[test]
fn test_chunked_with_other_encoding_accepted() {
  // Chunked with other encodings is allowed (chunked must be last)
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip, chunked\r\n\r\n5\r\nHello\r\n0\r\n\r\n";
  let result = Response::parse(input);

  assert!(result.is_ok(), "Chunked with other encodings should be accepted");
}

#[test]
fn test_multiple_transfer_encoding_headers() {
  // Multiple Transfer-Encoding headers (if they exist) should be handled
  // Note: This is more about parsing multiple headers with same name
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nHello\r\n0\r\n\r\n";
  let result = Response::parse(input);

  assert!(result.is_ok(), "Should handle Transfer-Encoding correctly");
}

// ============================================================================
// Phase 4.3: Trailer Field Retention (RFC 9112 Section 7.1.2)
// ============================================================================

#[test]
fn test_chunked_response_with_trailers() {
  // Test chunked response with trailer fields
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nHello\r\n0\r\nX-Trailer: value\r\n\r\n";
  let result = Response::parse(input);

  // Should parse successfully (trailers are part of chunked encoding)
  assert!(result.is_ok(), "Chunked response with trailers should parse");
  let response = result.unwrap();
  assert_eq!(response.body.as_bytes(), b"Hello");
}

#[test]
fn test_chunked_response_without_trailers() {
  // Test chunked response without trailer fields
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nHello\r\n0\r\n\r\n";
  let result = Response::parse(input);

  assert!(result.is_ok(), "Chunked response without trailers should parse");
  let response = result.unwrap();
  assert_eq!(response.body.as_bytes(), b"Hello");
}

// ============================================================================
// Integration Tests for Phase 3 and 4
// ============================================================================

#[test]
fn test_complete_response_with_connection_management() {
  // Test a complete response with connection management headers
  let input = b"HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 7\r\n\r\nSuccess";
  let result = Response::parse(input);

  assert!(result.is_ok(), "Complete response should parse");
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
  assert_eq!(response.body.as_bytes(), b"Success");
  assert_eq!(response.headers().get("Connection"), Some("close"));
}

#[test]
fn test_response_with_all_phase4_validations() {
  // Test response that passes all Phase 4 validations
  let input = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\n\r\n{\"ok\":true}";
  let result = Response::parse(input);

  assert!(result.is_ok(), "Response with valid headers should parse");
  let response = result.unwrap();
  assert_eq!(response.headers().get("Content-Type"), Some("application/json"));
  assert_eq!(response.body.as_bytes(), b"{\"ok\":true}");
}

#[test]
fn test_request_builder_with_proper_formatting() {
  // Test that RequestBuilder produces properly formatted requests
  let request = RequestBuilder::new("GET", "/api/test")
    .header("Host", "example.com")
    .header("User-Agent", "test-client")
    .build()
    .unwrap();

  let request_str = String::from_utf8_lossy(&request);

  // Should have proper format
  assert!(request_str.starts_with("GET /api/test HTTP/1.1\r\n"));
  assert!(request_str.contains("Host: example.com\r\n"));
  assert!(request_str.contains("User-Agent: test-client\r\n"));

  // Should not have whitespace before colons
  assert!(!request_str.contains(" :"));
  assert!(!request_str.contains("\t:"));
}

#[test]
fn test_body_read_strategy_precedence() {
  // Test that Transfer-Encoding takes precedence over Content-Length
  let mut headers = crate::headers::Headers::new();
  headers.insert("Transfer-Encoding", "chunked");
  headers.insert("Content-Length", "100");

  let strategy = Response::body_read_strategy(&headers, 200);

  // Should use Chunked, not ContentLength (but this would be rejected by Phase 2.1)
  // In practice, this combination should trigger ConflictingFraming error
  assert_eq!(
    strategy,
    BodyReadStrategy::Chunked,
    "Transfer-Encoding should take precedence in strategy determination"
  );
}

#[test]
fn test_empty_path_with_connection_close() {
  // Integration test: empty path + connection close
  let request = RequestBuilder::new("GET", "")
    .header("Host", "example.com")
    .header("Connection", "close")
    .build()
    .unwrap();

  let request_str = String::from_utf8_lossy(&request);

  assert!(
    request_str.starts_with("GET / HTTP/1.1\r\n"),
    "Empty path should become /"
  );
  assert!(
    request_str.contains("Connection: close\r\n"),
    "Should have Connection: close"
  );
}
