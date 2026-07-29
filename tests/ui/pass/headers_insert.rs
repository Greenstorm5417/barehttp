//! Headers insert / get.
use barehttp::Headers;

fn main() {
  let mut h = Headers::new();
  h.insert("Accept", "*/*");
  let _ = h.get("accept");
}
