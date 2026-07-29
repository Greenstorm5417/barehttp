//! Criterion: end-to-end HTTP over a local mock server (no internet).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;

#[path = "support.rs"]
mod support;

fn e2e_get(c: &mut Criterion) {
  let server = support::LoopbackServer::spawn(support::RESP_PLAIN.to_vec());
  let url = server.url();
  let client = support::bench_client();

  let mut group = c.benchmark_group("e2e_loopback");
  group.warm_up_time(Duration::from_millis(500));
  group.measurement_time(Duration::from_secs(3));
  group.sample_size(40);

  group.bench_function("get_plain", |b| {
    b.iter(|| {
      let resp = client.get(black_box(url.as_str())).call().unwrap();
      black_box(resp.status_code());
      black_box(resp.body().len());
    });
  });

  group.finish();
  drop(server);
}

fn e2e_chunked(c: &mut Criterion) {
  let server = support::LoopbackServer::spawn(support::RESP_CHUNKED.to_vec());
  let url = server.url();
  let client = support::bench_client();

  let mut group = c.benchmark_group("e2e_loopback_chunked");
  group.warm_up_time(Duration::from_millis(500));
  group.measurement_time(Duration::from_secs(3));
  group.sample_size(40);

  group.bench_function("get_chunked", |b| {
    b.iter(|| {
      let resp = client.get(black_box(url.as_str())).call().unwrap();
      black_box(resp.body());
    });
  });

  group.finish();
  drop(server);
}

#[cfg(feature = "gzip")]
fn e2e_gzip(c: &mut Criterion) {
  use flate2::Compression;
  use flate2::write::GzEncoder;
  use std::io::Write;

  let plain = b"gzipped-e2e-benchmark-body-0123456789";
  let mut enc = GzEncoder::new(Vec::new(), Compression::default());
  enc.write_all(plain).unwrap();
  let gz = enc.finish().unwrap();
  let mut msg = format!(
    "HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
    gz.len()
  )
  .into_bytes();
  msg.extend_from_slice(&gz);

  let server = support::LoopbackServer::spawn(msg);
  let url = server.url();
  let client = support::bench_client();

  let mut group = c.benchmark_group("e2e_loopback_gzip");
  group.warm_up_time(Duration::from_millis(500));
  group.measurement_time(Duration::from_secs(3));
  group.sample_size(30);

  group.bench_function("get_gzip_body", |b| {
    b.iter(|| {
      let resp = client.get(black_box(url.as_str())).call().unwrap();
      black_box(resp.body());
    });
  });

  group.finish();
  drop(server);
}

#[cfg(not(feature = "gzip"))]
fn e2e_gzip(_c: &mut Criterion) {}

criterion_group!(benches, e2e_get, e2e_chunked, e2e_gzip);
criterion_main!(benches);
