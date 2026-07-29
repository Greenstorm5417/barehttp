//! Criterion: adversarial / pathological inputs (short samples for CI).
//!
//! Env:
//! - `BAREHTTP_ADV_SAMPLE_SIZE`: Criterion sample size (default 10)
//! - `BAREHTTP_ADV_MEASURE_SECS`: measurement seconds (default 0.5)
#![allow(clippy::unwrap_used, clippy::expect_used)]

use barehttp::Response;
use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use std::env;
use std::hint::black_box;
use std::time::Duration;

fn sample_size() -> usize {
  env::var("BAREHTTP_ADV_SAMPLE_SIZE")
    .ok()
    .and_then(|s| s.parse().ok())
    .unwrap_or(10)
    .clamp(10, 100)
}

fn measure_secs() -> f64 {
  env::var("BAREHTTP_ADV_MEASURE_SECS")
    .ok()
    .and_then(|s| s.parse().ok())
    .unwrap_or(0.5_f64)
    .clamp(0.2_f64, 5.0_f64)
}

fn many_small_headers() -> Vec<u8> {
  let mut msg = b"HTTP/1.1 200 OK\r\n".to_vec();
  for i in 0..128 {
    msg.extend_from_slice(format!("X-H-{i}: v\r\n").as_bytes());
  }
  msg.extend_from_slice(b"Content-Length: 0\r\n\r\n");
  msg
}

fn pathological_chunked() -> Vec<u8> {
  // Many tiny chunks within a modest total size.
  let mut msg = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec();
  for _ in 0..64 {
    msg.extend_from_slice(b"1\r\nx\r\n");
  }
  msg.extend_from_slice(b"0\r\n\r\n");
  msg
}

fn medium_body_cl() -> Vec<u8> {
  let body = vec![b'y'; 4096];
  let mut msg = format!(
    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
    body.len()
  )
  .into_bytes();
  msg.extend_from_slice(&body);
  msg
}

/// Accumulate `data` one byte at a time, then `Response::parse` the full buffer.
/// Measures parse cost of medium bodies; socket-level `max_read=1` lives in
/// transport unit tests.
fn fragmented_accumulate_then_parse(data: &[u8]) {
  let mut acc = Vec::with_capacity(data.len());
  for &b in data {
    acc.push(b);
  }
  let r = Response::parse(black_box(acc.as_slice())).unwrap();
  black_box(r.body().len());
}

fn adversarial_group(c: &mut Criterion) {
  let mut group = c.benchmark_group("adversarial");
  group.sample_size(sample_size());
  group.warm_up_time(Duration::from_millis(200));
  group.measurement_time(Duration::from_secs_f64(measure_secs()));

  let many = many_small_headers();
  group.throughput(Throughput::Bytes(many.len() as u64));
  group.bench_function("many_small_headers", |b| {
    b.iter(|| {
      let r = Response::parse(black_box(many.as_slice())).unwrap();
      black_box(r.headers().len());
    });
  });

  let chunked = pathological_chunked();
  group.throughput(Throughput::Bytes(chunked.len() as u64));
  group.bench_function("pathological_chunked_tiny", |b| {
    b.iter(|| {
      let r = Response::parse(black_box(chunked.as_slice())).unwrap();
      black_box(r.body().len());
    });
  });

  let medium = medium_body_cl();
  group.throughput(Throughput::Bytes(medium.len() as u64));
  group.bench_function("medium_body_byte_accumulate", |b| {
    b.iter(|| fragmented_accumulate_then_parse(black_box(medium.as_slice())));
  });

  group.finish();
}

criterion_group!(benches, adversarial_group);
criterion_main!(benches);
