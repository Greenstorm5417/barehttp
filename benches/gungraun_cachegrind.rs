//! Gungraun Cachegrind: simulated cache behavior.
//!
//! Uses Cachegrind client requests so only the measured region is instrumented.
//! Requires `valgrind` and `gungraun-runner` matching the `gungraun` crate version.
//! Allocation metrics: use `dhat_*` benches (not Gungraun DHAT).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use barehttp::Response;
use gungraun::client_requests::cachegrind as cr;
use gungraun::{Cachegrind, LibraryBenchmarkConfig, ValgrindTool, library_benchmark, library_benchmark_group, main};
use std::hint::black_box;

#[path = "support.rs"]
mod support;

fn cachegrind_config() -> LibraryBenchmarkConfig {
  let mut cfg = LibraryBenchmarkConfig::default();
  cfg
    .default_tool(ValgrindTool::Cachegrind)
    .tool(Cachegrind::with_args(["--instr-at-start=no"]));
  cfg
}

#[library_benchmark(config = cachegrind_config())]
#[bench::plain(support::RESP_PLAIN)]
#[bench::chunked(support::RESP_CHUNKED)]
fn parse_response_cache(data: &[u8]) -> u16 {
  cr::start_instrumentation();
  let r = Response::parse(black_box(data)).unwrap();
  let code = black_box(r.status_code());
  cr::stop_instrumentation();
  code
}

#[cfg(feature = "gzip")]
mod gzip_benches {
  use super::*;
  use barehttp::gzip::decompress_gzip;

  #[library_benchmark(config = cachegrind_config())]
  #[bench::hello(support::GZIP_HELLO_WORLD)]
  #[bench::long(support::GZIP_LONG)]
  pub fn decompress_cache(data: &[u8]) -> usize {
    cr::start_instrumentation();
    let n = black_box(decompress_gzip(black_box(data), 4096).unwrap().len());
    cr::stop_instrumentation();
    n
  }
}

#[cfg(feature = "gzip")]
use gzip_benches::decompress_cache as gzip_decompress_cache;

#[cfg(feature = "gzip")]
library_benchmark_group!(
  name = cachegrind_group,
  benchmarks = [parse_response_cache, gzip_decompress_cache]
);

#[cfg(not(feature = "gzip"))]
library_benchmark_group!(name = cachegrind_group, benchmarks = [parse_response_cache]);

main!(library_benchmark_groups = cachegrind_group);
