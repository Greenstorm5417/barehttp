//! ExtensionMethod fields are private (opaque token storage).
use barehttp::Method;

fn main() {
  let m = Method::new("CUSTOM").unwrap();
  if let Method::Extension(em) = m {
    let _ = em.token;
  }
}
