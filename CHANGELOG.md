# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Dependencies (`no_std` / `alloc`): `bytes` (`default-features = false`), `phf` (`default-features = false`, `macros`) for well-known header name → id maps, `compact_str` (SSO header name/value storage), `hashbrown` (header side-index + pool). Gzip trailers stay `from_le_bytes` / `from_be_bytes` (no `zerocopy`).
- `WellKnownHeader` + `well_known_header` / `well_known_header_bytes` (ASCII-lower + PHF). `Headers::CONTENT_ENCODING`.
- Zero-copy header scanner (`HeaderRef`, `scan_header_fields`); materialize to owned `Headers` only when building the public response. Connection path runs framing on borrowed refs first.
- Performance suite under `benches/`: Criterion, Gungraun Callgrind/Cachegrind, dhat-rs.
- CI Benches job: compile all benches; Callgrind soft limits (+5% Ir / +10% EstimatedCycles); Criterion smoke; dhat smoke.

### Changed

- Connection pool uses `hashbrown::HashMap` (foldhash) instead of `BTreeMap`. Idle entries carry reusable receive `BytesMut` + read scratch; connections reuse those buffers across reads and pooled hops (public `Response` lifetime unchanged).
- **Breaking:** `Response::into_bytes()` returns `bytes::Bytes` (not `Vec<u8>`). `body()` / `as_bytes()` remain `&[u8]`. Re-exported as `barehttp::Bytes`.
- `RawResponse.body_bytes` and request wire output (`serialize_request` / `build_request`) use `Bytes` / `BytesMut` on the connection read and serialize paths.
- Framing / wire / Accept-Encoding / Connection checks use PHF well-known header ids. Arbitrary headers remain supported.
- Header parsing builds `Headers` in one pass (no double string materialization).
- `Headers::set` replaces matching fields in place.
- Fixed DEFLATE Huffman tables embedded as static data (no runtime build / leak cache).
- Header value UTF-8 fast path; `body_read_strategy` single header pass; chunked output reserve (capped); percent-encode hex nibble table.
- Gzip/DEFLATE: `u64` bit reader with bulk refill, packed Huffman tables, specialized fixed-block inflate, faster `copy_match` / CRC-32 / Adler-32.
- Buffered `Response::parse` materializes owned headers in one pass. Framing TE/CL uses direct case-insensitive compare (no PHF per field). Byte Content-Length / TE token parse; stack decimal for injected `Content-Length`; `BytesMut` reserves on receive.
- `Headers` stores `(CompactString, CompactString)` with a private lowercase→first-index `hashbrown` map (`Option<Box<_>>`) for `get`/`contains`. Public API still exposes `&str` / `(String, String)` at the edges. Materialize writes `CompactString` directly (`from_utf8_lossy` for values, no intermediate `String`).
- After materialize, rebuild the header side-index in batch; skip the side-index below 8 fields; alloc-free case-insensitive lookup (`Equivalent`); `set` avoids a full rebuild on append or a single match.
- `Error::HttpStatus` boxes its `Response` so `Error` stays small for `Result` call sites.

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
- `Config` defaults: connect `10s`, read/write `30s` (was unlimited).
- `BlockingSocket::is_os_cleartext` default is `true` (fail closed); TLS adapters must return `false`.
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
