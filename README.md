# barehttp

**A minimal, explicit HTTP client for Rust**

barehttp is a low-level, blocking HTTP client designed for developers who want
**predictable behavior, full control, and minimal dependencies**.

It supports `no_std` (with `alloc`), avoids global state, and exposes all network
behavior explicitly. There is no async runtime, no built-in TLS—you bring your own
via adapters.

## Key Features

- **Minimal and explicit**: No global state, no implicit behavior
- **`no_std` compatible**: Core works with `alloc` only
- **Blocking I/O**: Simple, predictable execution model
- **Generic adapters**: Custom socket and DNS implementations
- **Compile-time safety**: Typestate enforces correct body usage

## Quick Start

```rust
use barehttp::response::ResponseExt;

let response = barehttp::get("http://httpbin.org/get")?;
let body = response.text()?;
println!("{}", body);
```

## Using `HttpClient`

For repeated requests or more control, use `HttpClient`:

```rust
use barehttp::HttpClient;
use barehttp::response::ResponseExt;

let mut client = HttpClient::new()?;

let response = client
    .get("http://httpbin.org/get")
    .header("User-Agent", "my-app/1.0")
    .call()?;

println!("Status: {}", response.status_code);
println!("Body: {}", response.text()?);
```

## Configuration

Client behavior is controlled via `Config` and `ConfigBuilder`:

```rust
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
```

## Making Requests

### GET

```rust
let mut client = barehttp::HttpClient::new()?;

let response = client.get("http://httpbin.org/get").call()?;
```

### Connection Pooling

Connection pooling is enabled by default for better performance:

```rust
use barehttp::config::ConfigBuilder;
use core::time::Duration;

// Customize pooling behavior
let config = ConfigBuilder::new()
    .connection_pooling(true)  // enabled by default
    .max_idle_per_host(5)      // max idle connections per host
    .idle_timeout(Duration::from_secs(90))  // idle timeout
    .build();

let mut client = barehttp::HttpClient::with_config(config)?;

// First request creates a new connection
let resp1 = client.get("http://httpbin.org/get").call()?;

// Second request reuses the pooled connection
let resp2 = client.get("http://httpbin.org/status/200").call()?;

// Disable pooling for one-off requests
let config = ConfigBuilder::new()
    .connection_pooling(false)
    .build();
```

### POST with body

```rust
let mut client = barehttp::HttpClient::new()?;

let response = client
    .post("http://httpbin.org/post")
    .header("Content-Type", "application/json")
    .send(br#"{"name":"test"}"#.to_vec())?;
```

## Type-Safe Request Bodies

Request methods enforce body semantics at compile time:

- GET, HEAD, DELETE, OPTIONS → methods without body
- POST, PUT, PATCH → methods with body

```rust
// This won't compile:
let mut client = barehttp::HttpClient::new()?;
client.get("http://example.com").send(vec![])?;
```

```rust
// This compiles - POST requires .send():
let mut client = barehttp::HttpClient::new()?;
client.post("http://example.com").send(b"data".to_vec())?;
```

## Error Handling

All operations return `Result<T, barehttp::Error>`.

Errors include:
- DNS resolution failures
- Socket I/O errors
- Parse errors
- HTTP status errors (4xx/5xx by default)

```rust
use barehttp::{Error, HttpClient};

let mut client = HttpClient::new()?;

match client.get("http://httpbin.org/status/404").call() {
    Ok(resp) => println!("Status: {}", resp.status_code),
    Err(Error::HttpStatus(code)) => println!("HTTP error: {}", code),
    Err(e) => println!("Other error: {:?}", e),
}
```

Automatic HTTP status errors can be disabled via configuration.

## Response Helpers

The `ResponseExt` trait provides helpers for common tasks:

```rust
use barehttp::response::ResponseExt;

let mut client = barehttp::HttpClient::new()?;
let response = client.get("http://httpbin.org/get").call()?;

if response.is_success() {
    let text = response.text()?;
    println!("{}", text);
}
```

## Custom Socket and DNS Adapters

barehttp's networking is fully pluggable.

Implement `BlockingSocket` and `DnsResolver` traits to provide:

- TLS via external libraries
- Proxies or tunnels
- Embedded or WASM networking
- Test mocks

```rust
use barehttp::{HttpClient, OsBlockingSocket, OsDnsResolver};

let dns = OsDnsResolver::new();
let mut client: HttpClient<OsBlockingSocket, _> = HttpClient::new_with_adapters(dns);
let response = client.get("http://example.com").call()?;
```

### Implementing Custom Sockets

For embedded systems or custom networking, implement `BlockingSocket`:

```rust
use barehttp::socket::BlockingSocket;
use barehttp::error::SocketError;

struct MyCustomSocket {
    // Your custom implementation
}

impl BlockingSocket for MyCustomSocket {
    fn new() -> Result<Self, SocketError> {
        // Initialize your socket (called by the pool when needed)
        Ok(Self { /* ... */ })
    }
    
    fn connect(&mut self, addr: &SocketAddr<'_>) -> Result<(), SocketError> {
        // Your connection logic
    }
    
    // ... implement other trait methods
}

// Use with HttpClient
let dns = OsDnsResolver::new();
let client: HttpClient<MyCustomSocket, _> = HttpClient::new_with_adapters(dns);
```

The pool will call `MyCustomSocket::new()` internally when it needs to create connections.

## Design Notes

- **Blocking I/O** keeps the API simple and dependency-free
- **Connection pooling** reuses TCP connections for better performance (configurable)
- **Generic adapters** allow custom socket implementations via trait
- **Explicit behavior** with no hidden global state

barehttp is intended for environments where **clarity and control matter more than convenience**.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
