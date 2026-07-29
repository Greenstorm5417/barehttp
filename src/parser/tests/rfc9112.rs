//! Unique RFC 9112 MUST cases not covered by `message_body` / `chunked` / `security` / `status_line`.
use crate::error::ParseError;
use crate::parser::*;
extern crate alloc;
use alloc::string::String;

// --- Message robustness / obs-fold ---

#[test]
fn obs_fold_replaced_with_spaces() {
  let input = b"HTTP/1.1 200 OK\r\nX-Long: first\r\n second\r\n\tthird\r\n\r\n";
  let response = Response::parse(input).unwrap();
  let value = response.get_header("X-Long").unwrap();
  assert!(value.contains("first") && value.contains("second") && value.contains("third"));
}

#[test]
fn leading_empty_lines_skipped() {
  let input = b"\r\n\nHTTP/1.1 200 OK\r\n\r\n";
  assert_eq!(Response::parse(input).unwrap().status_code, 200);
}

#[test]
fn lf_only_line_terminators_accepted() {
  let input = b"HTTP/1.1 200 OK\nContent-Length: 5\n\nHello";
  assert_eq!(Response::parse(input).unwrap().body.as_bytes(), b"Hello");
}

#[test]
fn http_version_case_sensitive() {
  assert!(Response::parse(b"http/1.1 200 OK\r\n\r\n").is_err());
}

#[test]
fn whitespace_before_colon_rejected() {
  assert!(Response::parse(b"HTTP/1.1 200 OK\r\nContent-Type : text/plain\r\n\r\n").is_err());
}

#[test]
fn invalid_header_name_rejected() {
  assert!(Response::parse(b"HTTP/1.1 200 OK\r\nInvalid@Header: value\r\n\r\n").is_err());
}

// --- Body framing edge cases ---

#[test]
fn te_cl_conflict_is_conflicting_framing() {
  let input =
    b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Length: 5\r\n\r\n5\r\nHello\r\n0\r\n\r\n";
  assert_eq!(Response::parse(input).unwrap_err(), ParseError::ConflictingFraming);
}

#[test]
fn identical_comma_separated_content_lengths_accepted() {
  let input = b"HTTP/1.1 200 OK\r\nContent-Length: 5, 5, 5\r\n\r\nHello";
  assert_eq!(Response::parse(input).unwrap().body.as_bytes(), b"Hello");
}

#[test]
fn trailers_not_merged_into_headers() {
  let input =
    b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nHello\r\n0\r\nX-Trailer: value\r\n\r\n";
  let response = Response::parse(input).unwrap();
  assert_eq!(response.body.as_bytes(), b"Hello");
  assert!(response.get_header("X-Trailer").is_none());
}

#[test]
fn connect_2xx_ignores_framing() {
  let input = b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nThis should be ignored";
  let (status_line, after_status) = crate::parser::status_line::StatusLine::parse(input).unwrap();
  let (headers_bytes, remaining) = crate::parser::headers::HeaderField::parse(after_status).unwrap();
  let body = Response::parse_body(
    remaining,
    &headers_bytes,
    status_line.status.as_u16(),
    Some("CONNECT"),
  )
  .unwrap();
  assert!(body.is_empty());
}

#[test]
fn te_rejected_on_http_10() {
  let input = b"HTTP/1.0 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nHello\r\n0\r\n\r\n";
  assert!(Response::parse(input).is_err());
}

#[test]
fn body_read_strategy_selection() {
  let empty = crate::headers::Headers::new();
  assert_eq!(Response::body_read_strategy(&empty, 100).unwrap(), BodyReadStrategy::NoBody);
  assert_eq!(Response::body_read_strategy(&empty, 204).unwrap(), BodyReadStrategy::NoBody);
  assert_eq!(Response::body_read_strategy(&empty, 304).unwrap(), BodyReadStrategy::NoBody);

  let mut cl = crate::headers::Headers::new();
  cl.insert("Content-Length", "100");
  assert_eq!(
    Response::body_read_strategy(&cl, 200).unwrap(),
    BodyReadStrategy::ContentLength(100)
  );

  let mut te = crate::headers::Headers::new();
  te.insert("Transfer-Encoding", "chunked");
  assert_eq!(
    Response::body_read_strategy(&te, 200).unwrap(),
    BodyReadStrategy::Chunked
  );

  let mut gzip = crate::headers::Headers::new();
  gzip.insert("Transfer-Encoding", "gzip");
  assert_eq!(
    Response::body_read_strategy(&gzip, 200).unwrap(),
    BodyReadStrategy::UntilClose
  );

  // no CL/TE → until connection close (RFC 9112 §6.3)
  assert_eq!(
    Response::body_read_strategy(&empty, 200).unwrap(),
    BodyReadStrategy::UntilClose
  );
}

// --- WireRequest ---

#[test]
fn empty_path_becomes_slash() {
  let request = WireRequest::new("GET", "")
    .header("Host", "example.com")
    .build()
    .unwrap();
  assert!(String::from_utf8_lossy(&request).starts_with("GET / HTTP/1.1\r\n"));
}

#[test]
fn request_no_extra_crlf_around_body() {
  let request = WireRequest::new("POST", "/")
    .header("Host", "example.com")
    .body(b"test body".to_vec())
    .build()
    .unwrap();
  assert!(request.starts_with(b"POST"));
  assert!(request.ends_with(b"test body"));
  assert!(!request.ends_with(b"test body\r\n"));
}

#[test]
fn host_header_required() {
  assert_eq!(
    WireRequest::new("GET", "/").build().unwrap_err(),
    ParseError::MissingHostHeader
  );
}

#[test]
fn multiple_host_headers_rejected() {
  let result = WireRequest::new("GET", "/")
    .header("Host", "example.com")
    .header("host", "another.com")
    .build();
  assert_eq!(result.unwrap_err(), ParseError::MultipleHostHeaders);
}

#[test]
fn invalid_host_value_rejected() {
  let result = WireRequest::new("GET", "/")
    .header("Host", "example .com")
    .build();
  assert_eq!(result.unwrap_err(), ParseError::InvalidHostHeaderValue);
}

#[test]
fn chunked_applied_twice_rejected() {
  let result = WireRequest::new("POST", "/")
    .header("Host", "example.com")
    .header("Transfer-Encoding", "chunked, chunked")
    .build();
  assert_eq!(result.unwrap_err(), ParseError::ChunkedAppliedMultipleTimes);
}

#[test]
fn request_te_and_cl_conflict_rejected() {
  let result = WireRequest::new("POST", "/")
    .header("Host", "example.com")
    .header("Transfer-Encoding", "chunked")
    .header("Content-Length", "4")
    .body(b"test".to_vec())
    .build();
  assert_eq!(result.unwrap_err(), ParseError::ConflictingFraming);
}

#[test]
fn request_rejects_bare_cr_and_obs_fold_in_headers() {
  assert_eq!(
    WireRequest::new("GET", "/")
      .header("Host", "example.com")
      .header("X-Bad", "value\rwith\rCR")
      .build()
      .unwrap_err(),
    ParseError::InvalidHeaderValue
  );
  assert_eq!(
    WireRequest::new("GET", "/")
      .header("Host", "example.com")
      .header("X-Bad", "line1\r\n line2")
      .build()
      .unwrap_err(),
    ParseError::InvalidHeaderValue
  );
}

#[test]
fn te_header_rules() {
  assert_eq!(
    WireRequest::new("GET", "/")
      .header("Host", "example.com")
      .header("TE", "chunked, gzip")
      .build()
      .unwrap_err(),
    ParseError::ChunkedInTeHeader
  );
  assert_eq!(
    WireRequest::new("GET", "/")
      .header("Host", "example.com")
      .header("TE", "gzip")
      .build()
      .unwrap_err(),
    ParseError::TeHeaderMissingConnection
  );
}
