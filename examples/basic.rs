//! Basic HTTP client usage example
//!
//! Demonstrates simple GET and POST requests using barehttp.

use barehttp::response::ResponseExt;
use barehttp::{Error, HttpClient};

fn main() -> Result<(), Error> {
  println!("=== Basic barehttp Examples ===\n");

  // Simple GET request using convenience function
  println!("1. Simple GET request:");
  let response = barehttp::get("http://httpbin.org/get")?;
  println!("Status: {}", response.status_code);
  println!("Body preview: {}...\n", &response.text()?[..100]);

  // Using HttpClient for repeated requests
  println!("2. Using HttpClient:");
  let mut client = HttpClient::new()?;

  let response = client
    .get("http://httpbin.org/get")
    .header("User-Agent", "barehttp-example/1.0")
    .call()?;

  if response.is_success() {
    println!("✓ Request successful");
    println!("Status: {}\n", response.status_code);
  }

  // POST request with JSON body
  println!("3. POST request with JSON:");
  let json_body = br#"{"name":"barehttp","type":"http-client"}"#;

  let response = client
    .post("http://httpbin.org/post")
    .header("Content-Type", "application/json")
    .send(json_body.to_vec())?;

  println!("Status: {}", response.status_code);
  println!("✓ POST successful\n");

  // GET with query parameters
  println!("4. GET with query parameters:");
  let response = client
    .get("http://httpbin.org/get")
    .query("foo", "bar")
    .query("baz", "qux")
    .call()?;

  println!("Status: {}", response.status_code);
  println!("✓ Query parameters sent\n");

  Ok(())
}
