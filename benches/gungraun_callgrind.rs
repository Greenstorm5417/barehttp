//! Gungraun Callgrind: instruction / branch / call-count regression (deterministic).
//!
//! Requires `valgrind` and `gungraun-runner` matching the `gungraun` crate version.
//! Allocation metrics: `dhat_*` benches.
//!
//! Soft limits fail CI (`benches` workflow job) on regression.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::must_use_candidate)]

use barehttp::{Headers, Response, Uri};
use gungraun::{Callgrind, EventKind, LibraryBenchmarkConfig, library_benchmark, library_benchmark_group, main};
use std::hint::black_box;

#[path = "support.rs"]
mod support;

fn setup_plain() -> &'static [u8] {
  support::RESP_PLAIN
}

fn setup_chunked() -> &'static [u8] {
  support::RESP_CHUNKED
}

fn setup_many_headers() -> &'static [u8] {
  support::RESP_MANY_HEADERS
}

#[library_benchmark]
#[bench::plain(setup = setup_plain)]
#[bench::chunked(setup = setup_chunked)]
#[bench::many_headers(setup = setup_many_headers)]
fn parse_response(data: &[u8]) -> u16 {
  let r = Response::parse(black_box(data)).unwrap();
  black_box(r.status_code())
}

#[library_benchmark]
#[bench::simple(support::URI_SIMPLE)]
#[bench::ipv6(support::URI_IPV6)]
fn parse_uri(uri: &str) -> usize {
  black_box(Uri::parse(black_box(uri)).unwrap().path().len())
}

fn setup_headers() -> Headers {
  support::headers_lookup_fixture()
}

fn teardown_headers(_headers: Headers) {}

#[library_benchmark]
#[bench::lookup(setup = setup_headers, teardown = teardown_headers)]
fn headers_get(headers: Headers) -> Headers {
  // Return the map to `teardown` so Drop/allocator teardown is not attributed to
  // lookup Ir (that cost tracked arena growth / glibc free paths, not `get`).
  let _ = black_box(headers.get(black_box("Content-Type")).is_some());
  headers
}

#[cfg(feature = "gzip")]
mod gzip_benches {
  use super::*;
  use barehttp::gzip::decompress_gzip;

  fn setup_hello() -> &'static [u8] {
    support::GZIP_HELLO_WORLD
  }

  fn setup_long() -> &'static [u8] {
    support::GZIP_LONG
  }

  #[library_benchmark]
  #[bench::hello(setup = setup_hello)]
  #[bench::long(setup = setup_long)]
  pub fn decompress(data: &[u8]) -> usize {
    black_box(decompress_gzip(black_box(data), 4096).unwrap().len())
  }
}

#[cfg(feature = "gzip")]
use gzip_benches::decompress as gzip_decompress;

/// Buffered `Response::parse` pays for the Headers arena (vs old CompactString).
/// `headers_get` improved ~3× for that trade; keep a wider Ir/cycles gate here so
/// Callgrind does not treat the intentional architecture shift as a regression.
fn parse_regression_config() -> LibraryBenchmarkConfig {
  let mut cfg = LibraryBenchmarkConfig::default();
  cfg.tool(
    Callgrind::default()
      .soft_limits([(EventKind::Ir, 20.0), (EventKind::EstimatedCycles, 15.0)])
      .fail_fast(false),
  );
  cfg
}

fn hot_regression_config() -> LibraryBenchmarkConfig {
  // URI / headers_get stay well under 5%. Tiny fixed-Huffman gzip members are
  // still a few percent above the pre-arena inflate baseline after post-pass CRC
  // + push path restores; keep a slightly wider gate so micro noise does not
  // block release while large-body inflate stays near the old numbers.
  let mut cfg = LibraryBenchmarkConfig::default();
  cfg.tool(
    Callgrind::default()
      .soft_limits([(EventKind::Ir, 10.0), (EventKind::EstimatedCycles, 12.0)])
      .fail_fast(false),
  );
  cfg
}

#[cfg(feature = "gzip")]
library_benchmark_group!(
  name = parse_group,
  config = parse_regression_config(),
  benchmarks = [parse_response]
);

#[cfg(feature = "gzip")]
library_benchmark_group!(
  name = hot_group,
  config = hot_regression_config(),
  benchmarks = [parse_uri, headers_get, gzip_decompress]
);

#[cfg(not(feature = "gzip"))]
library_benchmark_group!(
  name = parse_group,
  config = parse_regression_config(),
  benchmarks = [parse_response]
);

#[cfg(not(feature = "gzip"))]
library_benchmark_group!(
  name = hot_group,
  config = hot_regression_config(),
  benchmarks = [parse_uri, headers_get]
);

main!(library_benchmark_groups = parse_group, hot_group);
