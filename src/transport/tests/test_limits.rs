//! Allocation and body-size limit tests.

use crate::error::Error;
use crate::transport::connection::Connection;
use crate::transport::pool::{ConnectionPool, PoolKey, PooledBuffers};
use crate::transport::tests::mock_socket::MockSocket;
use alloc::format;
use alloc::string::String;
use core::time::Duration;

#[test]
fn max_response_body_size_content_length() {
  let response = "HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\n";
  let mut socket = MockSocket::with_response(response);
  let mut conn = Connection::new(&mut socket, 8192, 64);
  let err = conn.read_raw_response(true).unwrap_err();
  assert!(matches!(err, Error::BodyExceedsLimit(64)));
}

#[test]
fn max_response_body_size_chunked() {
  // Declared chunk total exceeds limit before reading all payload.
  let response = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n64\r\n";
  let mut socket = MockSocket::with_response(response);
  let mut conn = Connection::new(&mut socket, 8192, 10);
  let err = conn.read_raw_response(true).unwrap_err();
  assert!(
    matches!(err, Error::BodyExceedsLimit(10) | Error::Parse(_) | Error::Socket(_)),
    "unexpected: {err:?}"
  );
}

#[test]
fn max_response_header_size() {
  let huge = format!("HTTP/1.1 200 OK\r\nX-Pad: {}\r\n\r\n", "Z".repeat(4096));
  let mut socket = MockSocket::with_response(&huge);
  let mut conn = Connection::new(&mut socket, 256, usize::MAX);
  let err = conn.read_raw_response(true).unwrap_err();
  assert!(matches!(err, Error::ResponseHeaderTooLarge));
}

#[test]
fn connection_pool_respects_max_idle_per_host() {
  let pool = ConnectionPool::<MockSocket>::new(1, Duration::from_mins(1));
  let key = PoolKey::new(String::from("http"), "example.com", 80);

  pool.return_connection(key.clone(), MockSocket::empty(), PooledBuffers::default());
  pool.return_connection(key.clone(), MockSocket::empty(), PooledBuffers::default());

  assert!(pool.get(&key).is_some());
  assert!(
    pool.get(&key).is_none(),
    "second idle socket must have been dropped when max_idle_per_host=1"
  );
}

#[test]
fn connection_pool_zero_disables_storage() {
  let pool = ConnectionPool::<MockSocket>::new(0, Duration::from_mins(1));
  let key = PoolKey::new(String::from("http"), "example.com", 80);
  pool.return_connection(key.clone(), MockSocket::empty(), PooledBuffers::default());
  assert!(pool.get(&key).is_none());
}
