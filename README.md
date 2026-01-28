# barehttp

[![Crates.io](https://img.shields.io/crates/v/barehttp)](https://crates.io/crates/barehttp) [![Documentation](https://docs.rs/barehttp/badge.svg)](https://docs.rs/barehttp) [![License](https://img.shields.io/crates/l/barehttp)](https://github.com/Greenstorm5417/barehttp) [![Build Status](https://github.com/Greenstorm5417/barehttp/workflows/CI/badge.svg)](https://github.com/Greenstorm5417/barehttp/actions) [![Downloads](https://img.shields.io/crates/d/barehttp)](https://crates.io/crates/barehttp) [![GitHub stars](https://img.shields.io/github/stars/Greenstorm5417/barehttp)](https://github.com/Greenstorm5417/barehttp/stargazers) [![GitHub issues](https://img.shields.io/github/issues/Greenstorm5417/barehttp)](https://github.com/Greenstorm5417/barehttp/issues) [![Rust Version](https://img.shields.io/badge/rust-2024+-blue.svg)](https://www.rust-lang.org)

**A minimal, explicit HTTP client for Rust**

barehttp is a low-level, blocking HTTP client designed for developers who want
**predictable behavior, full control, and minimal dependencies**.

It supports `no_std` (with `alloc`), avoids global state, and exposes all network
behavior explicitly. There is no async runtime, no hidden connection pooling,
and no built-in TLS—you bring your own via adapters.

## Key Features

- **Minimal and explicit**: No global state, no implicit behavior
- **`no_std` compatible**: Core works with `alloc` only
- **Blocking I/O**: Simple, predictable execution model
- **Generic adapters**: Custom socket and DNS implementations
- **Compile-time safety**: Typestate enforces correct body usage

## Quick Start

```no_run
use barehttp::response::ResponseExt;

let response = barehttp::get("http://httpbin.org/get")?;
let body = response.text()?;
println!("{}", body);
# Ok::<(), barehttp::Error>(())
```

## Using `HttpClient`

For repeated requests or more control, use [`HttpClient`]:

```no_run
use barehttp::HttpClient;
use barehttp::response::ResponseExt;

let mut client = HttpClient::new()?;

let response = client
    .get("http://httpbin.org/get")
    .header("User-Agent", "my-app/1.0")
    .call()?;

println!("Status: {}", response.status_code);
println!("Body: {}", response.text()?);
# Ok::<(), barehttp::Error>(())
```

## Configuration

Client behavior is controlled via [`config::Config`] and [`config::ConfigBuilder`]:

```no_run
use barehttp::HttpClient;
use barehttp::config::ConfigBuilder;
use core::time::Duration;

let config = ConfigBuilder::new()
    .timeout(Duration::from_secs(30))
    .max_redirects(5)
    .user_agent("my-app/2.0")
    .build();

let mut client = HttpClient::with_config(config)?;

let response = client.get("http://httpbin.org/get").call()?;
# Ok::<(), barehttp::Error>(())
```

## Making Requests

### GET

```no_run
let mut client = barehttp::HttpClient::new()?;

let response = client.get("http://httpbin.org/get").call()?;
# Ok::<(), barehttp::Error>(())
```

### POST with body

```no_run
let mut client = barehttp::HttpClient::new()?;

let response = client
    .post("http://httpbin.org/post")
    .header("Content-Type", "application/json")
    .send(br#"{"name":"test"}"#.to_vec())?;
# Ok::<(), barehttp::Error>(())
```

## Type-Safe Request Bodies

Request methods enforce body semantics at compile time:

- GET, HEAD, DELETE, OPTIONS → methods without body
- POST, PUT, PATCH → methods with body

```compile_fail
let mut client = barehttp::HttpClient::new()?;
client.get("http://example.com").send(vec![])?;
```

```no_run
let mut client = barehttp::HttpClient::new()?;
client.post("http://example.com").send(b"data".to_vec())?;
# Ok::<(), barehttp::Error>(())
```

## Error Handling

All operations return `Result<T, barehttp::Error>`.

Errors include:
- DNS resolution failures
- Socket I/O errors
- Parse errors
- HTTP status errors (4xx/5xx by default)

```no_run
use barehttp::{Error, HttpClient};

let mut client = HttpClient::new()?;

match client.get("http://httpbin.org/status/404").call() {
    Ok(resp) => println!("Status: {}", resp.status_code),
    Err(Error::HttpStatus(code)) => println!("HTTP error: {}", code),
    Err(e) => println!("Other error: {:?}", e),
}
# Ok::<(), barehttp::Error>(())
```

Automatic HTTP status errors can be disabled via configuration.

## Response Helpers

The [`response::ResponseExt`] trait provides helpers for common tasks:

```no_run
use barehttp::response::ResponseExt;

let mut client = barehttp::HttpClient::new()?;
let response = client.get("http://httpbin.org/get").call()?;

if response.is_success() {
    let text = response.text()?;
    println!("{}", text);
}
# Ok::<(), barehttp::Error>(())
```

## Custom Socket and DNS Adapters

barehttp’s networking is fully pluggable.

Implement `BlockingSocket` and `DnsResolver` traits to provide:

- TLS via external libraries
- Proxies or tunnels
- Embedded or WASM networking
- Test mocks

```no_run
use barehttp::{HttpClient, OsBlockingSocket, OsDnsResolver};

let dns = OsDnsResolver::new();
let mut client: HttpClient<OsBlockingSocket, _> = HttpClient::new_with_adapters(dns);
let response = client.get("http://example.com").call()?;
# Ok::<(), barehttp::Error>(())
```

## Design Notes

- Blocking I/O keeps the API simple and dependency-free
- Each request is independent and explicit
- No shared state or background behavior

barehttp is intended for environments where **clarity and control matter more than convenience**.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
