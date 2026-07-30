//! Incremental TCP reads against the transport connection.

use crate::error::Error;
use crate::parser::has_complete_headers;
use crate::transport::connection::Connection;
use crate::transport::tests::mock_socket::MockSocket;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

fn read_ref(response: &str) -> (u16, Vec<u8>) {
  let mut socket = MockSocket::with_response(response);
  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);
  let raw = conn.read_raw_response(true).expect("unfragmented");
  (raw.status_code, raw.body_bytes.to_vec())
}

fn assert_matches_ref(
  response: &str,
  mut socket: MockSocket,
) {
  let (want_status, want_body) = read_ref(response);
  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);
  let raw = conn
    .read_raw_response(true)
    .unwrap_or_else(|e| panic!("fragmented read failed for split of {response:?}: {e:?}"));
  assert_eq!(raw.status_code, want_status);
  assert_eq!(raw.body_bytes.as_ref(), want_body.as_slice());
}

#[test]
fn content_length_body_one_byte_reads() {
  let response = "HTTP/1.1 200 OK\r\nContent-Length: 11\r\n\r\nHello World";
  let mut socket = MockSocket::with_max_read(response, 1);
  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);

  let raw = conn.read_raw_response(true).unwrap();
  assert_eq!(raw.status_code, 200);
  assert_eq!(&raw.body_bytes[..], b"Hello World");
}

#[test]
fn chunked_body_split_across_reads() {
  let response = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nHello\r\n6\r\n World\r\n0\r\n\r\n";
  let mut socket = MockSocket::with_max_read(response, 3);
  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);

  let raw = conn.read_raw_response(true).unwrap();
  assert_eq!(raw.status_code, 200);
  assert_eq!(&raw.body_bytes[..], b"Hello World");
  assert_eq!(
    raw
      .decoded_chunked_trailers
      .as_ref()
      .map(crate::headers::Headers::len),
    Some(0)
  );
}

#[test]
fn headers_incomplete_until_final_crlf() {
  let parts: &[&[u8]] = &[
    b"HTTP/1.1 ",
    b"200 OK\r\n",
    b"Content-Length: 4\r",
    b"\n\r\n",
    b"ping",
  ];
  let mut buf = Vec::new();
  for (i, part) in parts.iter().enumerate() {
    buf.extend_from_slice(part);
    let complete = has_complete_headers(&buf);
    if i < parts.len() - 2 {
      assert!(!complete, "headers should be incomplete after part {i}");
    }
  }
  assert!(has_complete_headers(&buf));

  let response = core::str::from_utf8(&buf).unwrap();
  let mut socket = MockSocket::with_max_read(response, 2);
  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);
  let raw = conn.read_raw_response(true).unwrap();
  assert_eq!(&raw.body_bytes[..], b"ping");
}

#[test]
fn truncated_headers_surface_as_socket_eof_or_parse() {
  // Incomplete header section: peer closes after partial write.
  let response = "HTTP/1.1 200 OK\r\nContent-Length: 5\r\n";
  let mut socket = MockSocket::with_max_read(response, 1);
  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);
  let err = conn.read_raw_response(true).unwrap_err();
  // Connection may map EOF mid-headers to Socket or Parse depending on path.
  assert!(
    matches!(err, Error::Socket(_) | Error::Parse(_) | Error::ResponseHeaderTooLarge),
    "unexpected error: {err:?}"
  );
}

#[test]
fn large_chunked_stream_byte_at_a_time() {
  let mut body = String::from("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n");
  for i in 0..20 {
    let chunk = format!("chunk-{i:02}");
    let size = format!("{:x}", chunk.len());
    body.push_str(&size);
    body.push_str("\r\n");
    body.push_str(&chunk);
    body.push_str("\r\n");
  }
  body.push_str("0\r\n\r\n");

  // One-byte reads stress the body framer; a few bytes still fragments every CRLF.
  let mut socket = MockSocket::with_max_read(&body, 1);
  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);
  let raw = conn
    .read_raw_response(true)
    .expect("fragmented chunked read");
  assert!(raw.body_bytes.starts_with(b"chunk-00"));
  assert!(raw.body_bytes.ends_with(b"chunk-19"));
  assert!(raw.decoded_chunked_trailers.is_some());
}

/// Two-fragment splits at every byte boundary for tiny fixtures (< 80 bytes).
#[test]
fn exhaustive_two_fragment_splits_match_reference() {
  let fixtures: &[&str] = &[
    "HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nping",
    "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nping\r\n0\r\n\r\n",
    "HTTP/1.0 200 OK\r\nContent-Length: 2\r\n\r\nok",
  ];
  for response in fixtures {
    assert!(
      response.len() < 80,
      "fixture too large for exhaustive split: {}",
      response.len()
    );
    for i in 1..response.len() {
      let socket = MockSocket::with_read_sizes(response.as_bytes(), &[i]);
      assert_matches_ref(response, socket);
    }
  }
}

/// Small three-fragment splits (i < j) for the smallest CL fixture.
#[test]
fn exhaustive_three_fragment_splits_match_reference() {
  let response = "HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nxyz";
  assert!(response.len() < 80);
  for i in 1..response.len() {
    for j in (i + 1)..response.len() {
      let socket = MockSocket::with_read_sizes(response.as_bytes(), &[i, j - i]);
      assert_matches_ref(response, socket);
    }
  }
}

#[test]
fn max_read_one_matches_unfragmented_for_cl_and_chunked() {
  for response in [
    "HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nHello",
    "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nHello\r\n0\r\n\r\n",
  ] {
    assert_matches_ref(response, MockSocket::with_max_read(response, 1));
  }
}
