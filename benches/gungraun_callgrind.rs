//! Gungraun Callgrind: instruction / branch / call-count regression (deterministic).
//!
//! Requires `valgrind` and `gungraun-runner` matching the `gungraun` crate version.
//! Does not enable Gungraun DHAT — allocation metrics use the `dhat_*` benches.
//!
//! Soft limits (+5% Ir, +10% EstimatedCycles vs previous/baseline) fail the run on
//! regression — used by CI (`benches` workflow job).
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

#[library_benchmark]
#[bench::lookup(setup = setup_headers)]
fn headers_get(headers: Headers) -> bool {
  black_box(headers.get(black_box("Content-Type")).is_some())
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

#[cfg(feature = "gzip")]
library_benchmark_group!(
  name = callgrind_group,
  benchmarks = [parse_response, parse_uri, headers_get, gzip_decompress]
);

#[cfg(not(feature = "gzip"))]
library_benchmark_group!(
  name = callgrind_group,
  benchmarks = [parse_response, parse_uri, headers_get]
);

fn callgrind_regression_config() -> LibraryBenchmarkConfig {
  let mut cfg = LibraryBenchmarkConfig::default();
  cfg.tool(
    Callgrind::default()
      .soft_limits([(EventKind::Ir, 5.0), (EventKind::EstimatedCycles, 10.0)])
      .fail_fast(false),
  );
  cfg
}

main!(
  config = callgrind_regression_config(),
  library_benchmark_groups = callgrind_group
);
