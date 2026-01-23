//! Custom headers example
//!
//! Demonstrates how to set custom headers on requests.

use barehttp::response::ResponseExt;
use barehttp::{Error, HttpClient};

fn main() -> Result<(), Error> {
  println!("=== Custom Headers Examples ===\n");

  let client = HttpClient::new()?;

  // Example 1: Single custom header
  println!("1. Single custom header:");
  let response = client
    .get("http://httpbin.org/headers")
    .header("X-Custom-Header", "my-value")
    .call()?;

  println!("Status: {}", response.status_code);
  println!("✓ Custom header sent\n");

  // Example 2: Multiple headers
  println!("2. Multiple headers:");
  let response = client
    .get("http://httpbin.org/headers")
    .header("User-Agent", "barehttp-custom/1.0")
    .header("Accept", "application/json")
    .header("X-Request-ID", "12345")
    .header("X-API-Key", "secret-key-here")
    .call()?;

  println!("Status: {}", response.status_code);
  println!("Body preview: {}...\n", &response.text()?[..150]);

  // Example 3: Authorization header
  println!("3. Authorization header:");
  let response = client
    .get("http://httpbin.org/bearer")
    .header("Authorization", "Bearer my-token-here")
    .call()?;

  println!("Status: {}", response.status_code);
  println!("✓ Authorization header sent\n");

  // Example 4: Content-Type for POST
  println!("4. POST with Content-Type:");
  let response = client
    .post("http://httpbin.org/post")
    .header("Content-Type", "application/x-www-form-urlencoded")
    .send(b"key1=value1&key2=value2".to_vec())?;

  println!("Status: {}", response.status_code);
  println!("✓ Form data posted\n");

  // Example 5: Custom headers with query parameters
  println!("5. Headers + query parameters:");
  let response = client
    .get("http://httpbin.org/get")
    .header("X-Tracking-ID", "abc-123")
    .query("search", "rust")
    .query("limit", "10")
    .call()?;

  println!("Status: {}", response.status_code);
  println!("✓ Combined headers and query params\n");

  Ok(())
}
