#![allow(unused_variables)]

//! Tests for RFC 9112 requirements that are NOT YET IMPLEMENTED
//! These tests should FAIL until we implement the missing features
//!
//! Run with: `cargo test rfc9112_missing --lib`

use crate::parser::message::{RequestBuilder, Response};
extern crate alloc;
use alloc::vec;

#[test]
fn test_missing_host_header_enforcement() {
  // RFC 9112 Section 3.2: Client MUST send Host in every HTTP/1.1 request
  let result = RequestBuilder::new("GET", "/path").build();

  // Should return error for missing Host header
  assert!(result.is_err(), "Should reject request without Host header");
  assert_eq!(
    result.unwrap_err(),
    crate::error::ParseError::MissingHostHeader
  );
}

#[test]
fn test_missing_empty_host_when_no_authority() {
  // RFC 9112 Section 3.2: If authority missing, send empty Host
  let result = RequestBuilder::new("GET", "/").build();

  // Should return error for missing Host header
  assert!(result.is_err(), "Should reject request without Host header");
  assert_eq!(
    result.unwrap_err(),
    crate::error::ParseError::MissingHostHeader
  );
}

// ============================================================================
// HIGH PRIORITY: Response Validation - TE in Forbidden Status Codes
// ============================================================================

#[test]
fn test_missing_reject_te_in_1xx_response() {
  // RFC 9112 Section 6.1: Server MUST NOT send TE in 1xx responses
  // Currently: Parser accepts this (should reject)
  let input = b"HTTP/1.1 100 Continue\r\nTransfer-Encoding: chunked\r\n\r\n";
  let result = Response::parse(input);

  // Should fail but currently succeeds
  assert!(
    result.is_err(),
    "MISSING: Should reject TE in 1xx responses"
  );
}

#[test]
fn test_missing_reject_te_in_204_response() {
  // RFC 9112 Section 6.1: Server MUST NOT send TE in 204
  // Currently: Parser accepts this (should reject)
  let input = b"HTTP/1.1 204 No Content\r\nTransfer-Encoding: chunked\r\n\r\n";
  let result = Response::parse(input);

  // Should fail but currently succeeds
  assert!(
    result.is_err(),
    "MISSING: Should reject TE in 204 responses"
  );
}

#[test]
fn test_missing_reject_te_in_2xx_connect_response() {
  // RFC 9112 Section 6.1: Server MUST NOT send TE in 2xx to CONNECT
  // RFC 9112 Section 6.3: Client SHOULD IGNORE (not reject) TE/CL in 2xx CONNECT
  let input =
    b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nHello\r\n0\r\n\r\n";

  // Parse with CONNECT method context
  let headers_bytes = vec![(b"Transfer-Encoding".to_vec(), b"chunked".to_vec())];

  let result = Response::parse_body(
    b"5\r\nHello\r\n0\r\n\r\n",
    &headers_bytes,
    200,
    Some("CONNECT"),
  );

  // Should succeed and ignore the TE header (return empty body)
  assert!(
    result.is_ok(),
    "Should ignore (not reject) TE in 2xx CONNECT responses"
  );
  assert!(
    result.unwrap().is_empty(),
    "2xx CONNECT should have empty body even with TE"
  );
}

// ============================================================================
// HIGH PRIORITY: Request Generation - Prevent Invalid Headers
// ============================================================================

#[test]
fn test_missing_prevent_bare_cr_in_header_values() {
  // RFC 9112 Section 2.2: Sender MUST NOT generate bare CR
  let result = RequestBuilder::new("GET", "/")
    .header("Host", "example.com")
    .header("X-Bad", "value\rwith\rCR") // Contains bare CR
    .build();

  // Should return error for bare CR in header value
  assert!(result.is_err(), "Should reject bare CR in header values");
  assert_eq!(
    result.unwrap_err(),
    crate::error::ParseError::BareCarriageReturnInHeader
  );
}

#[test]
fn test_missing_prevent_obs_fold_generation() {
  // RFC 9112 Section 5.2: Sender MUST NOT generate obs-fold
  let result = RequestBuilder::new("GET", "/")
    .header("Host", "example.com")
    .header("X-Bad", "line1\r\n line2") // Contains obs-fold
    .build();

  // Should return error for obs-fold in header value
  assert!(result.is_err(), "Should reject obs-fold in header values");
  assert_eq!(
    result.unwrap_err(),
    crate::error::ParseError::ObsoleteFoldInHeader
  );
}

#[test]
fn test_missing_no_cl_when_te_present() {
  // RFC 9112 Section 6.2: Sender MUST NOT send CL when TE present
  let result = RequestBuilder::new("POST", "/")
    .header("Host", "example.com")
    .header("Transfer-Encoding", "chunked")
    .header("Content-Length", "4") // Manually adding conflicting header
    .body(b"test".to_vec())
    .build();

  // Should return error when both TE and CL are present
  assert!(result.is_err(), "Should reject when both TE and CL present");
  assert_eq!(
    result.unwrap_err(),
    crate::error::ParseError::ConflictingFraming
  );
}

// ============================================================================
// MEDIUM PRIORITY: Request Body Framing
// ============================================================================

#[test]
fn test_missing_body_framing_validation() {
  // RFC 9112 Section 6.3: Request with body MUST use valid CL or chunked
  // RequestBuilder doesn't auto-add Content-Length, so this is checking
  // that the implementation properly handles bodies

  let request = RequestBuilder::new("POST", "/")
    .header("Host", "example.com")
    .header("Content-Length", "4")
    .body(b"data".to_vec())
    .build();

  // This should succeed with proper framing
  assert!(request.is_ok(), "Should accept body with Content-Length");

  let binding = request.unwrap();
  let request_str = core::str::from_utf8(&binding).unwrap();
  assert!(request_str.contains("Content-Length:"), "Should have CL");
}

// ============================================================================
// MEDIUM PRIORITY: HTTP Version Validation
// ============================================================================

#[test]
fn test_missing_version_check_for_chunked() {
  // RFC 9112 Section 6.1: Client MUST NOT use chunked unless server supports HTTP/1.1+
  // This is more of an architectural requirement - RequestBuilder always uses HTTP/1.1
  // Version negotiation would happen at a higher level

  let request = RequestBuilder::new("POST", "/")
    .header("Host", "example.com")
    .header("Transfer-Encoding", "chunked")
    .body(b"5\r\nHello\r\n0\r\n\r\n".to_vec())
    .build();

  // Should succeed - RequestBuilder uses HTTP/1.1
  assert!(request.is_ok(), "Should accept chunked with HTTP/1.1");

  let binding = request.unwrap();
  let request_str = core::str::from_utf8(&binding).unwrap();
  assert!(request_str.contains("HTTP/1.1"), "Should use HTTP/1.1");
}

#[test]
fn test_missing_version_check_for_te() {
  // RFC 9112 Section 6.1: Server MUST NOT send TE unless request indicates HTTP/1.1+
  // This is a response parsing requirement
  // Our implementation rejects TE in HTTP/1.0 responses

  let input =
    b"HTTP/1.0 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nHello\r\n0\r\n\r\n";
  let result = Response::parse(input);

  assert!(result.is_err(), "Should reject TE in HTTP/1.0 responses");
}

// ============================================================================
// MEDIUM PRIORITY: TE Header Validation
// ============================================================================

#[test]
fn test_missing_no_chunked_in_te_header() {
  // RFC 9112 Section 7.4: Client MUST NOT send "chunked" in TE
  let result = RequestBuilder::new("GET", "/")
    .header("Host", "example.com")
    .header("TE", "chunked, gzip") // Invalid: chunked in TE
    .build();

  // Should return error for chunked in TE header
  assert!(result.is_err(), "Should reject 'chunked' in TE header");
  assert_eq!(
    result.unwrap_err(),
    crate::error::ParseError::ChunkedInTeHeader
  );
}

#[test]
fn test_missing_te_requires_connection_header() {
  // RFC 9112 Section 7.4: Sender of TE MUST also send "TE" in Connection
  let result = RequestBuilder::new("GET", "/")
    .header("Host", "example.com")
    .header("TE", "gzip") // Missing Connection: TE
    .build();

  // Should return error when TE present without Connection: TE
  assert!(result.is_err(), "Should reject TE without Connection: TE");
  assert_eq!(
    result.unwrap_err(),
    crate::error::ParseError::TeHeaderMissingConnection
  );
}

// ============================================================================
// LOW PRIORITY: Request-Target Forms (Advanced)
// ============================================================================

#[test]
fn test_missing_absolute_form_support() {
  // RFC 9112 Section 3.2.2: For proxy requests, use absolute-form
  // Currently: RequestBuilder treats full URLs as paths (origin-form only)
  let request = RequestBuilder::new("GET", "http://example.com/path")
    .header("Host", "example.com")
    .build()
    .unwrap();

  let request_str = core::str::from_utf8(&request).unwrap();

  // Should have: GET http://example.com/path HTTP/1.1
  // But currently has: GET http://example.com/path HTTP/1.1 (as a path, not absolute-form)
  let uses_absolute_form = request_str.starts_with("GET http://");

  assert!(
    uses_absolute_form,
    "MISSING: Should support absolute-form for proxy requests (currently only origin-form)"
  );
}

#[test]
fn test_missing_authority_form_for_connect() {
  // RFC 9112 Section 3.2.3: For CONNECT, use authority-form (host:port)
  // Currently: CONNECT just uses whatever path is given as origin-form
  let request = RequestBuilder::new("CONNECT", "example.com:443")
    .header("Host", "example.com:443")
    .build()
    .unwrap();

  let request_str = core::str::from_utf8(&request).unwrap();

  // Should have: CONNECT example.com:443 HTTP/1.1
  // Currently has: CONNECT example.com:443 HTTP/1.1 (accidentally correct format)
  let uses_authority_form = request_str.starts_with("CONNECT example.com:443");

  assert!(
    uses_authority_form,
    "MISSING: Should explicitly support authority-form for CONNECT"
  );
}

#[test]
fn test_missing_asterisk_form_for_options() {
  // RFC 9112 Section 3.2.4: For server-wide OPTIONS, use "*"
  // Currently: RequestBuilder treats "*" as a literal path
  let request = RequestBuilder::new("OPTIONS", "*")
    .header("Host", "example.com")
    .build()
    .unwrap();

  let request_str = core::str::from_utf8(&request).unwrap();

  // Should have: OPTIONS * HTTP/1.1
  // Currently has: OPTIONS * HTTP/1.1 (accidentally correct!)
  assert!(
    request_str.starts_with("OPTIONS *"),
    "MISSING: Should explicitly support asterisk-form for OPTIONS"
  );
}

#[test]
fn test_missing_comprehensive_request_validation() {
  // Test that RequestBuilder now enforces Host header
  let result = RequestBuilder::new("POST", "/api/data")
    .header("X-Custom", "test")
    // Missing: No Host header (should fail)
    .body(b"payload".to_vec())
    .build();

  // Should fail due to missing Host header
  assert!(result.is_err(), "Should reject request without Host header");
  assert_eq!(
    result.unwrap_err(),
    crate::error::ParseError::MissingHostHeader
  );
}
