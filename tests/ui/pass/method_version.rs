//! Method and Version public usage.
use barehttp::{Method, Version};

fn main() {
  let m = Method::Get;
  let _ = m.as_str();
  let _ = Version::HTTP_11;
}
