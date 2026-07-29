//! Local TCP mock for integration tests (no outbound network).

use barehttp::HttpClient;
use barehttp::config::Config;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

fn spawn_server(handler: impl Fn(TcpStream) + Send + Sync + 'static) -> (SocketAddr, thread::JoinHandle<()>) {
  let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
  let addr = listener.local_addr().expect("local_addr");
  let handler = Arc::new(handler);
  let ready = Arc::new(Barrier::new(2));
  let ready2 = Arc::clone(&ready);
  let handle = thread::spawn(move || {
    ready2.wait();
    for _ in 0..8 {
      match listener.accept() {
        Ok((stream, _)) => handler(stream),
        Err(_) => break,
      }
    }
  });
  ready.wait();
  thread::sleep(Duration::from_millis(10));
  (addr, handle)
}

fn read_request(stream: &mut TcpStream) -> Vec<u8> {
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

fn write_all(
  stream: &mut TcpStream,
  data: &[u8],
) {
  stream.write_all(data).expect("write");
  let _ = stream.flush();
}

fn host_from_request(req: &str) -> String {
  req
    .lines()
    .find(|l| l.to_ascii_lowercase().starts_with("host:"))
    .map(|l| l[5..].trim().to_string())
    .unwrap_or_else(|| "127.0.0.1".into())
}

#[test]
fn get_200_plain() {
  let (addr, _jh) = spawn_server(|mut stream| {
    let _ = read_request(&mut stream);
    write_all(
      &mut stream,
      b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello",
    );
  });

  let client = HttpClient::with_config(
    Config::builder()
      .max_redirects(0)
      .max_idle_per_host(0)
      .build(),
  );
  let url = format!("http://{addr}/");
  let resp = client.get(&url).call().expect("call");
  assert_eq!(resp.status_code(), 200);
  assert_eq!(resp.body(), b"hello");
}

#[test]
fn chunked_body() {
  let (addr, _jh) = spawn_server(|mut stream| {
    let _ = read_request(&mut stream);
    write_all(
      &mut stream,
      b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n5\r\nhello\r\n0\r\n\r\n",
    );
  });

  let client = HttpClient::with_config(Config::builder().max_idle_per_host(0).build());
  let resp = client.get(format!("http://{addr}/")).call().unwrap();
  assert_eq!(resp.body(), b"hello");
}

#[test]
fn redirect_follow() {
  let (addr, _jh) = spawn_server(|mut stream| {
    let req = String::from_utf8_lossy(&read_request(&mut stream)).into_owned();
    if req.contains("GET /start") {
      let host = host_from_request(&req);
      let body =
        format!("HTTP/1.1 302 Found\r\nLocation: http://{host}/done\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
      write_all(&mut stream, body.as_bytes());
    } else {
      write_all(
        &mut stream,
        b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\ndone",
      );
    }
  });

  let client = HttpClient::with_config(
    Config::builder()
      .max_redirects(5)
      .max_idle_per_host(0)
      .build(),
  );
  let resp = client.get(format!("http://{addr}/start")).call().unwrap();
  assert_eq!(resp.status_code(), 200);
  assert_eq!(resp.body(), b"done");
}

#[cfg(feature = "gzip")]
#[test]
fn gzip_content_encoding() {
  use flate2::Compression;
  use flate2::write::GzEncoder;
  use std::io::Write;

  let plain = b"gzipped-integration-body";
  let mut enc = GzEncoder::new(Vec::new(), Compression::default());
  enc.write_all(plain).unwrap();
  let gz = enc.finish().unwrap();

  let (addr, _jh) = spawn_server(move |mut stream| {
    let _ = read_request(&mut stream);
    let mut msg = format!(
      "HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
      gz.len()
    )
    .into_bytes();
    msg.extend_from_slice(&gz);
    write_all(&mut stream, &msg);
  });

  let client = HttpClient::with_config(Config::builder().max_idle_per_host(0).build());
  let resp = client.get(format!("http://{addr}/")).call().unwrap();
  assert_eq!(resp.body(), plain);
}

#[test]
fn body_limit_returns_error() {
  let (addr, _jh) = spawn_server(|mut stream| {
    let _ = read_request(&mut stream);
    write_all(
      &mut stream,
      b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n",
    );
  });

  let client = HttpClient::with_config(
    Config::builder()
      .max_response_body_size(16)
      .max_idle_per_host(0)
      .build(),
  );
  let err = client.get(format!("http://{addr}/")).call().unwrap_err();
  assert!(matches!(err, barehttp::Error::BodyExceedsLimit(16)), "got {err:?}");
}
