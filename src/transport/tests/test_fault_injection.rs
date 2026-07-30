//! Fault-injection tests against `Connection` via [`ScriptedSocket`].
//!
//! Short, zero, and error reads; EOF mid Content-Length; `TimedOut`. An incomplete
//! body yields an error; success never carries a truncated payload.

use crate::error::{Error, SocketError};
use crate::transport::connection::Connection;
use crate::transport::tests::scripted_socket::{ReadStep, ScriptedSocket, WriteStep};
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;

fn headers_and_body(
  cl: usize,
  body: &[u8],
) -> Vec<u8> {
  let mut v = Vec::new();
  v.extend_from_slice(b"HTTP/1.1 200 OK\r\nContent-Length: ");
  v.extend_from_slice(cl.to_string().as_bytes());
  v.extend_from_slice(b"\r\n\r\n");
  v.extend_from_slice(body);
  v
}

#[test]
fn eof_mid_content_length_fails_not_short_ok() {
  // Headers + 3 of 10 promised body bytes, then EOF.
  let partial = b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\nhel";
  let mut socket = ScriptedSocket::new();
  socket
    .push_read(ReadStep::Data(partial.to_vec()))
    .push_read(ReadStep::Eof);

  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);
  let err = conn.read_raw_response(true).unwrap_err();
  assert_eq!(err, Error::Socket(SocketError::NotConnected));
  // Must not return Ok with a truncated body (silent truncation).
}

#[test]
fn partial_body_then_eof_via_zero_then_data_still_fails() {
  // First body read returns Ok(0) before remaining bytes; Connection treats that as EOF.
  let headers = b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\n".to_vec();
  let mut socket = ScriptedSocket::new();
  socket
    .push_read(ReadStep::Data(headers))
    .push_read(ReadStep::ZeroThenData(b"helloworld".to_vec()));

  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);
  let err = conn.read_raw_response(true).unwrap_err();
  assert_eq!(err, Error::Socket(SocketError::NotConnected));
}

#[test]
fn short_reads_still_assemble_full_content_length() {
  let body = b"HelloWorld";
  let full = headers_and_body(10, body);
  let mut socket = ScriptedSocket::new().with_max_io_calls(500);
  for byte in full {
    socket.push_read(ReadStep::Data(vec![byte]));
  }

  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);
  let raw = conn.read_raw_response(true).unwrap();
  assert_eq!(&raw.body_bytes[..], b"HelloWorld");
  assert!(conn.is_reusable());
}

#[test]
fn timed_out_on_read_fails_with_socket_timed_out() {
  let headers = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\n".to_vec();
  let mut socket = ScriptedSocket::new();
  socket
    .push_read(ReadStep::Data(headers))
    .push_read(ReadStep::TimedOut);

  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);
  let err = conn.read_raw_response(true).unwrap_err();
  assert_eq!(err, Error::Socket(SocketError::TimedOut));
}

#[test]
fn interrupted_on_read_fails_immediately_no_retry() {
  // Interrupted on read fails the hop at Connection; remaining read steps stay queued.
  let headers = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\n".to_vec();
  let mut socket = ScriptedSocket::new();
  socket
    .push_read(ReadStep::Data(headers))
    .push_read(ReadStep::Interrupted)
    .push_read(ReadStep::Data(b"Hello".to_vec()));

  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);
  let err = conn.read_raw_response(true).unwrap_err();
  assert_eq!(err, Error::Socket(SocketError::Interrupted));
  // Remaining Data step still queued (Connection stopped after Interrupted).
  assert_eq!(socket.read_calls, 2);
}

#[test]
fn os_error_on_read_propagates() {
  let mut socket = ScriptedSocket::new();
  socket.push_read(ReadStep::Error(SocketError::OsError(5)));

  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);
  let err = conn.read_raw_response(true).unwrap_err();
  assert_eq!(err, Error::Socket(SocketError::OsError(5)));
}

#[test]
fn connection_refused_style_error_on_read() {
  let mut socket = ScriptedSocket::new();
  socket.push_read(ReadStep::Error(SocketError::ConnectionRefused));

  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);
  let err = conn.read_raw_response(true).unwrap_err();
  assert_eq!(err, Error::Socket(SocketError::ConnectionRefused));
}

#[test]
fn not_connected_on_read_propagates() {
  let mut socket = ScriptedSocket::new();
  socket.push_read(ReadStep::Error(SocketError::NotConnected));

  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);
  let err = conn.read_raw_response(true).unwrap_err();
  assert_eq!(err, Error::Socket(SocketError::NotConnected));
}

#[test]
fn fatal_read_error_caller_must_discard_connection() {
  // After many socket errors `reusable` stays true; the client discards the
  // socket on Err and skips the idle pool.
  let mut socket = ScriptedSocket::new();
  socket
    .push_read(ReadStep::Data(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\n".to_vec()))
    .push_read(ReadStep::Error(SocketError::OsError(104)));

  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);
  let err = conn.read_raw_response(true).unwrap_err();
  assert!(matches!(err, Error::Socket(SocketError::OsError(104))));
  // I/O Err leaves reusable uncleared; pooling policy is the caller's job.
  assert!(conn.is_reusable());
}

#[test]
fn eof_mid_headers_does_not_panic() {
  let mut socket = ScriptedSocket::new();
  socket
    .push_read(ReadStep::Data(b"HTTP/1.1 200 OK\r\nConten".to_vec()))
    .push_read(ReadStep::Eof);

  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);
  let err = conn.read_raw_response(true).unwrap_err();
  assert!(
    matches!(err, Error::Socket(_) | Error::Parse(_)),
    "unexpected error: {err:?}"
  );
}

#[test]
fn write_failure_mid_request_reports_bytes_accepted() {
  let request = b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n";
  let mut socket = ScriptedSocket::new();
  socket
    .push_write(WriteStep::Accept(10))
    .push_write(WriteStep::Error(SocketError::OsError(32)));

  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);
  let err = conn.send_request(request, &[]).unwrap_err();
  assert_eq!(err, Error::Socket(SocketError::OsError(32)));
  assert_eq!(socket.written_len(), 10);
  assert_eq!(socket.get_written(), &request[..10]);
}

#[test]
fn zero_byte_write_maps_to_not_connected() {
  let request = b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n";
  let mut socket = ScriptedSocket::new();
  socket.push_write(WriteStep::Zero);

  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);
  let err = conn.send_request(request, &[]).unwrap_err();
  assert_eq!(err, Error::Socket(SocketError::NotConnected));
  assert_eq!(socket.written_len(), 0);
}

#[test]
fn interrupted_on_write_no_connection_retry() {
  let request = b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n";
  let mut socket = ScriptedSocket::new();
  socket
    .push_write(WriteStep::Accept(5))
    .push_write(WriteStep::Interrupted)
    .push_write(WriteStep::AcceptAll);

  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);
  let err = conn.send_request(request, &[]).unwrap_err();
  assert_eq!(err, Error::Socket(SocketError::Interrupted));
  assert_eq!(socket.written_len(), 5);
  assert_eq!(socket.write_calls, 2, "must not retry Interrupted at Connection layer");
}

#[test]
fn max_io_calls_guards_against_infinite_loop() {
  // Empty read queue → Ok(0); Connection must stop (not spin forever).
  // ScriptedSocket panics if max_io_calls is exceeded.
  let mut socket = ScriptedSocket::new().with_max_io_calls(50);

  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);
  let err = conn.read_raw_response(true).unwrap_err();
  assert!(
    matches!(err, Error::Socket(_) | Error::Parse(_)),
    "unexpected error: {err:?}"
  );
  assert!(
    socket.read_calls <= 50,
    "too many reads ({}); possible infinite loop",
    socket.read_calls
  );
}

#[test]
fn data_step_splits_across_buffer_sized_reads() {
  let body = b"ABCDEFGHIJ";
  let full = headers_and_body(10, body);
  let mut socket = ScriptedSocket::new();
  socket.push_read(ReadStep::Data(full));

  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);
  let raw = conn.read_raw_response(true).unwrap();
  assert_eq!(&raw.body_bytes[..], body);
}
