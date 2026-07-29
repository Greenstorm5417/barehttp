#![no_main]

use barehttp::Response;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
  // Must not panic on arbitrary input.
  let _ = Response::parse(data);
});
