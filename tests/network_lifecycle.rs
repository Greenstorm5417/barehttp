//! Network lifecycle integration tests against a local TCP mock (no public internet).

#[path = "support/mod.rs"]
mod support;

use barehttp::HttpClient;
use barehttp::config::Config;
use barehttp::Error;
#[cfg(feature = "gzip")]
use barehttp::ParseError;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;
use support::{read_request, spawn_server, write_all};

fn pooling_client(max_idle: usize) -> HttpClient<barehttp::OsBlockingSocket, barehttp::OsDnsResolver> {
  HttpClient::with_config(
    Config::builder()
      .max_redirects(0)
      .http_status_as_error(false)
      .max_idle_per_host(max_idle)
      .build(),
  )
}

#[test]
fn http10_without_keep_alive_not_reused() {
  let conns = Arc::new(AtomicUsize::new(0));
  let conns2 = Arc::clone(&conns);
  let (addr, _jh) = spawn_server(4, move |mut stream| {
    conns2.fetch_add(1, Ordering::SeqCst);
    let _ = read_request(&mut stream);
    write_all(&mut stream, b"HTTP/1.0 200 OK\r\nContent-Length: 2\r\n\r\nok");
  });

  let client = pooling_client(4);
  let url = format!("http://{addr}/");
  assert_eq!(client.get(&url).call().unwrap().body(), b"ok");
  assert_eq!(client.get(&url).call().unwrap().body(), b"ok");
  assert_eq!(
    conns.load(Ordering::SeqCst),
    2,
    "HTTP/1.0 without keep-alive must not pool"
  );
}

#[test]
fn http10_with_keep_alive_sequential_reuse() {
  let conns = Arc::new(AtomicUsize::new(0));
  let reqs = Arc::new(AtomicUsize::new(0));
  let conns2 = Arc::clone(&conns);
  let reqs2 = Arc::clone(&reqs);
  let (addr, _jh) = spawn_server(2, move |mut stream| {
    conns2.fetch_add(1, Ordering::SeqCst);
    loop {
      let req = read_request(&mut stream);
      if req.is_empty() {
        break;
      }
      reqs2.fetch_add(1, Ordering::SeqCst);
      write_all(
        &mut stream,
        b"HTTP/1.0 200 OK\r\nConnection: keep-alive\r\nContent-Length: 2\r\n\r\nok",
      );
    }
  });

  let client = pooling_client(4);
  let url = format!("http://{addr}/");
  assert_eq!(client.get(&url).call().unwrap().body(), b"ok");
  assert_eq!(client.get(&url).call().unwrap().body(), b"ok");
  assert_eq!(conns.load(Ordering::SeqCst), 1);
  assert_eq!(reqs.load(Ordering::SeqCst), 2);
}

#[test]
fn http11_default_persistent_reuse() {
  let conns = Arc::new(AtomicUsize::new(0));
  let reqs = Arc::new(AtomicUsize::new(0));
  let conns2 = Arc::clone(&conns);
  let reqs2 = Arc::clone(&reqs);
  let (addr, _jh) = spawn_server(2, move |mut stream| {
    conns2.fetch_add(1, Ordering::SeqCst);
    loop {
      let req = read_request(&mut stream);
      if req.is_empty() {
        break;
      }
      reqs2.fetch_add(1, Ordering::SeqCst);
      // No Connection header: HTTP/1.1 defaults to persistent.
      write_all(&mut stream, b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok");
    }
  });

  let client = pooling_client(4);
  let url = format!("http://{addr}/");
  assert_eq!(client.get(&url).call().unwrap().body(), b"ok");
  assert_eq!(client.get(&url).call().unwrap().body(), b"ok");
  assert_eq!(conns.load(Ordering::SeqCst), 1);
  assert_eq!(reqs.load(Ordering::SeqCst), 2);
}

#[test]
fn connection_close_response_not_reused() {
  let conns = Arc::new(AtomicUsize::new(0));
  let conns2 = Arc::clone(&conns);
  let (addr, _jh) = spawn_server(4, move |mut stream| {
    conns2.fetch_add(1, Ordering::SeqCst);
    let _ = read_request(&mut stream);
    write_all(
      &mut stream,
      b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
    );
  });

  let client = pooling_client(4);
  let url = format!("http://{addr}/");
  assert_eq!(client.get(&url).call().unwrap().body(), b"ok");
  assert_eq!(client.get(&url).call().unwrap().body(), b"ok");
  assert_eq!(conns.load(Ordering::SeqCst), 2);
}

#[test]
fn until_close_eof_delimited_body() {
  let (addr, _jh) = spawn_server(1, |mut stream| {
    let _ = read_request(&mut stream);
    // No Content-Length / Transfer-Encoding → UntilClose.
    write_all(&mut stream, b"HTTP/1.1 200 OK\r\n\r\nuntil-close-body");
    // Peer close ends the body.
  });

  let client = pooling_client(0);
  let resp = client.get(format!("http://{addr}/")).call().unwrap();
  assert_eq!(resp.body(), b"until-close-body");
}

#[test]
fn early_peer_close_mid_headers() {
  let (addr, _jh) = spawn_server(1, |mut stream| {
    let _ = read_request(&mut stream);
    write_all(&mut stream, b"HTTP/1.1 200 OK\r\nContent-Leng");
  });

  let client = pooling_client(0);
  let err = client.get(format!("http://{addr}/")).call().unwrap_err();
  assert!(
    matches!(err, Error::Socket(_) | Error::Parse(_) | Error::ResponseHeaderTooLarge),
    "got {err:?}"
  );
}

#[test]
fn early_peer_close_mid_body() {
  let (addr, _jh) = spawn_server(1, |mut stream| {
    let _ = read_request(&mut stream);
    write_all(
      &mut stream,
      b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\nshort",
    );
  });

  let client = pooling_client(0);
  let err = client.get(format!("http://{addr}/")).call().unwrap_err();
  assert!(matches!(err, Error::Socket(_)), "got {err:?}");
}

#[test]
fn extra_bytes_after_cl_body_not_pooled() {
  let conns = Arc::new(AtomicUsize::new(0));
  let conns2 = Arc::clone(&conns);
  let (addr, _jh) = spawn_server(4, move |mut stream| {
    conns2.fetch_add(1, Ordering::SeqCst);
    let _ = read_request(&mut stream);
    // Extra bytes after CL body → framing desync; must not pool.
    write_all(&mut stream, b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nokJUNK");
  });

  let client = pooling_client(4);
  let url = format!("http://{addr}/");
  assert_eq!(client.get(&url).call().unwrap().body(), b"ok");
  assert_eq!(client.get(&url).call().unwrap().body(), b"ok");
  assert_eq!(
    conns.load(Ordering::SeqCst),
    2,
    "extra trail bytes must prevent pooling"
  );
}

#[test]
fn stale_pool_idle_close_retries_once() {
  let conns = Arc::new(AtomicUsize::new(0));
  let conns2 = Arc::clone(&conns);
  let (addr, _jh) = spawn_server(4, move |mut stream| {
    let n = conns2.fetch_add(1, Ordering::SeqCst);
    let _ = read_request(&mut stream);
    write_all(&mut stream, b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok");
    if n == 0 {
      // Close while client may idle-pool this socket.
      let _ = stream.shutdown(std::net::Shutdown::Both);
    }
  });

  let client = pooling_client(4);
  let url = format!("http://{addr}/");
  assert_eq!(client.get(&url).call().unwrap().body(), b"ok");
  // Second request reuses stale socket → Socket error → one retry on fresh connect.
  assert_eq!(client.get(&url).call().unwrap().body(), b"ok");
  assert!(
    conns.load(Ordering::SeqCst) >= 2,
    "stale idle close should force a new connect (got {})",
    conns.load(Ordering::SeqCst)
  );
}

#[test]
fn max_idle_per_host_zero_disables_pooling() {
  let conns = Arc::new(AtomicUsize::new(0));
  let conns2 = Arc::clone(&conns);
  let (addr, _jh) = spawn_server(4, move |mut stream| {
    conns2.fetch_add(1, Ordering::SeqCst);
    let _ = read_request(&mut stream);
    // Server offers keep-alive; client with max_idle=0 sends Connection: close and does not pool.
    write_all(&mut stream, b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok");
  });

  let client = pooling_client(0);
  let url = format!("http://{addr}/");
  let r1 = client.get(&url).call().unwrap();
  let r2 = client.get(&url).call().unwrap();
  assert_eq!(r1.body(), b"ok");
  assert_eq!(r2.body(), b"ok");
  assert_eq!(conns.load(Ordering::SeqCst), 2);
}

// max_idle_age: requires waiting for wall-clock expiry → flaky under load; covered by
// ConnectionPool unit tests with injected Instant. Skipped here intentionally.

#[test]
fn sequential_reuse_two_gets_same_tcp() {
  let conns = Arc::new(AtomicUsize::new(0));
  let reqs = Arc::new(AtomicUsize::new(0));
  let conns2 = Arc::clone(&conns);
  let reqs2 = Arc::clone(&reqs);
  let (addr, _jh) = spawn_server(2, move |mut stream| {
    conns2.fetch_add(1, Ordering::SeqCst);
    loop {
      let req = read_request(&mut stream);
      if req.is_empty() {
        break;
      }
      reqs2.fetch_add(1, Ordering::SeqCst);
      write_all(&mut stream, b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\n\r\nZ");
    }
  });

  let client = pooling_client(2);
  let url = format!("http://{addr}/");
  assert_eq!(client.get(&url).call().unwrap().body(), b"Z");
  assert_eq!(client.get(&url).call().unwrap().body(), b"Z");
  assert_eq!(conns.load(Ordering::SeqCst), 1);
  assert_eq!(reqs.load(Ordering::SeqCst), 2);
}

#[test]
fn parse_failure_not_returned_to_pool() {
  let conns = Arc::new(AtomicUsize::new(0));
  let phase = Arc::new(AtomicUsize::new(0));
  let conns2 = Arc::clone(&conns);
  let phase2 = Arc::clone(&phase);
  let (addr, _jh) = spawn_server(4, move |mut stream| {
    conns2.fetch_add(1, Ordering::SeqCst);
    let _ = read_request(&mut stream);
    if phase2.fetch_add(1, Ordering::SeqCst) == 0 {
      write_all(&mut stream, b"NOTHTTP garbage\r\n\r\n");
    } else {
      write_all(&mut stream, b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok");
    }
  });

  let client = pooling_client(4);
  let url = format!("http://{addr}/");
  let err = client.get(&url).call().unwrap_err();
  assert!(matches!(err, Error::Parse(_)), "got {err:?}");
  assert_eq!(client.get(&url).call().unwrap().body(), b"ok");
  assert_eq!(conns.load(Ordering::SeqCst), 2);
}

#[cfg(feature = "gzip")]
#[test]
fn decompress_failure_not_returned_to_pool() {
  let conns = Arc::new(AtomicUsize::new(0));
  let phase = Arc::new(AtomicUsize::new(0));
  let conns2 = Arc::clone(&conns);
  let phase2 = Arc::clone(&phase);
  let (addr, _jh) = spawn_server(4, move |mut stream| {
    conns2.fetch_add(1, Ordering::SeqCst);
    let _ = read_request(&mut stream);
    if phase2.fetch_add(1, Ordering::SeqCst) == 0 {
      write_all(
        &mut stream,
        b"HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: 4\r\n\r\nbad!",
      );
    } else {
      write_all(&mut stream, b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok");
    }
  });

  let client = pooling_client(4);
  let url = format!("http://{addr}/");
  let err = client.get(&url).call().unwrap_err();
  assert!(matches!(err, Error::Parse(ParseError::Decompression(_))), "got {err:?}");
  assert_eq!(client.get(&url).call().unwrap().body(), b"ok");
  assert_eq!(conns.load(Ordering::SeqCst), 2);
}

#[test]
fn body_limit_max_response_body_size() {
  let (addr, _jh) = spawn_server(1, |mut stream| {
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
      .http_status_as_error(false)
      .build(),
  );
  let err = client.get(format!("http://{addr}/")).call().unwrap_err();
  assert!(matches!(err, Error::BodyExceedsLimit(16)), "got {err:?}");
}

#[test]
fn read_timeout_fires_without_flaky_sleep() {
  // Server accepts and never writes; client uses a short read timeout.
  // Prefer OS SO_RCVTIMEO over thread::sleep barriers.
  let (addr, _jh) = spawn_server(1, |mut stream| {
    let _ = read_request(&mut stream);
    // Hold the connection open until the client times out / drops.
    thread::sleep(Duration::from_secs(30));
    let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
  });

  let client = HttpClient::with_config(
    Config::builder()
      .timeout_read(Some(Duration::from_millis(80)))
      .max_idle_per_host(0)
      .http_status_as_error(false)
      .build(),
  );
  let err = client.get(format!("http://{addr}/")).call().unwrap_err();
  assert!(
    matches!(err, Error::Socket(_)),
    "expected read timeout as Socket error, got {err:?}"
  );
}
