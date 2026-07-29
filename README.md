# barehttp

Blocking HTTP/1.1 client for `no_std` + `alloc`. Cleartext HTTP; no async.
Design notes: [philosophy.md](philosophy.md).

**MSRV:** Rust **1.87** (`rust-version` in `Cargo.toml`; set by `ruzstd` when using `zstd`).

`https://` needs [`config::Config::assume_tls_socket`] and a [`BlockingSocket`] that terminates TLS.
[`OsBlockingSocket`] is TCP only. Pairing it with `assume_tls_socket` returns
[`Error::TlsNotConfigured`].

```rust,no_run
fn main() -> Result<(), barehttp::Error> {
    let response = barehttp::get("http://example.com").call()?;
    println!("{} {}", response.status_code(), response.to_text()?);
    Ok(())
}
```

## Naming

Primary names:

| Role | Primary | Compatibility alias |
|------|---------|---------------------|
| Client | [`HttpClient`] | [`Agent`] (= `HttpClient<OsBlockingSocket, OsDnsResolver>`) |
| Request builder | [`ClientRequestBuilder`] | [`RequestBuilder`] (same OS adapters) |
| Cookie store (`cookie-jar`) | [`cookie_jar::CookieStore`] | [`cookie_jar::CookieJar`] |
| Status / body | [`Response::status_code`], [`Response::body`] | deprecated [`Response::status`] / [`Response::as_bytes`] |

[`Agent`] / [`RequestBuilder`] / [`cookie_jar::CookieJar`] are documented ureq-like synonyms and are not deprecated. Prefer the primary names in new code. README and examples use primaries only.

[`agent`] builds a default-OS [`HttpClient`]. Free functions ([`get`], [`post`], …) return a builder; finish with [`ClientRequestBuilder::call`] or [`ClientRequestBuilder::send`]:

```rust,no_run
# fn main() -> Result<(), barehttp::Error> {
let response = barehttp::get("http://example.com").call()?;
let response = barehttp::post("http://example.com/api").send(b"{}")?;
# let _ = response;
# Ok(())
# }
```

## Module layout

Hot types live at the crate root (`HttpClient`, `Response`, `Error`, `Headers`, …).
Optional / larger surfaces stay in modules: [`config`], [`request_builder`], and
(feature-gated) [`cookie_jar`] / [`gzip`]. The half-nesting is intentional.

## Intentional limits

- **Buffered** response bodies (no streaming `Read` body API).
- **Blocking** I/O only (no async runtime).
- Public body accessors use `&[u8]` / `Vec<u8>` / `String` only (`core` / `alloc`).

## Features

- Custom [`BlockingSocket`] / [`DnsResolver`] (`connect` gets the hostname for SNI)
- Connection pooling (`Config::max_idle_per_host` default 3; `0` disables; `max_idle_age` default 15s)
- Response body size limit (`Config::max_response_body_size`, default ~10 MiB)
- Optional Cargo features: `gzip` exposes `barehttp::gzip` (hand-rolled RFC 1951/1952); `zstd` is Accept-Encoding + decode only; `cookie-jar` gates the cookie module
- Runtime deps (always on, kept out of the public API): `bytes` (internal body / wire buffers), `phf` (well-known header name map), `compact_str` (SSO header strings), `hashbrown` (header side-index / pool). Platform: `libc` / `windows-sys`.
- Request builder: `.form` / `.body` then `.call()`, or `.send(bytes)`; per-request `.timeout_read` / `.timeout_write` / `.timeout_connect`

See [CHANGELOG.md](CHANGELOG.md) for release notes.

## Examples

All examples use cleartext HTTP (`http://` only):

```text
cargo run --example basic                    # GET http://example.com
cargo run --example agent                    # shared client, headers + query
cargo run --example custom_adapters          # logging DnsResolver + BlockingSocket over the OS stack
cargo run --example gzip --features gzip     # http://httpbingo.org/gzip
cargo run --example cookies --features cookie-jar        # httpbingo/postman-echo cookie endpoints
```

## TLS / HTTPS

barehttp does not implement TLS. For `https://`:

1. Use a [`BlockingSocket`] whose `connect` / read / write speak TLS (or wrap one that does).
   `connect` receives the URI hostname for SNI.
2. Set `assume_tls_socket` on [`config::Config`] so the client accepts `https`.
   Without that flag you get [`Error::TlsNotConfigured`].

```rust,ignore
use barehttp::config::Config;
use barehttp::HttpClient;

let config = Config::builder()
    .assume_tls_socket(true)
    .build();
// Pair with a TLS-capable BlockingSocket. OsBlockingSocket is cleartext
// and rejects this config.
let client = HttpClient::<MyTlsSocket, _>::with_adapters(my_dns, config);
```

## Config

```rust,no_run
use barehttp::config::Config;
use barehttp::HttpClient;
use core::time::Duration;

# fn main() {
let config = Config::builder()
    .timeout_read(Some(Duration::from_secs(30)))
    .timeout_write(Some(Duration::from_secs(30)))
    .max_redirects(5)
    .user_agent("my-app/1.0")
    .build();

let client = HttpClient::with_config(config);
# let _ = client;
# }
```

## Custom adapters

```rust,ignore
use barehttp::config::Config;
use barehttp::{HttpClient, OsBlockingSocket};

let client: HttpClient<OsBlockingSocket, _> =
    HttpClient::with_adapters(my_dns, Config::default());
```

See `examples/custom_adapters.rs` for logging wrappers around [`OsDnsResolver`] / [`OsBlockingSocket`].

## Testing

Use [cargo-nextest](https://nexte.st/):

```bash
cargo install cargo-nextest --locked
cargo nextest run --all-features
```

CI runs nextest on push and pull requests. Details in [CONTRIBUTING.md](https://github.com/Greenstorm5417/barehttp/blob/main/CONTRIBUTING.md); fuzz targets in [fuzz/README.md](https://github.com/Greenstorm5417/barehttp/blob/main/fuzz/README.md).

## License

MIT OR Apache-2.0. Changes: [CHANGELOG.md](CHANGELOG.md).
