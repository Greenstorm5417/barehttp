//! Local TCP mock for integration tests (no outbound network).

#[path = "support/mod.rs"]
mod support;

use barehttp::HttpClient;
use barehttp::config::Config;
use support::{host_from_request, read_request, spawn_server, write_all};

#[test]
fn get_200_plain() {
  let (addr, _jh) = spawn_server(8, |mut stream| {
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
  let (addr, _jh) = spawn_server(8, |mut stream| {
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
  let (addr, _jh) = spawn_server(8, |mut stream| {
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

  let (addr, _jh) = spawn_server(8, move |mut stream| {
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
  let (addr, _jh) = spawn_server(8, |mut stream| {
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
