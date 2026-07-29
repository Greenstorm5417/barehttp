//! Private `parser` module must not be reachable from outside the crate.
fn main() {
  let _ = barehttp::parser::has_complete_headers(b"");
}
