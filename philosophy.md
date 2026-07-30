# Philosophy

barehttp is a blocking HTTP/1.1 client for `no_std` + `alloc`. Minimize `.text` and dependencies; the gzip inflater and busy-wait mutex are hand-rolled for size. Cleartext by default; TLS is an adapter you bring.

## Goals

- Default features pull no compression crate. Optional `gzip` is local RFC 1951/1952 code; `zstd` is the only optional compression runtime dep (`ruzstd`). Always-on: `bytes` + `phf` + `compact_str` + `hashbrown` (no_std). Platform sockets use `libc` / `windows-sys`.
- Targets `no_std` + `alloc` with sync, blocking I/O.
- Typed errors (`core::error::Error`), private fields with accessors, `#[non_exhaustive]` where the set can grow. API Guidelines-shaped.
- Connect timeouts and chunked framing are required. Response bodies are size-capped.

## Non-goals

- Async / Tokio / any executor
- HTTP/2 or HTTP/3
- Built-in TLS (`BlockingSocket` + `Config::assume_tls_socket` instead)
- Kitchen-sink client (cookies and compression stay behind Cargo features)
- HTTP caching ([RFC 9111](https://www.rfc-editor.org/rfc/rfc9111))
- CONNECT tunnels / proxy absolute-form / outbound chunked / `Expect: 100-continue` (RFC 9112 origin-client subset; `Method::Connect` is rejected at execution)

## RFCs (intentional subset)

| Area | Spec | Notes |
|------|------|-------|
| Messaging | [RFC 9112](https://www.rfc-editor.org/rfc/rfc9112) | Origin client: origin-form requests, response framing, chunked TE + trailers, Host/Connection |
| Semantics | [RFC 9110](https://www.rfc-editor.org/rfc/rfc9110) | Methods, field rules, redirects (client subset) |
| URI | [RFC 3986](https://www.rfc-editor.org/rfc/rfc3986) | `http`/`https` only |
| Cookies (`cookie-jar`) | [RFC 10025](https://www.rfc-editor.org/info/rfc10025) (obsoletes 6265) | SameSite stored; browser cross-site send rules N/A; minimal PSL guard |
| Compression (`gzip`) | RFC 1950–1952 | Decode only; single gzip member |

For HTTPS, wrap a TLS-capable socket and set `assume_tls_socket`. `OsBlockingSocket` is TCP only.
