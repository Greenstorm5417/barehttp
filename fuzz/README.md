# Fuzzing barehttp

cargo-fuzz targets under `fuzz/`. CI runs each target for a short campaign
(`FUZZ_MAX_TOTAL_TIME`, default **60** seconds). Release / nightly jobs may set
a longer value; pass it through to libFuzzer:

```bash
export FUZZ_MAX_TOTAL_TIME="${FUZZ_MAX_TOTAL_TIME:-60}"
cargo +nightly fuzz run TARGET -- -max_total_time="$FUZZ_MAX_TOTAL_TIME"
```

Prerequisites:

```bash
rustup install nightly
cargo install cargo-fuzz
```

## Targets

| Target | Input | Dictionary |
|--------|--------|------------|
| `parse_response` | Arbitrary bytes → `Response::parse` | `dictionaries/http.dict` |
| `parse_response_structured` | `arbitrary` status/headers/framing → parse (+ raw) | `dictionaries/http.dict` |
| `parse_uri` | UTF-8 → `Uri::parse` | `dictionaries/uri.dict` |
| `inflate_gzip` | Bytes → gzip / zlib / raw inflate (`gzip` feature) | `dictionaries/gzip.dict` |

Curated seeds live under `fuzz/corpus/<target>/seed_*` (tracked). Runtime discoveries,
`fuzz/artifacts/`, `fuzz/target/`, and coverage outputs are gitignored (see root
`.gitignore` and `fuzz/.gitignore`).

Dictionary syntax: AFL/libFuzzer accepts `\xHH` hex escapes only. Do not use `"\r\n"`
(rejected at startup).

## Commands

```bash
# If cargo-fuzz was installed via binstall / a musl prebuilt, pass the GNU host
# triple (CI does this). `cargo install cargo-fuzz` from source usually needs no flag.
cargo +nightly fuzz build --target x86_64-unknown-linux-gnu
cargo +nightly fuzz build --target x86_64-unknown-linux-gnu parse_response

FUZZ_MAX_TOTAL_TIME="${FUZZ_MAX_TOTAL_TIME:-60}"

cargo +nightly fuzz run --target x86_64-unknown-linux-gnu parse_response -- \
  -max_total_time="$FUZZ_MAX_TOTAL_TIME" -dict=fuzz/dictionaries/http.dict

cargo +nightly fuzz run --target x86_64-unknown-linux-gnu parse_response_structured -- \
  -max_total_time="$FUZZ_MAX_TOTAL_TIME" -dict=fuzz/dictionaries/http.dict

cargo +nightly fuzz run --target x86_64-unknown-linux-gnu parse_uri -- \
  -max_total_time="$FUZZ_MAX_TOTAL_TIME" -dict=fuzz/dictionaries/uri.dict

cargo +nightly fuzz run --target x86_64-unknown-linux-gnu inflate_gzip -- \
  -max_total_time="$FUZZ_MAX_TOTAL_TIME" -dict=fuzz/dictionaries/gzip.dict
```

Feature note: the fuzz package enables `barehttp` with `gzip` so inflate and
content-encoding paths are exercised. CI does not need separate feature-combo
targets; add more `[[bin]]` entries if you want `cookie-jar` / `zstd` isolation later.
