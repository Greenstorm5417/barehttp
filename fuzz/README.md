# Fuzzing barehttp

cargo-fuzz targets under `fuzz/`. CI builds them on push/PR; longer runs are local.

Prerequisites:

```bash
rustup install nightly
cargo install cargo-fuzz
```

| Target | Input |
|--------|--------|
| `parse_response` | Arbitrary bytes → `Response::parse` |
| `parse_uri` | UTF-8 strings → `Uri::parse` |
| `inflate_gzip` | Arbitrary bytes → gzip / zlib / raw inflate (`gzip`) |

```bash
cargo +nightly fuzz build
cargo +nightly fuzz build parse_response

cargo +nightly fuzz run parse_response
cargo +nightly fuzz run parse_uri
cargo +nightly fuzz run inflate_gzip -- -max_total_time=60
```
