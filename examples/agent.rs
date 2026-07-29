//! Shared client: headers + query.

fn main() -> Result<(), barehttp::Error> {
  let agent = barehttp::agent();

  let response = agent
    .get("http://example.com")
    .header("X-Example", "1")
    .query("q", "barehttp")
    .call()?;

  println!(
    "{} {}",
    response.status(),
    response.text()?.chars().take(120).collect::<String>()
  );
  Ok(())
}
