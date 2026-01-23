//! Error handling example
//!
//! Demonstrates different error scenarios and how to handle them.

use barehttp::config::{ConfigBuilder, HttpStatusHandling};
use barehttp::response::ResponseExt;
use barehttp::{Error, HttpClient};

fn main() -> Result<(), Error> {
  println!("=== Error Handling Examples ===\n");

  let client = HttpClient::new()?;

  // Example 1: Handling HTTP status errors
  println!("1. HTTP status error (404):");
  match client.get("http://httpbin.org/status/404").call() {
    Ok(response) => {
      println!("Status: {}", response.status_code);
    },
    Err(Error::HttpStatus(code)) => {
      println!("✓ Caught HTTP status error: {}\n", code);
    },
    Err(e) => {
      println!("Other error: {:?}\n", e);
    },
  }

  // Example 2: Handling different error types
  println!("2. Invalid URL:");
  match client.get("not-a-valid-url").call() {
    Ok(_) => println!("Unexpected success"),
    Err(e) => {
      println!("✓ Caught error: {:?}\n", e);
    },
  }

  // Example 3: DNS resolution error (fake domain)
  println!("3. DNS resolution error:");
  match client
    .get("http://this-domain-definitely-does-not-exist-12345.com")
    .call()
  {
    Ok(_) => println!("Unexpected success"),
    Err(Error::Dns(e)) => {
      println!("✓ Caught DNS error: {:?}\n", e);
    },
    Err(e) => {
      println!("Other error: {:?}\n", e);
    },
  }

  // Example 4: Treating HTTP errors as responses
  println!("4. HTTP errors as responses (not errors):");
  let config = ConfigBuilder::new()
    .http_status_handling(HttpStatusHandling::AsResponse)
    .build();

  let client = HttpClient::with_config(config)?;

  match client.get("http://httpbin.org/status/500").call() {
    Ok(response) => {
      println!("✓ Got response with status: {}", response.status_code);
      println!("Is server error: {}\n", response.is_server_error());
    },
    Err(e) => {
      println!("Error: {:?}\n", e);
    },
  }

  // Example 5: Comprehensive error matching
  println!("5. Comprehensive error handling:");
  let result = client.get("http://httpbin.org/status/403").call();

  match result {
    Ok(response) => {
      println!("Success! Status: {}", response.status_code);
    },
    Err(Error::HttpStatus(code)) => {
      println!("✓ HTTP error: {}", code);
    },
    Err(Error::Dns(e)) => {
      println!("DNS error: {:?}", e);
    },
    Err(Error::Socket(e)) => {
      println!("Socket error: {:?}", e);
    },
    Err(Error::Parse(e)) => {
      println!("Parse error: {:?}", e);
    },
    Err(Error::TooManyRedirects) => {
      println!("Too many redirects");
    },
    Err(e) => {
      println!("Other error: {:?}", e);
    },
  }

  Ok(())
}
