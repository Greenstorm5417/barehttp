//! Shared helpers for local TCP integration tests (no outbound network).
#![allow(dead_code)] // Each integration crate may use only a subset.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

/// Bind `127.0.0.1:0`, wait on a barrier so the listener is ready, then accept up to
/// `max_accepts` connections. Avoids long sleeps; a tiny settle sleep remains for the
/// listen backlog.
pub fn spawn_server(
  max_accepts: usize,
  handler: impl Fn(TcpStream) + Send + Sync + 'static,
) -> (SocketAddr, thread::JoinHandle<()>) {
  let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
  let addr = listener.local_addr().expect("local_addr");
  let handler = Arc::new(handler);
  let ready = Arc::new(Barrier::new(2));
  let ready2 = Arc::clone(&ready);
  let handle = thread::spawn(move || {
    ready2.wait();
    for _ in 0..max_accepts {
      match listener.accept() {
        Ok((stream, _)) => handler(stream),
        Err(_) => break,
      }
    }
  });
  ready.wait();
  // Brief settle so the accept loop is scheduled; Barrier already covers listen readiness.
  thread::sleep(Duration::from_millis(5));
  (addr, handle)
}

/// Read until headers complete (`\r\n\r\n`) or EOF / timeout.
pub fn read_request(stream: &mut TcpStream) -> Vec<u8> {
  let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
  let mut buf = vec![0u8; 8192];
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
  got
}

pub fn write_all(
  stream: &mut TcpStream,
  data: &[u8],
) {
  stream.write_all(data).expect("write");
  let _ = stream.flush();
}

pub fn host_from_request(req: &str) -> String {
  req
    .lines()
    .find(|l| l.to_ascii_lowercase().starts_with("host:"))
    .map(|l| l[5..].trim().to_string())
    .unwrap_or_else(|| "127.0.0.1".into())
}
