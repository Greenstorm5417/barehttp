# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Integration test `tests/api_shape.rs` locking post-audit public API contracts (trait impls, cookie-jar signatures, error lifting, response UTF-8 helpers, primary naming).
- `Headers` owning [`IntoIterator`] (`HeaderIntoIter` → `(String, String)`), matching `&Headers` / `iter`.
- Dependencies (`no_std` / `alloc`): `bytes` (`default-features = false`), `phf` (`default-features = false`, `macros`) for well-known header name → id maps, `compact_str` (SSO header name/value storage), `hashbrown` (header side-index + pool). Gzip trailers stay `from_le_bytes` / `from_be_bytes` (no `zerocopy`).
- `WellKnownHeader` + `well_known_header` / `well_known_header_bytes` (ASCII-lower + PHF). `Headers::CONTENT_ENCODING`.
- Zero-copy header scanner (`HeaderRef`, `scan_header_fields`); materialize to owned `Headers` only when building the public response. Connection path runs framing on borrowed refs first.
- Performance suite under `benches/`: Criterion, Gungraun Callgrind/Cachegrind, dhat-rs.
- CI Benches job: compile all benches; Callgrind soft limits (+5% Ir / +10% EstimatedCycles); Criterion smoke; dhat smoke.
- Rustdoc recovery examples on `Error` (`HttpStatus`, `BodyExceedsLimit`) and `IntoStringError`.
- README: MSRV (`rust-version` **1.97**), primary naming table, intentional limits (buffered / blocking), module-layout note.

### Changed

- **Breaking:** `HttpClient::cookie_store` returns `&CookieStore` (not `&Arc<CookieStore>`). Call sites that only called methods on the store need no change (`Deref` already worked); callers that needed an `Arc` should clone the client or wrap the store themselves.
- `Response` derives `Hash` (body + headers + trailers; matches `PartialEq`).
- Connection pool uses `hashbrown::HashMap` (foldhash) instead of `BTreeMap`. Idle entries carry reusable receive `BytesMut` + read scratch; connections reuse those buffers across reads and pooled hops (public `Response` lifetime unchanged).
- **Breaking:** `Response::into_bytes()` returns `Vec<u8>` (not `bytes::Bytes`). Primary body accessor is `body()`; `as_bytes()` remains as a **deprecated** alias. `bytes` stays an internal dependency only — no public re-export.
- **Breaking:** `CookieStore::store_response_cookies` returns `Result<(), ParseError>` (`InvalidUri` on unusable URI). Malformed individual `Set-Cookie` values still skipped. `request_cookie_header` still returns `""` for invalid URIs (documented).
- API naming polish (compat kept):
  - Primaries: `HttpClient`, `ClientRequestBuilder`, `CookieStore`, `status_code`, `body`.
  - Type aliases `Agent` / `RequestBuilder` / `CookieJar` stay **undeprecated** ureq-like synonyms.
  - Method aliases `Response::status` / `Response::as_bytes`: `#[deprecated]` (no longer merely `doc(hidden)`).
  - `Uri::query` public accessor: removed inappropriate `#[allow(dead_code)]`.
  - `#[must_use]` on `ClientRequestBuilder` chain methods (config builder already had them).
- Module half-nesting (`config` / `request_builder` / feature modules vs root re-exports) documented as intentional (WAIVE), not flattened.
- **Breaking:** `Response::trailers()` returns `&Headers` (not `&[(String, String)]`). Chunked trailers use the same map type as response headers.
- **Breaking:** `Method` adds `Options` / `Connect` / `Trace` / `Extension` (RFC 9110 tokens via opaque `ExtensionMethod`). No longer `Copy` (extension storage). `Method::new(impl AsRef<str>)`; `ParseMethodError::Unknown` → `InvalidToken` (invalid `tchar` only; unknown tokens become `Extension`).
- UTF-8 body helpers: `to_text` → `Utf8Error`, `into_string` → `IntoStringError` (response recoverable); both `From` into `Error::Utf8Error`. `IntoStringError` derives `Clone`/`PartialEq`/`Eq`. `DecompressError` docs: always at crate root; `gzip` module is feature-gated.
- **Breaking:** `Headers::from_vec` / `FromIterator` / `Extend` accept `AsRef<str>` pairs (not only `(String, String)`). `into_vec` still returns `Vec<(String, String)>` as the owned export.
- **Breaking:** `Headers::merge_cookie` is `pub(crate)` (builder/client plumbing).
- `Headers` implements `Hash` (fields only; side-index is a cache, matching `PartialEq`/`Eq`).
- `well_known_header_bytes` re-exported at the crate root alongside `well_known_header`.
- Public `Headers` docs speak `&str` / `String` only (internal SSO storage not part of the narrative).
- `RawResponse.body_bytes` and request wire output (`serialize_request` / `build_request`) use `Bytes` / `BytesMut` on the connection read and serialize paths.
- Framing / wire / Accept-Encoding / Connection checks use PHF well-known header ids. Arbitrary headers remain supported.
- Header parsing builds `Headers` in one pass (no double string materialization).
- `Headers::set` replaces matching fields in place.
- Fixed DEFLATE Huffman tables embedded as static data (no runtime build / leak cache).
- Header value UTF-8 fast path; `body_read_strategy` single header pass; chunked output reserve (capped); percent-encode hex nibble table.
- Gzip/DEFLATE: `u64` bit reader with bulk refill, packed Huffman tables, specialized fixed-block inflate, faster `copy_match` / CRC-32 / Adler-32.
- Buffered `Response::parse` materializes owned headers in one pass. Framing TE/CL uses direct case-insensitive compare (no PHF per field). Byte Content-Length / TE token parse; stack decimal for injected `Content-Length`; `BytesMut` reserves on receive.
- `Headers` stores name/value pairs with a private lowercase→first-index `hashbrown` map (`Option<Box<_>>`) for `get`/`contains`. Public `insert` / `set` (and request-builder `header` / `set_header` / `content_type`) take `impl AsRef<str>`. Materialize writes compact storage directly (`from_utf8_lossy` for values).
- After materialize, rebuild the header side-index in batch; skip the side-index below 8 fields; alloc-free case-insensitive lookup (`Equivalent`); `set` avoids a full rebuild on append or a single match.
- `Error::HttpStatus` boxes its `Response` so `Error` stays small for `Result` call sites.
- `IntoStringError` boxes its `Response` so `Result<String, IntoStringError>` stays small (`clippy::result_large_err` / C-GOOD-ERR).
- Cargo `[lints]`: API-shape lints only (`ptr_arg`, `missing_safety_doc`, …). Restriction denies stay in `src/lib.rs` so CI `-D warnings` does not apply them to tests/benches/examples.

### Notes

- Optional public-API drift check: `cargo public-api` / `cargo-semver-checks` are **not** required in CI; install locally if useful (see CONTRIBUTING).

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
