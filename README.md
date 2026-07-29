# barehttp

Blocking HTTP/1.1 client for `no_std` + `alloc`. No async runtime.

Cleartext HTTP by default. `https://` needs `Config::assume_tls_socket`
and a [`BlockingSocket`] that terminates TLS. The OS socket is TCP only.

`assume_tls_socket` with [`OsBlockingSocket`] returns
`Error::TlsNotConfigured`. Use a TLS-capable socket adapter.

```rust
use barehttp::HttpClient;

fn main() -> Result<(), barehttp::Error> {
    let client = HttpClient::new();
    let response = client.get("http://example.com").call()?;
    println!("{} {}", response.status_code, response.text()?);
    Ok(())
}
```

`Agent` is `HttpClient<OsBlockingSocket, OsDnsResolver>`. `barehttp::agent()`
builds one. Free functions (`get`, `post`, …) return a `RequestBuilder`
(same as `HttpClient::get`). Finish with `.call()` or `.send(body)`:

```rust
let response = barehttp::get("http://example.com").call()?;
let response = barehttp::post("http://example.com/api").send(b"{}")?;
```

## Features

- Custom `BlockingSocket` / `DnsResolver` (`connect` gets the hostname for SNI)
- Connection pooling (`Config::max_idle_per_host` default 3; `0` disables; `max_idle_age` default 15s)
- Response body size limit (`Config::max_response_body_size`, default ~10 MiB)
- Optional Cargo features: `cookie-jar`, `gzip-decompression` (hand-rolled RFC 1951/1952), `zstd-decompression`
- Request builder: `.form` / `.body` then `.call()`, or `.send(bytes)`; per-request `.timeout_read` / `.timeout_write` / `.timeout_connect`

## TLS / HTTPS

barehttp does not implement TLS. For `https://`:

1. Use a `BlockingSocket` whose `connect` / read / write speak TLS (or wrap one
   that does). `connect` receives the URI hostname for SNI.
2. Set `Config { assume_tls_socket: true, .. }` so the client accepts `https`.
   Without that flag you get `Error::TlsNotConfigured`.

```rust
use barehttp::config::Config;
use barehttp::HttpClient;

let config = Config {
    assume_tls_socket: true,
    ..Default::default()
};
// Pair with a TLS-capable BlockingSocket. OsBlockingSocket is cleartext
// and rejects this config.
let client = HttpClient::<MyTlsSocket, _>::with_adapters(my_dns, config);
```

## Config

```rust
use barehttp::config::Config;
use barehttp::HttpClient;
use core::time::Duration;

let config = Config::builder()
    .timeout_read(Some(Duration::from_secs(30)))
    .timeout_write(Some(Duration::from_secs(30)))
    .max_redirects(5)
    .user_agent("my-app/1.0")
    .build();

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
