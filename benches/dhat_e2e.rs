//! dhat-rs heap profile: end-to-end loopback GET (no internet).
#![allow(clippy::unwrap_used, clippy::expect_used)]

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
    .unwrap_or(50usize);

  let server = support::LoopbackServer::spawn(support::RESP_PLAIN.to_vec());
  let url = server.url();
  let client = support::bench_client();

  // Warm one request outside the measured window.
  let _ = client.get(&url).call().unwrap();

  let before = dhat::HeapStats::get();
  for _ in 0..iters {
    let resp = client.get(black_box(url.as_str())).call().unwrap();
    black_box(resp.body().len());
  }
  report("e2e_get_plain", before);

  let stats = dhat::HeapStats::get();
  eprintln!(
    "dhat[summary]: total_blocks={} total_bytes={} max_blocks={} max_bytes={}",
    stats.total_blocks, stats.total_bytes, stats.max_blocks, stats.max_bytes
  );

  drop(server);
}
