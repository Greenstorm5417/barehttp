#![no_main]

use barehttp::gzip::{decompress_gzip, decompress_http_deflate, decompress_raw_deflate};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
  // Cap output so bombs stay bounded during fuzzing.
  const MAX: usize = 64 * 1024;
  let _ = decompress_gzip(data, MAX);
  let _ = decompress_http_deflate(data, MAX);
  let _ = decompress_raw_deflate(data, MAX);
});
