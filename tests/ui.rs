//! trybuild UI compile tests for the public API surface.
//!
//! Pass fixtures: `tests/ui/pass/*.rs`
//! Fail fixtures: `tests/ui/fail/*.rs` (+ committed `.stderr` fragments)
//!
//! Edition 2024 / recent nightlies: if trybuild becomes sticky, prefer
//! `tests/api_compile.rs` (compile-pass only) and document the failure here.

#[test]
fn ui() {
  let t = trybuild::TestCases::new();
  t.pass("tests/ui/pass/*.rs");
  t.compile_fail("tests/ui/fail/*.rs");
}
