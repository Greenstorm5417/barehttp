//! Decompress `Content-Encoding: gzip` (needs `--features gzip`).
//! Hits a cleartext endpoint that returns gzip when `Accept-Encoding: gzip` is sent.

fn main() -> Result<(), barehttp::Error> {
  // Client auto-sends Accept-Encoding: gzip when gzip is enabled.
  let response = barehttp::get("http://httpbingo.org/gzip").call()?;
  let body = response.to_text()?;

  // Decompressed body must be readable text/JSON, not gzip magic (1f 8b).
  assert!(
    !body.as_bytes().starts_with(&[0x1f, 0x8b]),
    "body still looks gzip-compressed"
  );
  assert!(
    body.contains('{') || body.to_ascii_lowercase().contains("gzip"),
    "expected readable JSON/text in decompressed body, got: {}",
    body.chars().take(200).collect::<String>()
  );

  println!("{} {}", response.status(), body);
  Ok(())
}
