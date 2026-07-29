# Fuzzing barehttp

Coverage-guided targets under `fuzz/` (cargo-fuzz).

## Prerequisites

```bash
rustup install nightly
cargo install cargo-fuzz
```

## Targets

| Target | Input |
|--------|--------|
| `parse_response` | Arbitrary bytes → `Response::parse` |
| `parse_uri` | UTF-8 strings → `Uri::parse` |
| `inflate_gzip` | Arbitrary bytes → gzip / zlib / raw inflate (`gzip-decompression`) |

## Build

CI builds these on push/PR:

```bash
cargo +nightly fuzz build
cargo +nightly fuzz build parse_response
```

## Run locally

```bash
cargo +nightly fuzz run parse_response
cargo +nightly fuzz run parse_uri
cargo +nightly fuzz run inflate_gzip -- -max_total_time=60
```
