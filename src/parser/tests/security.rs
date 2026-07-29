use crate::error::ParseError;
use crate::parser::*;
extern crate alloc;
use alloc::vec::Vec;

#[test]
fn test_response_splitting_crlf_injection_in_reason() {
  let input = b"HTTP/1.1 200 OK\r\nInjected\r\nX-Evil: header\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_request_smuggling_conflicting_content_lengths() {
  // RFC 9112 Section 6.3: Duplicate Content-Length headers with different values
  // should be rejected to prevent request smuggling attacks
  let input = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nContent-Length: 10\r\n\r\nHelloWorld";
  let result = Response::parse(input);
  assert!(result.is_err(), "Should reject conflicting Content-Length headers");
}

#[test]
fn test_header_injection_null_byte() {
  let input = b"HTTP/1.1 200 OK\r\nX-Header: value\x00injected\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_ok());
}

#[test]
fn test_negative_content_length_rejected() {
  let input = b"HTTP/1.1 200 OK\r\nContent-Length: -1\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_chunked_extension_dos_attack() {
  let mut input = Vec::from(&b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5"[..]);
  for _ in 0..1000 {
    input.extend_from_slice(b";ext=val");
  }
  input.extend_from_slice(b"\r\nHello\r\n0\r\n\r\n");
  let result = Response::parse(&input);
  assert!(result.is_ok());
}

#[test]
fn test_header_name_with_control_chars() {
  let input = b"HTTP/1.1 200 OK\r\nX-\x01Header: value\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_bare_cr_in_header_value() {
  let input = b"HTTP/1.1 200 OK\r\nX-Header: val\rue\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_multiple_transfer_encoding_headers() {
  let input =
    b"HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nHello\r\n0\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_ok());
}

#[test]
fn test_whitespace_before_header_name() {
  let input = b"HTTP/1.1 200 OK\r\n Content-Type: text/html\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_chunked_smuggling_incomplete_chunk() {
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nHello\r\n5\r\nWorld";
  let result = Response::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_unicode_in_reason_phrase() {
  let input = "HTTP/1.1 200 Café\r\n\r\n".as_bytes();
  let result = Response::parse(input);
  assert!(result.is_ok());
}

#[test]
fn test_extremely_large_content_length() {
  let input = b"HTTP/1.1 200 OK\r\nContent-Length: 999999999999999\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_header_value_with_embedded_crlf() {
  let input = b"HTTP/1.1 200 OK\r\nX-Header: value\r\ninjected\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_chunked_with_negative_size() {
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n-5\r\nHello\r\n0\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_err());
}

#[test]
fn test_transfer_encoding_identity_not_chunked() {
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: identity\r\n\r\nPlaintext";
  let result = Response::parse(input);
  assert!(result.is_ok());
}

#[test]
fn test_header_with_vertical_tab() {
  let input = b"HTTP/1.1 200 OK\r\nX-Header:\x0Bvalue\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_ok());
}

#[test]
fn test_chunked_zero_chunk_not_last() {
  // RFC 9112 Section 6.3: Extra data after chunked terminator (0\r\n\r\n)
  // MUST NOT be processed as a separate response
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n5\r\nHello\r\n0\r\n\r\n";
  let result = Response::parse(input);
  assert!(result.is_err(), "Should reject extra data after chunked terminator");
}

#[test]
fn test_te_and_content_length_conflict() {
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Length: 5\r\n\r\n5\r\nHello\r\n0\r\n\r\n";
  let result = Response::parse(input);
  assert!(matches!(result, Err(ParseError::ConflictingFraming)), "got {result:?}");
}

#[test]
fn test_chunked_not_final_coding() {
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked, gzip\r\n\r\n5\r\nHello\r\n0\r\n\r\n";
  let result = Response::parse(input);
  assert!(matches!(result, Err(ParseError::ChunkedNotFinal)), "got {result:?}");
}

#[test]
fn test_obs_fold_rejected() {
  let input = b"HTTP/1.1 200 OK\r\nX-Fold: line1\r\n continued\r\n\r\n";
  let result = Response::parse(input);
  assert!(
    matches!(result, Err(ParseError::ObsoleteFoldInHeader)),
    "got {result:?}"
  );
}

#[test]
fn test_invalid_header_name_space() {
  let input = b"HTTP/1.1 200 OK\r\nBad Name: value\r\n\r\n";
  let result = Response::parse(input);
  assert!(matches!(result, Err(ParseError::InvalidHeaderName)));
}

#[test]
fn test_status_line_not_http() {
  let input = b"HTP/1.1 200 OK\r\n\r\n";
  let result = Response::parse(input);
  assert!(matches!(result, Err(ParseError::InvalidHttpVersion)));
}

#[test]
fn test_bare_lf_only_status_still_parses_or_rejects_consistently() {
  // LF-only message framing: either accepted (lenient) or rejected — must not panic.
  let input = b"HTTP/1.1 200 OK\nContent-Length: 0\n\n";
  let _ = Response::parse(input);
}

#[cfg(feature = "gzip-decompression")]
#[test]
fn test_truncated_gzip_body_rejected() {
  let input = b"HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: 4\r\n\r\n\x1f\x8b\x08\x00";
  let result = Response::parse(input);
  assert!(matches!(result, Err(ParseError::DecompressionFailed)), "got {result:?}");
}

#[cfg(feature = "gzip-decompression")]
#[test]
fn test_corrupt_gzip_body_rejected() {
  let junk = [0xffu8; 32];
  let mut msg = Vec::from(&b"HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: 32\r\n\r\n"[..]);
  msg.extend_from_slice(&junk);
  let result = Response::parse(&msg);
  assert!(matches!(result, Err(ParseError::DecompressionFailed)), "got {result:?}");
}
