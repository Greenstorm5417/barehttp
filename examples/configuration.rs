//! Configuration example
//!
//! Demonstrates how to configure the HTTP client with custom settings.

use barehttp::config::{ConfigBuilder, HttpStatusHandling, RedirectPolicy};
use barehttp::response::ResponseExt;
use barehttp::{Error, HttpClient};
use core::time::Duration;

fn main() -> Result<(), Error> {
  println!("=== Configuration Examples ===\n");

  // Example 1: Custom timeout
  println!("1. Client with custom timeout:");
  let config = ConfigBuilder::new()
    .timeout(Duration::from_secs(10))
    .user_agent("barehttp-config-example/1.0")
    .build();

  let mut client = HttpClient::with_config(config)?;

  let response = client.get("http://httpbin.org/delay/2").call()?;
  println!("✓ Request completed with timeout setting");
  println!("Status: {}\n", response.status_code);

  // Example 2: Disable automatic redirects
  println!("2. Client with no redirect following:");
  let config = ConfigBuilder::new()
    .redirect_policy(RedirectPolicy::NoFollow)
    .build();

  let mut client = HttpClient::with_config(config)?;

  let response = client.get("http://httpbin.org/redirect/1").call()?;
  println!("Status: {} (redirect not followed)", response.status_code);
  if let Some(location) = response.get_header("Location") {
    println!("Location header: {}\n", location);
  }

  // Example 3: Treat HTTP errors as responses (not errors)
  println!("3. HTTP status codes as responses:");
  let config = ConfigBuilder::new()
    .http_status_handling(HttpStatusHandling::AsResponse)
    .build();

  let mut client = HttpClient::with_config(config)?;

  let response = client.get("http://httpbin.org/status/404").call()?;
  println!("Status: {} (returned as response, not error)", response.status_code);
  println!("Is client error: {}\n", response.is_client_error());

  // Example 4: Limit redirects
  println!("4. Limit maximum redirects:");
  let config = ConfigBuilder::new()
    .max_redirects(2)
    .redirect_policy(RedirectPolicy::Follow)
    .build();

  let mut client = HttpClient::with_config(config)?;

  let response = client.get("http://httpbin.org/redirect/1").call()?;
  println!("✓ Followed redirect (max 2)");
  println!("Final status: {}\n", response.status_code);

  Ok(())
}
