#![no_main]

//! Structured HTTP response fuzzing via `arbitrary`.
//! Builds status / headers / framing from typed fields, then parses.

use arbitrary::{Arbitrary, Unstructured};
use barehttp::Response;
use libfuzzer_sys::fuzz_target;
use std::io::Write;

#[derive(Debug, Arbitrary)]
struct FuzzResponse {
  status: u16,
  /// Prefer common status band when mapping.
  use_http11: bool,
  content_length: Option<u16>,
  chunked: bool,
  header_count: u8,
  body_seed: u8,
  extra_header_bytes: [u8; 8],
}

fn build_message(f: &FuzzResponse) -> Vec<u8> {
  let status = if f.status == 0 {
    200
  } else {
    100 + (f.status % 500)
  };
  let version = if f.use_http11 { "HTTP/1.1" } else { "HTTP/1.0" };
  let mut out = format!("{version} {status} OK\r\n").into_bytes();

  let n = usize::from(f.header_count % 8);
  for i in 0..n {
    let b = f.extra_header_bytes[i % f.extra_header_bytes.len()];
    let _ = write!(out, "X-Fuzz-{i}: v{b:02x}\r\n");
  }

  if f.chunked && f.use_http11 {
    out.extend_from_slice(b"Transfer-Encoding: chunked\r\n\r\n");
    let chunk = [f.body_seed; 4];
    let _ = write!(out, "{:x}\r\n", chunk.len());
    out.extend_from_slice(&chunk);
    out.extend_from_slice(b"\r\n0\r\n\r\n");
  } else if let Some(cl) = f.content_length {
    let len = usize::from(cl % 64);
    let body = vec![f.body_seed; len];
    let _ = write!(out, "Content-Length: {len}\r\n\r\n");
    out.extend_from_slice(&body);
  } else {
    out.extend_from_slice(b"Content-Length: 0\r\n\r\n");
  }

  out
}

fuzz_target!(|data: &[u8]| {
  let mut u = Unstructured::new(data);
  if let Ok(f) = FuzzResponse::arbitrary(&mut u) {
    let msg = build_message(&f);
    let _ = Response::parse(&msg);
  }
  // Also feed residual / raw bytes.
  let _ = Response::parse(data);
});
