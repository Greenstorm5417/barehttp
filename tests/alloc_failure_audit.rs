//! Allocation-failure / oversized Content-Length audit.
//!
//! Network-influenced reserves live in `src/transport/connection.rs`:
//! - Header buffer grows until `max_response_header_size` (then error).
//! - `ContentLength(len)` checks `len > max_body` **before** `buf.reserve(bytes_needed)`.
//!
//! `Vec::reserve` may abort the process on OOM (global alloc); we do not expose
//! a fallible allocator API. With a configured `max_response_body_size`, huge
//! advertised CL must fail fast with [`Error::BodyExceedsLimit`] without
//! attempting to reserve `len` bytes.

use barehttp::config::Config;
use barehttp::{Error, HttpClient, Response};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[test]
fn buffered_parse_huge_cl_does_not_allocate_body() {
  // Response::parse compares advertised CL to available bytes first; no Vec of `len`.
  let msg = b"HTTP/1.1 200 OK\r\nContent-Length: 999999999999999\r\n\r\n";
  assert!(Response::parse(msg).is_err());
}

#[test]
fn connection_rejects_cl_over_max_before_body_read() {
  // Serve headers only with a huge CL; client max_body is tiny.
  let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
  let addr = listener.local_addr().expect("addr");
  let seen = Arc::new(Mutex::new(Vec::new()));
  let seen2 = Arc::clone(&seen);
  thread::spawn(move || {
    if let Ok((mut stream, _)) = listener.accept() {
      let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
      let mut buf = [0u8; 2048];
      let mut got = Vec::new();
      loop {
        match stream.read(&mut buf) {
          Ok(0) => break,
          Ok(n) => {
            got.extend_from_slice(&buf[..n]);
            if got.windows(4).any(|w| w == b"\r\n\r\n") {
              break;
            }
          },
          Err(_) => break,
        }
      }
      *seen2.lock().unwrap() = got;
      let resp = b"HTTP/1.1 200 OK\r\nContent-Length: 104857600\r\nConnection: close\r\n\r\n";
      let _ = stream.write_all(resp);
    }
  });

  let client = HttpClient::with_config(
    Config::builder()
      .max_redirects(0)
      .max_idle_per_host(0)
      .max_response_body_size(1024)
      .timeout_connect(Some(Duration::from_secs(2)))
      .timeout_read(Some(Duration::from_secs(2)))
      .timeout_write(Some(Duration::from_secs(2)))
      .build(),
  );
  let url = format!("http://{addr}/");
  let err = client.get(&url).call().expect_err("must fail body limit");
  assert!(
    matches!(err, Error::BodyExceedsLimit(1024)),
    "expected BodyExceedsLimit before allocating 100MiB, got {err:?}"
  );
}
