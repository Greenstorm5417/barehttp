//! dhat-rs heap profile: Response::parse / URI / Headers allocations.
//!
//! Prints total allocation count, total bytes, and peak live bytes; writes
//! `dhat-heap.json` (view with Valgrind's dh_view.html). Output lands in CWD
//! (typically the crate root when run via `cargo bench`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use barehttp::{Headers, Response, Uri};
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
    let r = Response::parse(black_box(support::RESP_PLAIN)).unwrap();
    black_box(r.status_code());
  }
  report("parse_plain", before);

  let before = dhat::HeapStats::get();
  for _ in 0..iters {
    let r = Response::parse(black_box(support::RESP_CHUNKED)).unwrap();
    black_box(r.body());
  }
  report("parse_chunked", before);

  let before = dhat::HeapStats::get();
  for _ in 0..iters {
    black_box(Uri::parse(black_box(support::URI_SIMPLE)).unwrap());
  }
  report("parse_uri", before);

  let before = dhat::HeapStats::get();
  for _ in 0..iters {
    let mut h = Headers::new();
    for i in 0..16 {
      h.insert(format!("H{i}"), "v");
    }
    black_box(h.get("H8"));
  }
  report("headers_build", before);

  let stats = dhat::HeapStats::get();
  eprintln!(
    "dhat[summary]: total_blocks={} total_bytes={} max_blocks={} max_bytes={}",
    stats.total_blocks, stats.total_bytes, stats.max_blocks, stats.max_bytes
  );
}
