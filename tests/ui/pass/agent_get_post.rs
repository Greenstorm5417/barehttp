//! Construct default Agent and free get/post helpers.
use barehttp::{agent, get, post};

fn main() {
  let _ = agent();
  let _ = get("http://example.com");
  let _ = post("http://example.com/api");
}
