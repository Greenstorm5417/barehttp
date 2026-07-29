# barehttp

Blocking HTTP/1.1 client for `no_std` + `alloc`. No async runtime. Cleartext HTTP; `https://` is rejected unless `Config::assume_tls_socket` is set and your [`BlockingSocket`](https://docs.rs/barehttp) terminates TLS.

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
- Custom `BlockingSocket` and `DnsResolver`
- Connection pooling on by default (`Config::max_idle_per_host`; `0` disables)
- Cargo features: `cookie-jar`, `gzip-decompression`, `zstd-decompression`
- Request builder: GET/HEAD/DELETE use `call()`; POST/PUT/PATCH use `send`/`form`

## Config

```rust
use barehttp::config::Config;
use barehttp::HttpClient;
use core::time::Duration;

let config = Config {
    timeout_read: Some(Duration::from_secs(30)),
    timeout_write: Some(Duration::from_secs(30)),
    max_redirects: 5,
    user_agent: String::from("my-app/1.0"),
    ..Default::default()
};

let client = HttpClient::with_config(config);
```

## Custom adapters

```rust
use barehttp::config::Config;
use barehttp::{HttpClient, OsBlockingSocket};

let client: HttpClient<OsBlockingSocket, _> =
    HttpClient::with_adapters(my_dns, Config::default());
```

## License

MIT OR Apache-2.0
