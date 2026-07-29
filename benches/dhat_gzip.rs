//! dhat-rs heap profile: gzip / deflate inflate (`--features gzip`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use barehttp::gzip::{decompress_gzip, decompress_http_deflate};
use std::hint::black_box;

#[path = "support.rs"]
mod support;

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

fn report(
  label: &str,
  before: dhat::HeapStats,
) {
  let after = dhat::HeapStats::get();
  eprintln!(
    "dhat[{label}]: allocs={} bytes={} curr_live={} process_peak_live={}",
    after.total_blocks.saturating_sub(before.total_blocks),
    after.total_bytes.saturating_sub(before.total_bytes),
    after.curr_bytes,
    after.max_bytes,
  );
}

fn main() {
  let _profiler = dhat::Profiler::new_heap();
  let iters = std::env::var("BAREHTTP_DHAT_ITERS")
    .ok()
    .and_then(|s| s.parse().ok())
    .unwrap_or(200usize);

  let before = dhat::HeapStats::get();
  for _ in 0..iters {
    black_box(decompress_gzip(black_box(support::GZIP_HELLO_WORLD), 64).unwrap());
  }
  report("gzip_hello", before);

  let before = dhat::HeapStats::get();
  for _ in 0..iters {
    black_box(decompress_gzip(black_box(support::GZIP_LONG), 4096).unwrap());
  }
  report("gzip_long", before);

  let before = dhat::HeapStats::get();
  for _ in 0..iters {
    black_box(decompress_http_deflate(black_box(support::ZLIB_HELLO), 64).unwrap());
  }
  report("zlib_hello", before);

  let stats = dhat::HeapStats::get();
  eprintln!(
    "dhat[summary]: total_blocks={} total_bytes={} max_blocks={} max_bytes={}",
    stats.total_blocks, stats.total_bytes, stats.max_blocks, stats.max_bytes
  );
}
