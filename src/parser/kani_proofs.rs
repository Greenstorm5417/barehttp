//! Bounded Kani proofs for pure parser helpers.
//!
//! Included from `parser` when built with `cargo kani` (`cfg(kani)`).
//! Run with `cargo kani --all-features` (see CONTRIBUTING.md).

use super::has_complete_headers;

#[kani::proof]
fn empty_has_no_complete_headers() {
  assert!(!has_complete_headers(&[]));
}

#[kani::proof]
fn crlf_crlf_is_complete() {
  assert!(has_complete_headers(b"\r\n\r\n"));
}

#[kani::proof]
fn lf_lf_is_complete() {
  assert!(has_complete_headers(b"\n\n"));
}

#[kani::proof]
fn incomplete_status_line_not_complete() {
  assert!(!has_complete_headers(b"HTTP/1.1 200 OK\r\n"));
}

/// Symbolic buffer of length ≤ 8: if the scan reports complete, the slice
/// contains `\r\n\r\n` or `\n\n`.
#[kani::proof]
#[kani::unwind(32)]
fn has_complete_headers_sound_small() {
  let len: usize = kani::any();
  kani::assume(len <= 8);
  let mut buf = [0u8; 8];
  for i in 0..len {
    if let Some(slot) = buf.get_mut(i) {
      *slot = kani::any();
    }
  }
  let data = buf.get(..len).unwrap_or(&[]);
  if has_complete_headers(data) {
    let has_crlf = data.windows(4).any(|w| w == b"\r\n\r\n");
    let has_lf = data.windows(2).any(|w| w == b"\n\n");
    assert!(has_crlf || has_lf);
  }
}
