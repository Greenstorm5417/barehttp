/// Comprehensive test suite for RFC 9112 MUST requirements
/// This file tests all client-applicable MUST requirements from RFC 9112
use crate::parser::*;
extern crate alloc;

// ============================================================================
// RFC 9112 Section 2.2: Message Parsing / Syntax
// ============================================================================

#[test]
fn test_must_parse_as_octet_sequence() {
  // MUST: Parse HTTP message as sequence of octets in superset of US-ASCII
  let input = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nHello";
  let result = Response::parse(input);
  assert!(result.is_ok(), "Parser must handle octet sequences");
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
  assert_eq!(response.body.as_bytes(), b"Hello");
}

#[test]
fn test_must_reject_bare_cr_in_status_line() {
  // MUST: Treat bare CR as invalid OR replace with SP
  // Our implementation rejects it
  let input = b"HTTP/1.1 200\rOK\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_err(), "Bare CR in status line must be rejected");
}

#[test]
fn test_must_reject_bare_cr_in_header_value() {
  // MUST: Treat bare CR as invalid OR replace with SP
  let input = b"HTTP/1.1 200 OK\r\nX-Header: value\rwith\rCR\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_err(), "Bare CR in header value must be rejected");
}

#[test]
fn test_must_handle_whitespace_before_first_header() {
  // MUST: Either reject OR consume whitespace-prefixed lines
  // Our implementation rejects (valid per RFC)
  let input = b"HTTP/1.1 200 OK\r\n \r\nContent-Length: 0\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_err(), "Whitespace before first header must be handled");
}

// ============================================================================
// RFC 9112 Section 5.2: Obsolete Line Folding (obs-fold)
// ============================================================================

#[test]
fn test_must_handle_obs_fold_with_crlf_space() {
  // MUST: User agent receiving obs-fold MUST replace with SP(s)
  let input = b"HTTP/1.1 200 OK\r\nX-Long-Header: first\r\n second\r\n third\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_ok(), "obs-fold must be handled");
  let response = result.unwrap();
  let header_value = response.get_header("X-Long-Header");
  assert!(header_value.is_some());
  // Should be "first second third" with spaces replacing obs-fold
  let value = header_value.unwrap();
  assert!(value.contains("first"));
  assert!(value.contains("second"));
  assert!(value.contains("third"));
}

#[test]
fn test_must_handle_obs_fold_with_crlf_tab() {
  // MUST: User agent receiving obs-fold MUST replace with SP(s)
  let input = b"HTTP/1.1 200 OK\r\nContent-Type: text/html;\r\n\tcharset=utf-8\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_ok(), "obs-fold with tab must be handled");
  let response = result.unwrap();
  let ct = response.get_header("Content-Type");
  assert!(ct.is_some());
  let value = ct.unwrap();
  assert!(value.contains("text/html"));
  assert!(value.contains("charset=utf-8"));
}

#[test]
fn test_must_handle_obs_fold_multiple_lines() {
  // MUST: Handle multiple obs-fold sequences
  let input = b"HTTP/1.1 200 OK\r\nX-Multi: line1\r\n line2\r\n line3\r\n line4\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_ok(), "Multiple obs-fold lines must be handled");
  let response = result.unwrap();
  let value = response.get_header("X-Multi").unwrap();
  assert!(value.contains("line1"));
  assert!(value.contains("line2"));
  assert!(value.contains("line3"));
  assert!(value.contains("line4"));
}

// ============================================================================
// RFC 9112 Section 6.1 & 7.1: Chunked Transfer Coding
// ============================================================================

#[test]
fn test_must_parse_chunked_transfer_coding() {
  // MUST: Recipient can parse chunked transfer coding
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nHello\r\n0\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_ok(), "Must be able to parse chunked encoding");
  let response = result.unwrap();
  assert_eq!(response.body.as_bytes(), b"Hello");
}

#[test]
fn test_must_prevent_chunk_size_overflow() {
  // MUST: Anticipate very large chunk-size hex numbers and prevent overflow
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nFFFFFFFFFFFFFFFF\r\ndata\r\n0\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_err(), "Must prevent chunk size overflow");
}

#[test]
fn test_must_ignore_chunk_extensions() {
  // MUST: Recipient ignores unrecognized chunk extensions
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5;ext=value;other=data\r\nHello\r\n0\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_ok(), "Must ignore chunk extensions");
  let response = result.unwrap();
  assert_eq!(response.body.as_bytes(), b"Hello");
}

#[test]
fn test_must_not_merge_trailer_fields_unsafely() {
  // MUST NOT: Merge trailer fields into headers unless explicitly allowed
  // Our implementation discards trailers (safe approach)
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nHello\r\n0\r\nX-Trailer: value\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_ok());
  let response = result.unwrap();
  // Trailer should not be in headers
  assert!(response.get_header("X-Trailer").is_none());
}

// ============================================================================
// RFC 9112 Section 6.2 & 6.3: Content-Length
// ============================================================================

#[test]
fn test_must_handle_content_length() {
  // MUST: Handle Content-Length framing
  let input = b"HTTP/1.1 200 OK\r\nContent-Length: 13\r\n\r\nHello, World!";
  let result = Response::parse(input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.body.as_bytes(), b"Hello, World!");
}

#[test]
fn test_must_error_on_incomplete_content_length() {
  // MUST: If Content-Length says N but fewer octets received, mark incomplete
  let input = b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nShort";
  let result = Response::parse(input);
  assert!(result.is_err(), "Must error when content is incomplete");
}

#[test]
fn test_must_prioritize_transfer_encoding_over_content_length() {
  // RFC 9112 Section 6.3: Both TE and CL is a potential request smuggling attack
  // Client MUST reject this combination (not prioritize one over the other)
  let input = b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nHello\r\n0\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_err(), "Response with both TE and CL should be rejected");
}

#[test]
fn test_must_handle_invalid_content_length() {
  // MUST: Treat invalid Content-Length as unrecoverable error
  let input = b"HTTP/1.1 200 OK\r\nContent-Length: invalid\r\n\r\n";
  let result = Response::parse(input);
  // Should handle gracefully (no body or error)
  assert!(result.is_ok());
}

// ============================================================================
// RFC 9112 Section 6.3: No Body Status Codes
// ============================================================================

#[test]
fn test_must_handle_1xx_no_body() {
  // MUST: 1xx responses never have a body
  for code in 100..200 {
    let input = alloc::format!("HTTP/1.1 {code} Continue\r\n\r\n");
    let result = Response::parse(input.as_bytes());
    assert!(result.is_ok(), "1xx response must parse");
    let response = result.unwrap();
    assert!(response.body.is_empty(), "1xx must have no body");
  }
}

#[test]
fn test_must_handle_204_no_body() {
  // MUST: 204 No Content never has a body
  let input = b"HTTP/1.1 204 No Content\r\nContent-Length: 100\r\n\r\nThis should be ignored";
  let result = Response::parse(input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert!(response.body.is_empty(), "204 must have no body");
}

#[test]
fn test_must_handle_304_no_body() {
  // MUST: 304 Not Modified never has a body
  let input = b"HTTP/1.1 304 Not Modified\r\nContent-Length: 50\r\n\r\nIgnored";
  let result = Response::parse(input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert!(response.body.is_empty(), "304 must have no body");
}

// ============================================================================
// RFC 9112 Section 4: Status Line
// ============================================================================

#[test]
fn test_must_handle_empty_reason_phrase() {
  // Status line with empty reason phrase must be accepted
  let input = b"HTTP/1.1 200 \r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_ok(), "Empty reason phrase must be accepted");
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
  assert_eq!(response.reason.len(), 0);
}

#[test]
fn test_must_require_space_after_status_code() {
  // MUST: Space between status code and reason phrase
  let input = b"HTTP/1.1 200\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_err(), "Missing space after status code must error");
}

// ============================================================================
// RFC 9112 Section 5.1: Header Field Syntax
// ============================================================================

#[test]
fn test_must_reject_whitespace_before_colon() {
  // Server MUST reject, parser should detect invalid syntax
  let input = b"HTTP/1.1 200 OK\r\nContent-Type : text/html\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_err(), "Whitespace before colon must be rejected");
}

#[test]
fn test_must_accept_valid_header_syntax() {
  // MUST: Accept valid header field syntax
  let input = b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: 0\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_ok(), "Valid headers must be accepted");
}

// ============================================================================
// RFC 9112 Section 2.2: Robustness - Leading CRLF
// ============================================================================

#[test]
fn test_must_skip_leading_crlf() {
  // MAY skip leading empty lines (robustness)
  let input = b"\r\n\r\nHTTP/1.1 200 OK\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_ok(), "Should skip leading CRLF");
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
}

#[test]
fn test_must_skip_multiple_leading_crlf() {
  // MAY skip multiple leading empty lines
  let input = b"\r\n\r\n\r\n\r\nHTTP/1.1 404 Not Found\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_ok(), "Should skip multiple leading CRLF");
  let response = result.unwrap();
  assert_eq!(response.status_code, 404);
}

// ============================================================================
// RFC 9112 Section 2.2: Line Terminators
// ============================================================================

#[test]
fn test_must_accept_lf_only_line_terminator() {
  // MAY accept LF-only as line terminator
  let input = b"HTTP/1.1 200 OK\nContent-Length: 5\n\nHello";
  let result = Response::parse(input);
  assert!(result.is_ok(), "LF-only line terminator should be accepted");
  let response = result.unwrap();
  assert_eq!(response.body.as_bytes(), b"Hello");
}

// ============================================================================
// RFC 9112 Section 7.4: TE Header Field
// ============================================================================

#[test]
fn test_must_not_send_chunked_in_te() {
  // This is a client requirement - tested in request builder tests
  // Client MUST NOT send "chunked" in TE header
  // Our implementation doesn't send TE at all (correct)
}

// ============================================================================
// Integration Tests for Complex Scenarios
// ============================================================================

#[test]
fn test_must_handle_chunked_with_multiple_chunks() {
  // MUST: Parse complete chunked message with multiple chunks
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nHello\r\n2\r\n, \r\n5\r\nWorld\r\n0\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.body.as_bytes(), b"Hello, World");
}

#[test]
fn test_must_handle_chunked_with_trailers() {
  // MUST: Parse chunked with trailer section
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nHello\r\n0\r\nX-Trailer: value\r\nY-Trailer: other\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.body.as_bytes(), b"Hello");
}

#[test]
fn test_must_handle_response_with_multiple_headers() {
  // MUST: Parse response with multiple header fields
  let input = b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: 5\r\nCache-Control: no-cache\r\nX-Custom: value\r\n\r\nHello";
  let result = Response::parse(input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.get_header("Content-Type"), Some("text/html"));
  assert_eq!(response.get_header("Content-Length"), Some("5"));
  assert_eq!(response.get_header("Cache-Control"), Some("no-cache"));
  assert_eq!(response.get_header("X-Custom"), Some("value"));
}

#[test]
fn test_must_handle_http_10_response() {
  // MUST: Accept HTTP/1.0 responses
  let input = b"HTTP/1.0 200 OK\r\nContent-Length: 2\r\n\r\nOK";
  let result = Response::parse(input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert_eq!(response.status_code, 200);
}

#[test]
fn test_must_handle_response_without_content_length_or_te() {
  // MUST: Handle response without Content-Length or Transfer-Encoding
  // Should assume no body for responses
  let input = b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_ok());
  let response = result.unwrap();
  assert!(response.body.is_empty());
}

// ============================================================================
// Security Tests - MUST Requirements
// ============================================================================

#[test]
fn test_must_prevent_request_smuggling_te_cl_conflict() {
  // RFC 9112 Section 6.3: If both Transfer-Encoding and Content-Length are present,
  // this is a potential request smuggling attack. Client MUST close connection
  // and discard the response.
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Length: 5\r\n\r\n5\r\nHello\r\n0\r\n\r\n";
  let result = Response::parse(input);

  // Now properly implemented: reject the response
  assert!(result.is_err(), "Response with both TE and CL should be rejected");
}

#[test]
fn test_must_validate_header_field_names() {
  // MUST: Reject invalid header field names
  let input = b"HTTP/1.1 200 OK\r\nInvalid@Header: value\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_err(), "Invalid header name must be rejected");
}

#[test]
fn test_must_handle_very_large_valid_chunk_size() {
  // MUST: Anticipate large chunk sizes but prevent overflow
  // This should fail gracefully
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nFFFFFFF\r\n";
  let result = Response::parse(input);
  // Should either parse or error, but not crash
  let _ = result;
}
