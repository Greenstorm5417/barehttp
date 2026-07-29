//! Minimal GET.

fn main() -> Result<(), barehttp::Error> {
  let response = barehttp::get("http://example.com").call()?;
  println!("{} {}", response.status_code(), response.to_text()?);
  Ok(())
}
