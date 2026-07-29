#![no_main]

use barehttp::Uri;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
  if let Ok(s) = core::str::from_utf8(data) {
    let _ = Uri::parse(s);
  }
});
