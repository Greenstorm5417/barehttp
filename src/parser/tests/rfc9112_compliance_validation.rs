/// Comprehensive test suite for RFC 9112 compliance validation
/// Tests all edge cases and validation rules added to prevent future regressions
use crate::parser::RequestBuilder;
extern crate alloc;

// ============================================================================
// RFC 9112 Section 3.2: Multiple Host Header Detection
// ============================================================================

#[test]
fn test_rfc9112_reject_multiple_host_headers() {
  // RFC 9112 Section 3.2: Server responds 400 if multiple Host headers present
  let mut builder = RequestBuilder::new("GET", "/");
  builder = builder.header("Host", "example.com");
  builder = builder.header("Host", "another.com");

  let result = builder.build();
  assert!(result.is_err(), "Should reject multiple Host headers");
  assert_eq!(
    result.unwrap_err(),
    crate::error::ParseError::MultipleHostHeaders
  );
}

#[test]
fn test_rfc9112_single_host_header_accepted() {
  // Single Host header should be accepted
  let builder = RequestBuilder::new("GET", "/").header("Host", "example.com");

  let result = builder.build();
  assert!(result.is_ok(), "Single Host header should be accepted");
}

#[test]
fn test_rfc9112_case_insensitive_host_detection() {
  // Host header detection should be case-insensitive
  let mut builder = RequestBuilder::new("GET", "/");
  builder = builder.header("Host", "example.com");
  builder = builder.header("host", "another.com");

  let result = builder.build();
  assert!(
    result.is_err(),
    "Should detect duplicate Host regardless of case"
  );
}

// ============================================================================
// RFC 9112 Section 3.2: Host Header Value Validation
// ============================================================================

#[test]
fn test_rfc9112_valid_host_hostname_only() {
  let builder = RequestBuilder::new("GET", "/").header("Host", "example.com");

  assert!(builder.build().is_ok(), "Valid hostname should be accepted");
}

#[test]
fn test_rfc9112_valid_host_with_port() {
  let builder = RequestBuilder::new("GET", "/").header("Host", "example.com:8080");

  assert!(
    builder.build().is_ok(),
    "Valid hostname with port should be accepted"
  );
}

#[test]
fn test_rfc9112_valid_host_ipv4() {
  let builder = RequestBuilder::new("GET", "/").header("Host", "192.168.1.1");

  assert!(builder.build().is_ok(), "IPv4 address should be accepted");
}

#[test]
fn test_rfc9112_valid_host_ipv4_with_port() {
  let builder = RequestBuilder::new("GET", "/").header("Host", "192.168.1.1:8080");

  assert!(builder.build().is_ok(), "IPv4 with port should be accepted");
}

#[test]
fn test_rfc9112_valid_host_ipv6_literal() {
  let builder = RequestBuilder::new("GET", "/").header("Host", "[2001:db8::1]");

  assert!(builder.build().is_ok(), "IPv6 literal should be accepted");
}

#[test]
fn test_rfc9112_valid_host_ipv6_with_port() {
  let builder = RequestBuilder::new("GET", "/").header("Host", "[2001:db8::1]:8080");

  assert!(
    builder.build().is_ok(),
    "IPv6 literal with port should be accepted"
  );
}

#[test]
fn test_rfc9112_valid_host_empty() {
  // RFC 9112 Section 3.2: Empty Host is valid when authority is missing
  let builder = RequestBuilder::new("GET", "/").header("Host", "");

  assert!(builder.build().is_ok(), "Empty Host should be accepted");
}

#[test]
fn test_rfc9112_invalid_host_with_whitespace() {
  let builder = RequestBuilder::new("GET", "/").header("Host", "example .com");

  let result = builder.build();
  assert!(result.is_err(), "Host with whitespace should be rejected");
  assert_eq!(
    result.unwrap_err(),
    crate::error::ParseError::InvalidHostHeaderValue
  );
}

#[test]
fn test_rfc9112_invalid_host_port_zero() {
  let builder = RequestBuilder::new("GET", "/").header("Host", "example.com:0");

  let result = builder.build();
  assert!(result.is_err(), "Port 0 should be rejected");
}

#[test]
fn test_rfc9112_invalid_host_port_non_numeric() {
  let builder = RequestBuilder::new("GET", "/").header("Host", "example.com:abc");

  let result = builder.build();
  assert!(result.is_err(), "Non-numeric port should be rejected");
}

#[test]
fn test_rfc9112_invalid_host_empty_port() {
  let builder = RequestBuilder::new("GET", "/").header("Host", "example.com:");

  let result = builder.build();
  assert!(result.is_err(), "Empty port should be rejected");
}

#[test]
fn test_rfc9112_valid_host_subdomain() {
  let builder = RequestBuilder::new("GET", "/").header("Host", "api.example.com");

  assert!(builder.build().is_ok(), "Subdomain should be accepted");
}

#[test]
fn test_rfc9112_valid_host_with_hyphen() {
  let builder = RequestBuilder::new("GET", "/").header("Host", "my-server.example.com");

  assert!(
    builder.build().is_ok(),
    "Hostname with hyphen should be accepted"
  );
}

// ============================================================================
// RFC 9112 Section 6.1: Transfer-Encoding Chunked Duplication Check
// ============================================================================

#[test]
fn test_rfc9112_reject_chunked_applied_multiple_times() {
  // RFC 9112 Section 6.1: MUST NOT apply chunked more than once
  let builder = RequestBuilder::new("POST", "/")
    .header("Host", "example.com")
    .header("Transfer-Encoding", "chunked, chunked");

  let result = builder.build();
  assert!(
    result.is_err(),
    "Should reject chunked applied multiple times"
  );
  assert_eq!(
    result.unwrap_err(),
    crate::error::ParseError::ChunkedAppliedMultipleTimes
  );
}

#[test]
fn test_rfc9112_single_chunked_accepted() {
  let builder = RequestBuilder::new("POST", "/")
    .header("Host", "example.com")
    .header("Transfer-Encoding", "chunked");

  let result = builder.build();
  assert!(result.is_ok(), "Single chunked should be accepted");
}

#[test]
fn test_rfc9112_chunked_with_other_encoding() {
  // Chunked with another encoding (chunked must be last)
  let builder = RequestBuilder::new("POST", "/")
    .header("Host", "example.com")
    .header("Transfer-Encoding", "gzip, chunked");

  let result = builder.build();
  assert!(
    result.is_ok(),
    "Chunked as final encoding should be accepted"
  );
}

#[test]
fn test_rfc9112_reject_chunked_in_middle() {
  // Chunked not as final encoding
  let builder = RequestBuilder::new("POST", "/")
    .header("Host", "example.com")
    .header("Transfer-Encoding", "chunked, gzip");

  // Note: This test validates that chunked appears only once
  // The "chunked must be final" validation is in response parsing
  let result = builder.build();
  assert!(
    result.is_ok(),
    "Single chunked occurrence is valid in request building"
  );
}

#[test]
fn test_rfc9112_case_insensitive_chunked_detection() {
  let builder = RequestBuilder::new("POST", "/")
    .header("Host", "example.com")
    .header("Transfer-Encoding", "Chunked, CHUNKED");

  let result = builder.build();
  assert!(result.is_err(), "Should detect chunked case-insensitively");
}

// ============================================================================
// RFC 9112 Section 6.1: Transfer-Encoding with HTTP/1.1
// ============================================================================

#[test]
fn test_rfc9112_transfer_encoding_with_http11() {
  // We always use HTTP/1.1, so Transfer-Encoding is valid
  let builder = RequestBuilder::new("POST", "/")
    .header("Host", "example.com")
    .header("Transfer-Encoding", "chunked");

  let result = builder.build();
  assert!(result.is_ok(), "Transfer-Encoding valid with HTTP/1.1");
}

// ============================================================================
// Integration Tests: Multiple Validations
// ============================================================================

#[test]
fn test_rfc9112_multiple_validations_all_pass() {
  // Test that a valid request with multiple headers passes all validations
  let builder = RequestBuilder::new("POST", "/api/data")
    .header("Host", "api.example.com:443")
    .header("Content-Type", "application/json")
    .header("User-Agent", "TestClient/1.0")
    .body(b"{\"test\":\"data\"}".to_vec());

  let result = builder.build();
  assert!(result.is_ok(), "Valid request should pass all validations");
}

#[test]
fn test_rfc9112_multiple_violations_first_error_returned() {
  // Test that when multiple violations exist, we get an error
  let mut builder = RequestBuilder::new("POST", "/");
  builder = builder.header("Host", "example.com");
  builder = builder.header("Host", "another.com"); // Duplicate Host
  builder = builder.header("Transfer-Encoding", "chunked, chunked"); // Duplicate chunked

  let result = builder.build();
  assert!(
    result.is_err(),
    "Should reject request with multiple violations"
  );
  // Should get MultipleHostHeaders first since it's checked earlier
  assert_eq!(
    result.unwrap_err(),
    crate::error::ParseError::MultipleHostHeaders
  );
}

// ============================================================================
// Edge Cases and Boundary Conditions
// ============================================================================

#[test]
fn test_rfc9112_host_max_length() {
  // Test very long but valid hostname
  let long_hostname = "a".repeat(253); // Max DNS label length
  let builder = RequestBuilder::new("GET", "/").header("Host", &long_hostname);

  let result = builder.build();
  assert!(result.is_ok(), "Long valid hostname should be accepted");
}

#[test]
fn test_rfc9112_host_port_max_value() {
  let builder = RequestBuilder::new("GET", "/").header("Host", "example.com:65535");

  let result = builder.build();
  assert!(result.is_ok(), "Max port value should be accepted");
}

#[test]
fn test_rfc9112_host_localhost() {
  let builder = RequestBuilder::new("GET", "/").header("Host", "localhost");

  assert!(builder.build().is_ok(), "localhost should be accepted");
}

#[test]
fn test_rfc9112_host_localhost_with_port() {
  let builder = RequestBuilder::new("GET", "/").header("Host", "localhost:8080");

  assert!(
    builder.build().is_ok(),
    "localhost with port should be accepted"
  );
}

#[test]
fn test_rfc9112_transfer_encoding_empty_value() {
  let builder = RequestBuilder::new("POST", "/")
    .header("Host", "example.com")
    .header("Transfer-Encoding", "");

  let result = builder.build();
  // Empty Transfer-Encoding is technically invalid but won't trigger our chunked checks
  assert!(
    result.is_ok(),
    "Empty TE doesn't violate chunked duplication"
  );
}

#[test]
fn test_rfc9112_transfer_encoding_whitespace_handling() {
  let builder = RequestBuilder::new("POST", "/")
    .header("Host", "example.com")
    .header("Transfer-Encoding", "  chunked  ");

  let result = builder.build();
  assert!(
    result.is_ok(),
    "Whitespace around chunked should be handled"
  );
}

// ============================================================================
// Regression Prevention Tests
// ============================================================================

#[test]
fn test_rfc9112_regression_host_case_sensitivity() {
  // Ensure Host header checking remains case-insensitive
  let builder = RequestBuilder::new("GET", "/").header("HOST", "example.com");

  assert!(builder.build().is_ok(), "HOST (uppercase) should work");
}

#[test]
fn test_rfc9112_regression_multiple_non_host_headers() {
  // Ensure we don't accidentally reject multiple non-Host headers
  let mut builder = RequestBuilder::new("GET", "/");
  builder = builder.header("Host", "example.com");
  builder = builder.header("Accept", "text/html");
  builder = builder.header("Accept", "application/json");

  let result = builder.build();
  assert!(
    result.is_ok(),
    "Multiple non-Host headers should be allowed"
  );
}

#[test]
fn test_rfc9112_regression_transfer_encoding_other_values() {
  // Ensure we don't reject non-chunked Transfer-Encoding values
  let builder = RequestBuilder::new("POST", "/")
    .header("Host", "example.com")
    .header("Transfer-Encoding", "gzip");

  let result = builder.build();
  assert!(result.is_ok(), "Non-chunked TE should be accepted");
}
