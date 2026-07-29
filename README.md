# barehttp

Blocking HTTP/1.1 client for `no_std` + `alloc`. No async runtime. HTTP cleartext only unless your [`BlockingSocket`](https://docs.rs/barehttp) does TLS (`Config::assume_tls_socket`; `https://` is rejected by default).

```rust
use barehttp::HttpClient;

fn main() -> Result<(), barehttp::Error> {
    let client = HttpClient::new();
    let response = client.get("http://example.com").call()?;
    println!("{} {}", response.status_code, response.text()?);
    Ok(())
}
```

## Features

- `no_std` + `alloc`, blocking I/O
- Pluggable socket + DNS (`BlockingSocket`, `DnsResolver`)
- Optional connection pooling (on by default; disable in [`Config`](https://docs.rs/barehttp))
- Optional `cookie-jar`, `gzip-decompression`, `zstd-decompression`
- Typestate builder: GET/HEAD/DELETE have no body; POST/PUT/PATCH take `send`/`form`

## Config

```rust
use barehttp::config::ConfigBuilder;
use barehttp::HttpClient;
use core::time::Duration;

let config = ConfigBuilder::new()
    .timeout(Duration::from_secs(30))
    .max_redirects(5)
    .user_agent("my-app/1.0")
    .build();

let client = HttpClient::with_config(config);
```

## Custom adapters

```rust
use barehttp::{DnsResolver, HttpClient, OsBlockingSocket};

let client: HttpClient<OsBlockingSocket, _> =
    HttpClient::new_with_adapters(my_dns);
```

## License

MIT OR Apache-2.0
