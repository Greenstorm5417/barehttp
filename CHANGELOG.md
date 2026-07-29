# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-07-29

### Added

- Hand-rolled gzip/deflate inflater (RFC 1951/1952): gzip member, zlib wrapper, raw deflate; `DecompressError`; fixtures, unit tests, proptest props, flate2 differential tests.
- Crate-local busy-wait `sync::Mutex` / `MutexGuard` (replaces `spin`); exponential `spin_loop` backoff while held; `try_lock`.
- `BlockingSocketFactory` and object-safe `BlockingSocket`; OS + stub + mock impls.
- `BlockingSocket::set_connect_timeout`; OS adapters enforce it with nonblocking connect + `poll`/`select` + `SO_ERROR`.
- `InvalidRequest` (`FormAndBody`, `CookieOctet`); `ParseMethodError` for `Method::FromStr`.
- `CookieJar` type alias for `CookieStore`.
- `Headers`: `FromIterator` / `Extend<(String, String)>`.
- Fuzz targets (`inflate_gzip`, `parse_response`, `parse_uri`) under `fuzz/`.
- Integration tests: local mock HTTP server; differential gzip vs flate2; security/malformed parser cases; fragmentation and body-limit transport tests; zstd decompress smoke test.
- `CONTRIBUTING.md` (nextest profiles, fuzz, Miri, test categories).
- Package `homepage` metadata; crate docs pull in `README.md` via `#![doc = include_str!(...)]`.

### Changed

- Cargo features: `gzip-decompression` → `gzip`, `zstd-decompression` → `zstd`. `gzip` is dep-free (no miniz); `zstd` still uses `ruzstd`.
- Dropped runtime deps `miniz_oxide` and `spin`. Dev-deps: `flate2`, `proptest`.
- `Config` defaults: connect **10s**, read/write **30s** (was unlimited).
- `BlockingSocket::is_os_cleartext` default is **`true`** (fail closed); TLS adapters must return `false`.
- `Response::status_code` is primary; `status` is an alias. `#[must_use]` on free/`HttpClient` request builders.
- Chunked body receive: stateful `ChunkedDecoder::feed` + cursor (O(n) over wire bytes). Framing-only scan no longer re-decodes the whole buffer each read.
- Public API (Rust API Guidelines):
  - Private fields + accessors on `Config`, `Response`, `StoredCookie`.
  - `#[non_exhaustive]` on `Error`, `ParseError`, `DnsError`, `SocketError`, `DecompressError`, `Method`.
  - `core::error::Error` + `Display` on error types; `From` bridges into `Error`.
  - `Method`: `Display`, `AsRef<str>`, `FromStr`.
  - Public renames: `Response::header`, `Response::to_text`, `Uri::to_path_and_query`, `Method::needs_request_body`, `CookieStore::request_cookie_header`, `gzip::decompress_raw_deflate`.
  - `CookieStore::iter` yields `&StoredCookie` via named `cookie_jar::Iter` (holds the store lock).
- Examples (`basic`, `agent`, `custom_adapters`, `gzip`, `cookies`) rewritten against real cleartext sites (example.com, httpbingo, postman-echo). Custom adapters forward connect timeout / cleartext.
- CI: push/PR only; cargo-nextest; feature-matrix clippy/test; fuzz; Miri (`sync::`, `gzip::`, `cookie_jar::`); pinned `cargo-nextest` / `cargo-fuzz` via `taiki-e/install-action`; Swatinem rust-cache. Dropped root workspace / `nostd-check` crate.
- Chunked body path: `message_len_if_complete` / `take_trailers`; connection receive avoids a second full decode after the poll loop.
- `duration_ms_u32`: overflow saturates to `u32::MAX` instead of mapping to "no timeout".

### Fixed

- Connect timeout no longer sets write timeout (`SO_SNDTIMEO`).
- EINTR/`WSAEINTR` on connect wait shrinks the remaining deadline instead of restarting the full timeout.
- `MutexGuard`: `Sync` only when `T: Sync` (matches `std::sync::MutexGuard`).
- Winsock: chunk recv/send lengths so `len` cannot truncate/wrap negative; `i32::try_from` for sockaddr / opt lengths.
- FFI `SAFETY` comments on Unix/Windows socket and time helpers.
