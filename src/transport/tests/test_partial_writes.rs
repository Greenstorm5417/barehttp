//! Partial-write coverage for `Connection::send_request`.
//!
//! `Connection` retries short writes (`Ok(n > 0)`). `Interrupted` is left to OS
//! socket adapters. Client-level `Error::Socket` retries run only on a reused
//! pooled socket, and only by starting a fresh hop.

use crate::error::{Error, SocketError};
use crate::transport::connection::Connection;
use crate::transport::tests::scripted_socket::{ReadStep, RetryInterrupted, ScriptedSocket, WriteStep};
use alloc::vec::Vec;

const REQUEST: &[u8] = b"GET /path HTTP/1.1\r\nHost: example.com\r\nContent-Length: 4\r\n\r\nBODY";

fn success_response() -> Vec<u8> {
  b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n".to_vec()
}

#[test]
fn short_writes_split_across_request_line_headers_body() {
  // Force cuts inside request-line, headers, and body.
  let mut socket = ScriptedSocket::new();
  socket
    .push_writes([
      WriteStep::Accept(3),  // "GET"
      WriteStep::Accept(8),  // " /path H"
      WriteStep::Accept(1),  // one byte into "TTP/..."
      WriteStep::Accept(20), // more headers
      WriteStep::Accept(5),  // short into headers/body boundary region
      WriteStep::AcceptAll,  // rest
      WriteStep::AcceptAll,
      WriteStep::AcceptAll,
      WriteStep::AcceptAll,
      WriteStep::AcceptAll,
    ])
    .push_read(ReadStep::Data(success_response()));

  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);
  assert!(conn.send_request(REQUEST).is_ok());
  assert_eq!(socket.get_written(), REQUEST);
}

#[test]
fn one_byte_writes_send_full_request() {
  let mut socket = ScriptedSocket::new();
  for _ in 0..REQUEST.len() {
    socket.push_write(WriteStep::Accept(1));
  }
  socket.push_read(ReadStep::Data(success_response()));

  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);
  assert!(conn.send_request(REQUEST).is_ok());
  assert_eq!(socket.get_written(), REQUEST);
  assert_eq!(socket.write_calls, REQUEST.len());
}

#[test]
fn interrupted_mid_write_returns_socket_interrupted() {
  // Any write Err, including Interrupted, fails the send immediately.
  let mut socket = ScriptedSocket::new();
  socket.push_writes([
    WriteStep::Accept(10),
    WriteStep::Interrupted,
    WriteStep::AcceptAll, // unused: send must fail on Interrupted before this step
  ]);

  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);
  let err = conn.send_request(REQUEST).unwrap_err();
  assert_eq!(err, Error::Socket(SocketError::Interrupted));
  assert_eq!(socket.written_len(), 10);
  assert_eq!(socket.write_calls, 2);
}

#[test]
fn zero_byte_write_maps_to_not_connected() {
  let mut socket = ScriptedSocket::new();
  socket.push_writes([WriteStep::Accept(4), WriteStep::Zero]);

  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);
  let err = conn.send_request(REQUEST).unwrap_err();
  assert_eq!(err, Error::Socket(SocketError::NotConnected));
  assert_eq!(socket.written_len(), 4);
}

#[test]
fn failure_mid_request_after_partial_accept() {
  let mut socket = ScriptedSocket::new();
  socket.push_writes([
    WriteStep::Accept(15),
    WriteStep::Error(SocketError::OsError(104)),
  ]);

  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);
  let err = conn.send_request(REQUEST).unwrap_err();
  assert_eq!(err, Error::Socket(SocketError::OsError(104)));
  assert_eq!(socket.written_len(), 15);
  assert_eq!(socket.get_written(), &REQUEST[..15]);
}

#[test]
fn connection_refused_mid_write() {
  let mut socket = ScriptedSocket::new();
  socket.push_writes([
    WriteStep::Accept(1),
    WriteStep::Error(SocketError::ConnectionRefused),
  ]);

  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);
  let err = conn.send_request(REQUEST).unwrap_err();
  assert_eq!(err, Error::Socket(SocketError::ConnectionRefused));
  assert_eq!(socket.written_len(), 1);
}

#[test]
fn timed_out_mid_write() {
  let mut socket = ScriptedSocket::new();
  socket.push_writes([
    WriteStep::Accept(8),
    WriteStep::Error(SocketError::TimedOut),
  ]);

  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);
  let err = conn.send_request(REQUEST).unwrap_err();
  assert_eq!(err, Error::Socket(SocketError::TimedOut));
  assert_eq!(socket.written_len(), 8);
}

#[test]
fn not_connected_mid_write() {
  let mut socket = ScriptedSocket::new();
  socket.push_writes([
    WriteStep::Accept(2),
    WriteStep::Error(SocketError::NotConnected),
  ]);

  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);
  let err = conn.send_request(REQUEST).unwrap_err();
  assert_eq!(err, Error::Socket(SocketError::NotConnected));
  assert_eq!(socket.written_len(), 2);
}

#[test]
fn retry_interrupted_adapter_absorbs_eintr_like_os() {
  // Models OS Unix/Windows adapters that loop on Interrupted before Connection sees it.
  let mut inner = ScriptedSocket::new();
  inner.push_writes([
    WriteStep::Accept(5),
    WriteStep::Interrupted,
    WriteStep::Interrupted,
    WriteStep::AcceptAll,
    WriteStep::AcceptAll,
    WriteStep::AcceptAll,
  ]);
  let mut socket = RetryInterrupted::new(inner);

  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);
  assert!(conn.send_request(REQUEST).is_ok());
  assert_eq!(socket.interrupted_retries, 2);
  assert_eq!(socket.inner().get_written(), REQUEST);
}

#[test]
fn accept_all_unscripted_writes_succeed() {
  // Empty write queue → AcceptAll semantics.
  let mut socket = ScriptedSocket::new();
  socket.push_read(ReadStep::Data(success_response()));

  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);
  assert!(conn.send_request(REQUEST).is_ok());
  assert_eq!(socket.get_written(), REQUEST);
  assert_eq!(socket.write_calls, 1);
}

#[test]
fn short_write_of_zero_cap_is_not_connected() {
  // Accept(0) is a zero-byte write → Connection maps to NotConnected.
  let mut socket = ScriptedSocket::new();
  socket.push_write(WriteStep::Accept(0));

  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);
  let err = conn.send_request(REQUEST).unwrap_err();
  assert_eq!(err, Error::Socket(SocketError::NotConnected));
}

#[test]
fn send_then_read_after_fragmented_writes() {
  let mut socket = ScriptedSocket::new();
  for _ in 0..8 {
    socket.push_write(WriteStep::Accept(7));
  }
  socket.push_write(WriteStep::AcceptAll);
  socket.push_read(ReadStep::Data(success_response()));

  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);
  conn.send_request(REQUEST).unwrap();
  let raw = conn.read_raw_response(true).unwrap();
  assert_eq!(raw.status_code, 200);
  assert!(raw.body_bytes.is_empty());
}

/// `HttpClient` retries `Error::Socket` only after a failed hop on a reused pooled
/// socket, then reconnects. Mid-write `Interrupted` on a newly connected socket
/// propagates to the caller.
#[test]
fn document_no_mid_write_client_retry_on_fresh_connect() {
  let mut socket = ScriptedSocket::new();
  socket.push_writes([WriteStep::Accept(3), WriteStep::Interrupted]);

  let mut conn = Connection::new(&mut socket, 8192, usize::MAX);
  let err = conn.send_request(REQUEST).unwrap_err();
  assert_eq!(err, Error::Socket(SocketError::Interrupted));
  assert_eq!(socket.written_len(), 3);
}
