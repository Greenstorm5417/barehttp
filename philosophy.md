# Philosophy

barehttp is a blocking HTTP/1.1 client for `no_std` + `alloc`. Minimize `.text` and dependencies; the gzip inflater and busy-wait mutex are hand-rolled for size. Cleartext by default; TLS is an adapter you bring.

## Goals

- Default features pull no compression crate. Optional `gzip` is local RFC 1951/1952 code; `zstd` is the only optional runtime dep (`ruzstd`). Platform sockets use `libc` / `windows-sys`.
- Targets `no_std` + `alloc` with sync, blocking I/O.
- Typed errors (`core::error::Error`), private fields with accessors, `#[non_exhaustive]` where the set can grow. API Guidelines-shaped.
- Connect timeouts and chunked framing are required. Response bodies are size-capped.

## Non-goals

- Async / Tokio / any executor
- HTTP/2 or HTTP/3
- Built-in TLS (`BlockingSocket` + `Config::assume_tls_socket` instead)
- Kitchen-sink client (cookies and compression stay behind Cargo features)

For HTTPS, wrap a TLS-capable socket and set `assume_tls_socket`. `OsBlockingSocket` is TCP only.
