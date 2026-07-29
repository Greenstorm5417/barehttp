//! GET/POST, headers, config, and errors with barehttp.

use barehttp::config::Config;
use barehttp::{Error, HttpClient};
use core::time::Duration;

fn main() -> Result<(), Error> {
  // Convenience GET
  let response = barehttp::get("http://httpbin.org/get")?;
  println!("GET status: {}", response.status_code);

  let client = HttpClient::new();

  // Headers + query
  let response = client
    .get("http://httpbin.org/get")
    .header("User-Agent", "barehttp-example/1.0")
    .header("X-Custom", "one")
    .query("foo", "bar")
    .call()?;
  println!("headers/query status: {}", response.status_code);

  // POST JSON
  let response = client
    .post("http://httpbin.org/post")
    .header("Content-Type", "application/json")
    .send(br#"{"name":"barehttp"}"#)?;
  println!("POST status: {}", response.status_code);

  // Custom config
  let client = HttpClient::with_config(Config {
    timeout_read: Some(Duration::from_secs(10)),
    follow_redirects: false,
    http_status_as_error: false,
    user_agent: String::from("barehttp-example/1.0"),
    ..Default::default()
  });
  let response = client.get("http://httpbin.org/status/404").call()?;
  println!("404 as response: {}", response.status_code);

  // Error match
  match HttpClient::new().get("not-a-valid-url").call() {
    Err(e) => println!("caught: {e:?}"),
    Ok(_) => println!("unexpected ok"),
  }

  Ok(())
}
