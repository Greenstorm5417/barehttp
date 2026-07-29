//! Panic-freedom: untrusted bytes must not unwind through public parsers.
//!
//! Run under debug (default `cargo test`). Release builds may elide some checks;
//! these tests still assert `catch_unwind` success on the same corpus.

use barehttp::{Response, Uri};
use std::panic::{AssertUnwindSafe, catch_unwind};

fn must_not_panic(
  label: &str,
  f: impl FnOnce() + std::panic::UnwindSafe,
) {
  let r = catch_unwind(AssertUnwindSafe(f));
  assert!(r.is_ok(), "panicked on {label}");
}

fn response_corpus() -> Vec<Vec<u8>> {
  let mut v: Vec<Vec<u8>> = vec![
    b"".to_vec(),
    b"HTTP/1.1 200 OK\r\n\r\n".to_vec(),
    b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello".to_vec(),
    b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nHello\r\n0\r\n\r\n".to_vec(),
    b"HTTP/1.0 200 OK\r\nContent-Length: 0\r\n\r\n".to_vec(),
    b"HTTP/1.1 200 OK\r\nX-Fold: a\r\n b\r\n\r\n".to_vec(),
    b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Length: 1\r\n\r\n0\r\n\r\n".to_vec(),
    b"HTTP/1.1 200 OK\r\nContent-Length: 999999999999999\r\n\r\n".to_vec(),
    b"HTTP/1.1 200 OK\r\nContent-Length: 18446744073709551615\r\n\r\n".to_vec(),
    b"HTTP/1.1 200 OK\r\nHost: a\r\nHost: b\r\n\r\n".to_vec(),
    b"HTTP/1.1 200 OK\r\nX: val\rue\r\n\r\n".to_vec(),
    b"\r\n\r\n".to_vec(),
    b"GET / HTTP/1.1\r\n\r\n".to_vec(),
    vec![0u8; 256],
    vec![0xffu8; 128],
  ];
  // Adversarial: many short headers within a modest bound.
  let mut many = b"HTTP/1.1 200 OK\r\n".to_vec();
  for i in 0..64 {
    many.extend_from_slice(format!("X-{i}: v\r\n").as_bytes());
  }
  many.extend_from_slice(b"Content-Length: 0\r\n\r\n");
  v.push(many);
  v
}

fn uri_corpus() -> Vec<String> {
  vec![
    String::new(),
    String::from("http://example.com/"),
    String::from("https://[::1]/443/x"),
    String::from("http://EXAMPLE.COM:80/Path?q=1"),
    String::from("not-a-uri"),
    String::from("http://"),
    String::from("http:///path"),
    String::from("http://[gggg::1]/"),
    String::from("http://user:pass@host/x"),
    "http://example.com/".repeat(8),
  ]
}

#[test]
fn response_parse_never_panics_on_corpus() {
  for (i, msg) in response_corpus().into_iter().enumerate() {
    must_not_panic(&format!("response[{i}]"), || {
      let _ = Response::parse(&msg);
    });
  }
}

#[test]
fn uri_parse_never_panics_on_corpus() {
  for (i, s) in uri_corpus().into_iter().enumerate() {
    must_not_panic(&format!("uri[{i}]"), || {
      let _ = Uri::parse(&s);
    });
  }
}

#[test]
fn overflow_boundary_content_length_errors_not_panics() {
  // Advertised CL larger than available bytes: error path, no panic.
  let msg = b"HTTP/1.1 200 OK\r\nContent-Length: 999999999999999\r\n\r\n";
  must_not_panic("huge_cl", || {
    assert!(Response::parse(msg).is_err());
  });
  // Digit string that overflows usize on parse → InvalidContentLength (or err).
  let overflow = b"HTTP/1.1 200 OK\r\nContent-Length: 999999999999999999999999999\r\n\r\n";
  must_not_panic("overflow_cl_digits", || {
    assert!(Response::parse(overflow).is_err());
  });
}

#[cfg(feature = "gzip")]
#[test]
fn gzip_decompress_never_panics_on_junk() {
  use barehttp::gzip::{decompress_gzip, decompress_http_deflate, decompress_raw_deflate};
  const MAX: usize = 64 * 1024;
  let inputs: &[&[u8]] = &[b"", b"\x1f\x8b", b"\xff\xff\xff", &[0u8; 64], b"hello"];
  for (i, data) in inputs.iter().enumerate() {
    must_not_panic(&format!("gzip[{i}]"), || {
      let _ = decompress_gzip(data, MAX);
      let _ = decompress_http_deflate(data, MAX);
      let _ = decompress_raw_deflate(data, MAX);
    });
  }
}

#[test]
fn randomish_bytes_no_panic() {
  // Deterministic LCG stream; bounded for CI.
  let mut state = 0xDEAD_BEEFu64;
  for n in [0usize, 1, 7, 31, 127, 512] {
    let mut buf = vec![0u8; n];
    for b in &mut buf {
      state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
      *b = (state >> 33) as u8;
    }
    must_not_panic(&format!("rand_len_{n}"), || {
      let _ = Response::parse(&buf);
    });
  }
}
