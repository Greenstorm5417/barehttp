//! RFC 9112 MUST cases beyond `message_body` / `chunked` / `security`.
use crate::error::ParseError;
use crate::parser::version::Version;
use crate::parser::*;
extern crate alloc;
use crate::headers::Headers;
use alloc::string::String;

#[test]
fn obs_fold_rejected() {
  let input = b"HTTP/1.1 200 OK\r\nX-Long: first\r\n second\r\n\tthird\r\n\r\n";
  assert_eq!(Response::parse(input).unwrap_err(), ParseError::ObsoleteFoldInHeader);
}

#[test]
fn leading_empty_lines_skipped() {
  let input = b"\r\n\nHTTP/1.1 200 OK\r\n\r\n";
  assert_eq!(Response::parse(input).unwrap().status_code(), 200);
}

#[test]
fn lf_only_line_terminators_accepted() {
  let input = b"HTTP/1.1 200 OK\nContent-Length: 5\n\nHello";
  assert_eq!(Response::parse(input).unwrap().body(), b"Hello");
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

#[test]
fn identical_comma_separated_content_lengths_accepted() {
  let input = b"HTTP/1.1 200 OK\r\nContent-Length: 5, 5, 5\r\n\r\nHello";
  assert_eq!(Response::parse(input).unwrap().body(), b"Hello");
}

#[test]
fn trailers_not_merged_into_headers() {
  let input = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nHello\r\n0\r\nX-Trailer: value\r\n\r\n";
  let response = Response::parse(input).unwrap();
  assert_eq!(response.body(), b"Hello");
  assert!(response.header("X-Trailer").is_none());
  assert_eq!(response.trailers().len(), 1);
  assert_eq!(response.trailers().get("X-Trailer"), Some("value"));
}

#[test]
fn te_rejected_on_http_10() {
  let input = b"HTTP/1.0 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nHello\r\n0\r\n\r\n";
  assert!(Response::parse(input).is_err());
}

#[test]
fn body_read_strategy_selection() {
  let empty = Headers::new();
  assert_eq!(
    Response::body_read_strategy(&empty, 100, Version::HTTP_11).unwrap(),
    BodyReadStrategy::NoBody
  );
  assert_eq!(
    Response::body_read_strategy(&empty, 204, Version::HTTP_11).unwrap(),
    BodyReadStrategy::NoBody
  );
  assert_eq!(
    Response::body_read_strategy(&empty, 304, Version::HTTP_11).unwrap(),
    BodyReadStrategy::NoBody
  );

  let mut cl = Headers::new();
  cl.insert("Content-Length", "100");
  assert_eq!(
    Response::body_read_strategy(&cl, 200, Version::HTTP_11).unwrap(),
    BodyReadStrategy::ContentLength(100)
  );

  let mut te = Headers::new();
  te.insert("Transfer-Encoding", "chunked");
  assert_eq!(
    Response::body_read_strategy(&te, 200, Version::HTTP_11).unwrap(),
    BodyReadStrategy::Chunked
  );

  let mut gzip = Headers::new();
  gzip.insert("Transfer-Encoding", "gzip");
  assert_eq!(
    Response::body_read_strategy(&gzip, 200, Version::HTTP_11).unwrap(),
    BodyReadStrategy::UntilClose
  );

  assert_eq!(
    Response::body_read_strategy(&empty, 200, Version::HTTP_11).unwrap(),
    BodyReadStrategy::UntilClose
  );
}

fn serialize_with_headers(
  method: &str,
  path: &str,
  headers: &Headers,
  body: Option<&[u8]>,
) -> Result<bytes::Bytes, ParseError> {
  Ok(serialize_request(method, path, headers, body)?.to_bytes())
}

#[test]
fn serialize_keeps_body_out_of_head_buffer() {
  let mut headers = Headers::new();
  headers.insert("Host", "example.com");
  let body = b"large-body-payload";
  let req = serialize_request("POST", "/", &headers, Some(body)).unwrap();
  assert!(req.head.ends_with(b"\r\n\r\n"));
  assert!(!req.head.windows(body.len()).any(|w| w == body));
  assert_eq!(req.body, body);
  let wire = req.to_bytes();
  assert!(wire.ends_with(body));
  assert!(
    core::str::from_utf8(&req.head)
      .unwrap()
      .contains("Content-Length: 18\r\n")
  );
}

#[test]
fn empty_path_becomes_slash() {
  let mut headers = Headers::new();
  headers.insert("Host", "example.com");
  let request = serialize_with_headers("GET", "", &headers, None).unwrap();
  assert!(String::from_utf8_lossy(&request).starts_with("GET / HTTP/1.1\r\n"));
}

#[test]
fn request_no_extra_crlf_around_body() {
  let mut headers = Headers::new();
  headers.insert("Host", "example.com");
  let request = serialize_with_headers("POST", "/", &headers, Some(b"test body")).unwrap();
  assert!(request.starts_with(b"POST"));
  assert!(request.ends_with(b"test body"));
  assert!(!request.ends_with(b"test body\r\n"));
}

#[test]
fn host_header_required() {
  assert_eq!(
    serialize_with_headers("GET", "/", &Headers::new(), None).unwrap_err(),
    ParseError::MissingHostHeader
  );
}

#[test]
fn multiple_host_headers_rejected() {
  let mut headers = Headers::new();
  headers.insert("Host", "example.com");
  headers.insert("host", "another.com");
  assert_eq!(
    serialize_with_headers("GET", "/", &headers, None).unwrap_err(),
    ParseError::MultipleHostHeaders
  );
}

#[test]
fn request_te_and_cl_conflict_rejected() {
  let mut headers = Headers::new();
  headers.insert("Host", "example.com");
  headers.insert("Transfer-Encoding", "chunked");
  headers.insert("Content-Length", "4");
  assert_eq!(
    serialize_with_headers("POST", "/", &headers, Some(b"test")).unwrap_err(),
    ParseError::ConflictingFraming
  );
}

#[test]
fn request_transfer_encoding_rejected() {
  let mut headers = Headers::new();
  headers.insert("Host", "example.com");
  headers.insert("Transfer-Encoding", "chunked");
  assert_eq!(
    serialize_with_headers("POST", "/", &headers, Some(b"test")).unwrap_err(),
    ParseError::RequestTransferEncodingUnsupported
  );
}

#[test]
fn request_rejects_content_length_mismatch() {
  let mut headers = Headers::new();
  headers.insert("Host", "example.com");
  headers.insert("Content-Length", "99");
  assert_eq!(
    serialize_with_headers("POST", "/", &headers, Some(b"test")).unwrap_err(),
    ParseError::InvalidContentLength
  );
}

#[test]
fn request_rejects_ctl_injection_in_headers() {
  let mut bad_cr = Headers::new();
  bad_cr.insert("Host", "example.com");
  bad_cr.insert("X-Bad", "value\rwith\rCR");
  assert_eq!(
    serialize_with_headers("GET", "/", &bad_cr, None).unwrap_err(),
    ParseError::InvalidHeaderValue
  );

  let mut bad_fold = Headers::new();
  bad_fold.insert("Host", "example.com");
  bad_fold.insert("X-Bad", "line1\r\n line2");
  assert_eq!(
    serialize_with_headers("GET", "/", &bad_fold, None).unwrap_err(),
    ParseError::InvalidHeaderValue
  );
}

#[test]
fn host_is_first_header_on_wire() {
  let mut headers = Headers::new();
  headers.insert("X-Custom", "1");
  headers.insert("Host", "example.com");
  let request = serialize_with_headers("GET", "/path", &headers, None).unwrap();
  let text = String::from_utf8_lossy(&request);
  assert!(text.starts_with("GET /path HTTP/1.1\r\nHost: example.com\r\n"));
}

#[test]
fn absolute_form_request_target_rejected() {
  let mut headers = Headers::new();
  headers.insert("Host", "example.com");
  assert_eq!(
    serialize_with_headers("GET", "http://example.com/path", &headers, None).unwrap_err(),
    ParseError::InvalidUri
  );
}

#[test]
fn invalid_host_value_rejected() {
  let mut headers = Headers::new();
  headers.insert("Host", "bad host.com");
  assert_eq!(
    serialize_with_headers("GET", "/", &headers, None).unwrap_err(),
    ParseError::InvalidHostHeaderValue
  );
}

#[test]
fn te_chunked_token_rejected() {
  let mut headers = Headers::new();
  headers.insert("Host", "example.com");
  headers.insert("TE", "chunked");
  headers.insert("Connection", "TE");
  assert_eq!(
    serialize_with_headers("GET", "/", &headers, None).unwrap_err(),
    ParseError::ChunkedInTeHeader
  );
}

#[test]
fn te_requires_connection_te() {
  let mut headers = Headers::new();
  headers.insert("Host", "example.com");
  headers.insert("TE", "trailers");
  assert_eq!(
    serialize_with_headers("GET", "/", &headers, None).unwrap_err(),
    ParseError::TeHeaderMissingConnection
  );
}

#[test]
fn duplicate_chunked_te_rejected() {
  let mut headers = Headers::new();
  headers.insert("Transfer-Encoding", "chunked, chunked");
  assert_eq!(
    Response::body_read_strategy(&headers, 200, Version::HTTP_11).unwrap_err(),
    ParseError::ChunkedAppliedMultipleTimes
  );
}
