//! Criterion: isolated hot paths (parse, URI, headers, optional gzip).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use barehttp::{Headers, Response, Uri};
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;

#[path = "support.rs"]
mod support;

fn parse_group(c: &mut Criterion) {
  let mut group = c.benchmark_group("parse_response");
  group.warm_up_time(Duration::from_millis(300));
  group.measurement_time(Duration::from_secs(2));

  group.throughput(Throughput::Bytes(support::RESP_PLAIN.len() as u64));
  group.bench_function("plain_small", |b| {
    b.iter(|| {
      let r = Response::parse(black_box(support::RESP_PLAIN)).unwrap();
      black_box(r.status_code());
    });
  });

  group.throughput(Throughput::Bytes(support::RESP_MANY_HEADERS.len() as u64));
  group.bench_function("many_headers", |b| {
    b.iter(|| {
      let r = Response::parse(black_box(support::RESP_MANY_HEADERS)).unwrap();
      black_box(r.header("x-request-id"));
    });
  });

  group.throughput(Throughput::Bytes(support::RESP_CHUNKED.len() as u64));
  group.bench_function("chunked", |b| {
    b.iter(|| {
      let r = Response::parse(black_box(support::RESP_CHUNKED)).unwrap();
      black_box(r.body());
    });
  });

  let resp_1k = support::resp_1k();
  group.throughput(Throughput::Bytes(resp_1k.len() as u64));
  group.bench_with_input(BenchmarkId::new("body_1k", resp_1k.len()), &resp_1k, |b, data| {
    b.iter(|| {
      let r = Response::parse(black_box(data.as_slice())).unwrap();
      black_box(r.body().len());
    });
  });

  group.finish();
}

fn uri_group(c: &mut Criterion) {
  let mut group = c.benchmark_group("parse_uri");
  group.warm_up_time(Duration::from_millis(200));
  group.measurement_time(Duration::from_secs(1));

  group.bench_function("simple", |b| {
    b.iter(|| black_box(Uri::parse(black_box(support::URI_SIMPLE)).unwrap()));
  });
  group.bench_function("ipv6", |b| {
    b.iter(|| black_box(Uri::parse(black_box(support::URI_IPV6)).unwrap()));
  });
  group.finish();
}

fn headers_group(c: &mut Criterion) {
  let headers = support::headers_lookup_fixture();
  let mut group = c.benchmark_group("headers");
  group.warm_up_time(Duration::from_millis(200));
  group.measurement_time(Duration::from_secs(1));

  group.bench_function("get_hit", |b| {
    b.iter(|| black_box(headers.get(black_box("Content-Type"))));
  });
  group.bench_function("get_miss", |b| {
    b.iter(|| black_box(headers.get(black_box("X-Missing"))));
  });
  group.bench_function("insert_set", |b| {
    b.iter(|| {
      let mut h = Headers::new();
      for i in 0..8 {
        h.insert(format!("H{i}"), "v");
      }
      h.set("Content-Type", "text/plain");
      black_box(h.get("content-type"));
    });
  });
  group.finish();
}

#[cfg(feature = "gzip")]
fn gzip_group(c: &mut Criterion) {
  use barehttp::gzip::{decompress_gzip, decompress_http_deflate};

  let mut group = c.benchmark_group("gzip");
  group.warm_up_time(Duration::from_millis(300));
  group.measurement_time(Duration::from_secs(2));

  group.throughput(Throughput::Bytes(support::GZIP_HELLO_WORLD.len() as u64));
  group.bench_function("decompress_hello", |b| {
    b.iter(|| {
      black_box(decompress_gzip(black_box(support::GZIP_HELLO_WORLD), 64).unwrap());
    });
  });

  group.throughput(Throughput::Bytes(support::GZIP_LONG.len() as u64));
  group.bench_function("decompress_long", |b| {
    b.iter(|| {
      black_box(decompress_gzip(black_box(support::GZIP_LONG), 4096).unwrap());
    });
  });

  group.bench_function("http_deflate_zlib", |b| {
    b.iter(|| {
      black_box(decompress_http_deflate(black_box(support::ZLIB_HELLO), 64).unwrap());
    });
  });

  group.finish();
}

#[cfg(not(feature = "gzip"))]
fn gzip_group(_c: &mut Criterion) {}

criterion_group!(benches, parse_group, uri_group, headers_group, gzip_group);
criterion_main!(benches);
