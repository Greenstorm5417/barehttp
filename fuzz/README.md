# Fuzzing barehttp

cargo-fuzz targets under `fuzz/`. CI runs each target for 60s on push/PR; longer campaigns are local.

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
# If cargo-fuzz was installed via binstall / a musl prebuilt, pass the GNU host
# triple (CI does this). `cargo install cargo-fuzz` from source usually needs no flag.
cargo +nightly fuzz build --target x86_64-unknown-linux-gnu
cargo +nightly fuzz build --target x86_64-unknown-linux-gnu parse_response

cargo +nightly fuzz run --target x86_64-unknown-linux-gnu parse_response
cargo +nightly fuzz run --target x86_64-unknown-linux-gnu parse_uri
cargo +nightly fuzz run --target x86_64-unknown-linux-gnu inflate_gzip -- -max_total_time=60
```
