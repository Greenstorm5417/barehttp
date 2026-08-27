# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1] - 2026-08-27

### Changed

- Parse and connection paths keep typical header field lists inline (heap `Vec` only from the 9th field); the header scanner uses a stack buffer of refs instead of allocating on every response.
- Buffered `Response::parse` adopts one wire `Bytes`; header spans and the body are subslices (no packed-arena or body copy).
- SWAR byte scans for header-section end / CR/LF; token-char LUT.
- Gzip inflater reserves a tighter output buffer for larger members.
- Request URL is borrowed until a redirect; connect SNI for a DNS name is borrowed (IP literals still allocate). URI recomposition no longer copies the hostname into an extra `String` before `format!`.
- Embedded CI/release checks `cookie-jar`, `zstd`, and `--all-features` on `thumbv7em-none-eabi` (still `no_std` + `alloc`).

## [0.1.0] - 2026-07-29

First crates.io minor after `0.0.1`. Cargo treats `0.0.x → 0.1.0` as a major/breaking bump ([SemVer Compatibility](https://doc.rust-lang.org/cargo/reference/semver.html)).

### Added

- Hand-rolled gzip/deflate inflater (RFC 1951/1952): gzip member, zlib wrapper, raw deflate; `DecompressError`; fixtures, unit tests, proptest props, flate2 differential tests.
- Crate-local busy-wait `sync::Mutex` / `MutexGuard` (replaces `spin`); exponential `spin_loop` backoff while held; `try_lock`.
- `BlockingSocketFactory` and object-safe `BlockingSocket`; OS + stub + mock impls.
- `BlockingSocket::set_connect_timeout`; OS adapters enforce it with nonblocking connect + `poll`/`select` + `SO_ERROR`.
- `InvalidRequest` (`FormAndBody`, `CookieOctet`, `ConnectUnsupported`); `ParseMethodError` for `Method::FromStr`.
- `CookieJar` type alias for `CookieStore`.
- `Headers`: `FromIterator` / `Extend`; owning [`IntoIterator`] (`HeaderIntoIter` → `(String, String)`).
- `WellKnownHeader` + `well_known_header` / `well_known_header_bytes` (ASCII-lower + PHF). `Headers::CONTENT_ENCODING`.
- Dependencies (`no_std` / `alloc`): `bytes`, `phf`, `compact_str`, `hashbrown` (all kept out of the public API). Gzip trailers use `from_le_bytes` / `from_be_bytes` (no `zerocopy`).
- Zero-copy header scanner (`HeaderRef`, `scan_header_fields`); materialize to owned `Headers` only when building the public response.
- Fuzz targets (`inflate_gzip`, `parse_response`, `parse_uri`, structured) under `fuzz/` with dictionaries/corpora.
- Integration / audit tests: mock HTTP server; differential HTTP vs httparse and gzip vs flate2; panic-freedom / alloc-failure; trybuild UI (`tests/ui`); `tests/api_shape.rs`; security/malformed parser; fragmentation and body-limit transport; zstd decompress smoke; Kani harnesses.
- Performance suite under `benches/`: Criterion, Gungraun Callgrind/Cachegrind, dhat-rs; CI Benches job with Callgrind soft limits.
- Rustdoc recovery examples on `Error` (`HttpStatus`, `BodyExceedsLimit`) and `IntoStringError`.
- `CONTRIBUTING.md`; package `homepage`; crate docs pull in `README.md` via `#![doc = include_str!(...)]`.
- README: MSRV **1.90**, primary naming table, intentional limits (buffered / blocking), module-layout note.
- Release workflow: feature pairs, package+packaged tests, cargo-semver-checks, sanitizers, Kani, Docker interop (`scripts/semver-checks.sh`).

### Changed

- Windows cleartext TCP: `OsBlockingSocket::write_vectored` uses `WSASend` (head+body, no concat), matching Unix `writev`.
- Connection receive path: response header section is frozen into the `Headers` arena (spans point into that `Bytes`) instead of copying name/value bytes; non-ASCII (obs-text) values still materialize with lossy UTF-8.
- Cookie jar (`cookie-jar`) cites **RFC 10025** (obsoletes 6265 / 6265bis): `SameSite`, `__Secure-` / `__Host-` prefixes, CTL/size caps, 400-day age limit, host-only in uniqueness, no Secure overlay from cleartext. Browser cross-site SameSite send rules are intentionally not applied.

- Request send: header block and body stay separate (`SerializedRequest`); `Connection::send_request` uses vectored writes (OS TCP `writev`) without concatenating. `BlockingSocket::write_vectored` defaults to `write` for TLS/`&[u8]`-only adapters.
- Cargo features: `gzip-decompression` → `gzip`, `zstd-decompression` → `zstd`. `gzip` is dep-free (no miniz); `zstd` still uses `ruzstd`.
- Dropped runtime deps `miniz_oxide` and `spin`. Dev-deps: `flate2`, `proptest`.
- MSRV: `rust-version` **1.90** (`const fn` APIs, `ruzstd` 0.9, `bincode-next` via gungraun). Fits Kani 0.67's rustc 1.93.
- `Config` defaults: connect `10s`, read/write `30s` (was unlimited).
- `BlockingSocket::is_os_cleartext` default is `true` (fail closed); TLS adapters must return `false`.
- `HttpClient::cookie_store` returns `&CookieStore` (not `&Arc<CookieStore>`).
- `Response::into_bytes()` returns `Vec<u8>` (not `bytes::Bytes`). Primary accessors: `status_code`, `body`; deprecated aliases `status` / `as_bytes`.
- `Response::trailers()` returns `&Headers`. `Response` derives `Hash`.
- `CookieStore::store_response_cookies` returns `Result<(), ParseError>`.
- Primaries: `HttpClient`, `ClientRequestBuilder`, `CookieStore`. Aliases `Agent` / `RequestBuilder` / `CookieJar` stay undeprecated ureq-like synonyms.
- `Method` adds `Options` / `Connect` / `Trace` / `Extension` (no longer `Copy`). `Method::new(impl AsRef<str>)`; unknown tokens → `Extension`; bad `tchar` → `ParseMethodError::InvalidToken`. Execution rejects `Method::Connect` with `InvalidRequest::ConnectUnsupported` (no tunnel API; RFC 9112 authority-form / ignore CL/TE on success).
- UTF-8 helpers: `to_text` → `Utf8Error`, `into_string` → `IntoStringError`; both lift into `Error::Utf8Error`.
- `Headers::from_vec` / `FromIterator` / `Extend` take `AsRef<str>` pairs; `insert` / `set` / builder header APIs same. Side-index via `hashbrown`; public narrative is `&str` / `String` only.
- `Headers::merge_cookie` is `pub(crate)`. Connection pool uses `hashbrown::HashMap` with reusable receive buffers.
- `Error::HttpStatus` and `IntoStringError` box `Response` (small `Result`).
- Public API (Rust API Guidelines): private fields + accessors; `#[non_exhaustive]` on error enums + `Method`; `core::error::Error` + `Display`; `#[must_use]` on builders.
- Module half-nesting (`config` / `request_builder` / feature modules vs root re-exports) documented as intentional.
- Chunked body path: stateful `ChunkedDecoder::feed` + cursor; framing TE/CL / PHF well-known header ids; gzip/DEFLATE inflater speedups (static Huffman tables, bulk bit reader).
- Examples rewritten against cleartext sites. CI: nextest, feature matrix, fuzz, Miri, MSRV 1.90, OS matrix, coverage fail-under.
- Cargo `[lints]`: API-shape lints in `Cargo.toml`; restriction denies stay in `src/lib.rs`.
- `duration_ms_u32`: overflow saturates to `u32::MAX`.

### Fixed

- Response header field values reject NUL and other dangerous CTLs (RFC 9110); previously some were accepted.
- Connect timeout no longer sets write timeout (`SO_SNDTIMEO`).
- EINTR/`WSAEINTR` on connect wait shrinks the remaining deadline instead of restarting the full timeout.
- `MutexGuard`: `Sync` only when `T: Sync` (matches `std::sync::MutexGuard`).
- Winsock: chunk recv/send lengths so `len` cannot truncate/wrap negative; `i32::try_from` for sockaddr / opt lengths.
- FFI `SAFETY` comments on Unix/Windows socket and time helpers.
