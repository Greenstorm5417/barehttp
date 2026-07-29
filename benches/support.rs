//! Shared fixtures for Criterion / Gungraun / dhat / perf wrappers.
#![allow(dead_code)]

use barehttp::Headers;
use barehttp::HttpClient;
use barehttp::config::Config;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// Small Content-Length response.
pub const RESP_PLAIN: &[u8] =
  b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nhello";

/// Many header fields (lookup / parse pressure).
pub const RESP_MANY_HEADERS: &[u8] = b"\
HTTP/1.1 200 OK\r\n\
Host: example.com\r\n\
User-Agent: barehttp-bench/1.0\r\n\
Accept: */*\r\n\
Accept-Encoding: gzip, deflate\r\n\
Accept-Language: en-US,en;q=0.9\r\n\
Cache-Control: no-cache\r\n\
Connection: close\r\n\
Content-Type: application/json\r\n\
X-Request-Id: 01234567-89ab-cdef-0123-456789abcdef\r\n\
X-Custom-A: value-a\r\n\
X-Custom-B: value-b\r\n\
X-Custom-C: value-c\r\n\
Content-Length: 2\r\n\
\r\n\
{}";

/// Chunked transfer-encoding body.
pub const RESP_CHUNKED: &[u8] = b"\
HTTP/1.1 200 OK\r\n\
Transfer-Encoding: chunked\r\n\
Connection: close\r\n\
\r\n\
5\r\nhello\r\n\
6\r\n world\r\n\
0\r\n\
\r\n";

/// ~1 KiB Content-Length body.
pub fn resp_1k() -> Vec<u8> {
  let body = vec![b'x'; 1024];
  let mut msg = format!(
    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
    body.len()
  )
  .into_bytes();
  msg.extend_from_slice(&body);
  msg
}

pub const URI_SIMPLE: &str = "http://example.com/path?q=1";
pub const URI_IPV6: &str = "http://[2001:db8::1]:8080/api/v1/items?limit=100";

/// Gzip member for payload "hello world" (from crate gzip tests).
#[cfg(feature = "gzip")]
pub const GZIP_HELLO_WORLD: &[u8] = &[
  0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0xff, 0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x57, 0x28, 0xcf, 0x2f,
  0xca, 0x49, 0x01, 0x00, 0x85, 0x11, 0x4a, 0x0d, 0x0b, 0x00, 0x00, 0x00,
];

/// Longer gzip fixture (~1.8 KiB uncompressed when inflated).
#[cfg(feature = "gzip")]
pub const GZIP_LONG: &[u8] = &[
  0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0xff, 0x0b, 0xc9, 0x48, 0x55, 0x28, 0x2c, 0xcd, 0x4c, 0xce,
  0x56, 0x48, 0x2a, 0xca, 0x2f, 0xcf, 0x53, 0x48, 0xcb, 0xaf, 0x50, 0xc8, 0x2a, 0xcd, 0x2d, 0x28, 0x56, 0xc8, 0x2f,
  0x4b, 0x2d, 0x52, 0x28, 0x01, 0x4a, 0xe7, 0x24, 0x56, 0x55, 0x2a, 0xa4, 0xe4, 0xa7, 0xeb, 0x29, 0x84, 0x8c, 0x2a,
  0x1e, 0x55, 0x3c, 0xaa, 0x78, 0x54, 0xf1, 0xa8, 0xe2, 0x51, 0xc5, 0xc3, 0x4b, 0x31, 0x00, 0xe6, 0xc3, 0x95, 0x64,
  0x08, 0x07, 0x00, 0x00,
];

#[cfg(feature = "gzip")]
pub const ZLIB_HELLO: &[u8] = &[
  0x78, 0xda, 0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x07, 0x00, 0x06, 0x2c, 0x02, 0x15,
];

pub fn headers_lookup_fixture() -> Headers {
  let mut h = Headers::new();
  for i in 0..32 {
    h.insert(format!("X-Header-{i}"), format!("value-{i}"));
  }
  h.insert("Content-Type", "application/json");
  h.insert("Content-Length", "42");
  h
}

fn read_request_headers(stream: &mut TcpStream) {
  let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
  let mut buf = [0u8; 4096];
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
}

/// Loopback HTTP server that serves a fixed response until dropped.
pub struct LoopbackServer {
  pub addr: SocketAddr,
  stop: Arc<AtomicBool>,
  join: Option<JoinHandle<()>>,
}

impl LoopbackServer {
  pub fn spawn(response: Vec<u8>) -> Self {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    listener.set_nonblocking(true).expect("nonblocking");
    let stop = Arc::new(AtomicBool::new(false));
    let stop_flag = Arc::clone(&stop);
    let join = thread::spawn(move || {
      while !stop_flag.load(Ordering::Relaxed) {
        match listener.accept() {
          Ok((mut stream, _)) => {
            let _ = stream.set_nodelay(true);
            read_request_headers(&mut stream);
            let _ = stream.write_all(&response);
            let _ = stream.flush();
          },
          Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
            thread::sleep(Duration::from_millis(1));
          },
          Err(_) => break,
        }
      }
    });
    // Brief settle so the first accept is ready.
    thread::sleep(Duration::from_millis(5));
    Self {
      addr,
      stop,
      join: Some(join),
    }
  }

  pub fn url(&self) -> String {
    format!("http://{}/", self.addr)
  }
}

impl Drop for LoopbackServer {
  fn drop(&mut self) {
    self.stop.store(true, Ordering::Relaxed);
    if let Some(handle) = self.join.take() {
      let _ = handle.join();
    }
  }
}

pub fn bench_client() -> HttpClient<barehttp::OsBlockingSocket, barehttp::OsDnsResolver> {
  HttpClient::with_config(
    Config::builder()
      .max_redirects(0)
      .max_idle_per_host(0)
      .timeout_connect(Some(Duration::from_secs(2)))
      .timeout_read(Some(Duration::from_secs(2)))
      .timeout_write(Some(Duration::from_secs(2)))
      .build(),
  )
}
