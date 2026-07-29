# Contributing

## Tests

```bash
cargo install cargo-nextest --locked   # once locally
cargo nextest run --all-features
cargo nextest run --lib --features gzip
cargo nextest run --test '*'           # integration tests only
```

Profiles: `.config/nextest.toml` (`default`, `ci`).

`cargo test` works too; CI uses nextest.

## CI tooling

CI installs pinned `cargo-nextest` / `cargo-fuzz` via [`taiki-e/install-action`](https://github.com/taiki-e/install-action) (prebuilt binaries each run, not `cargo install`). Versions live in `.github/workflows/ci.yml` (`NEXTEST_VERSION`, `CARGO_FUZZ_VERSION`). Cargo build artifacts use [`Swatinem/rust-cache`](https://github.com/Swatinem/rust-cache). Miri comes from the nightly toolchain component.

## Test categories

| Category | How to run |
|----------|------------|
| Unit (`#[cfg(test)]`) | `cargo nextest run --lib --all-features` |
| Fragmentation / streaming | `--lib` (`transport::tests::test_fragmentation`) |
| Malformed / security | `--lib` (`parser::tests::security`, `rfc9112`) |
| Property-based (proptest) | `--lib` + `--features gzip` for gzip props; also `tests/differential_gzip.rs` |
| Differential (vs flate2) | `cargo nextest run --features gzip -E 'test(differential)'` |
| Integration (local mock HTTP) | `cargo nextest run --test mock_http_server --features gzip` |
| Resource / body limits | `--lib` (`transport::tests::test_limits`) + mock server body-limit test |
| Decompression bombs | `tests/differential_gzip.rs` + gzip unit stored-block bomb |
| Live httpbin | ignored: `cargo test --test httpbin_test -- --ignored` |

## Benchmarks / performance

Library stays `no_std` + `alloc`; benches use `std`. Sources live under `benches/`.

```bash
cargo bench --locked --no-run --all-features

# Criterion
cargo bench --bench criterion_hot_paths --all-features
cargo bench --bench criterion_e2e --all-features
# short smoke (matches CI):
cargo bench --bench criterion_hot_paths --all-features -- \
  --warm-up-time 0.2 --measurement-time 0.5 --sample-size 10

# Gungraun (Linux only; Valgrind + gungraun-runner; needs feature bench-gungraun)
cargo bench --bench gungraun_callgrind --all-features
cargo bench --bench gungraun_cachegrind --all-features

# dhat allocation smoke
BAREHTTP_DHAT_ITERS=5 cargo bench --bench dhat_parser --all-features
BAREHTTP_DHAT_ITERS=5 cargo bench --bench dhat_gzip --features gzip
BAREHTTP_DHAT_ITERS=5 cargo bench --bench dhat_e2e --all-features
```

CI job Benches compiles all targets. Callgrind soft limits are +5% Ir / +10% EstimatedCycles vs the cached baseline. Criterion and dhat allocation smokes also run.

## Fuzzing

See [`fuzz/README.md`](fuzz/README.md).

## Miri

```bash
rustup toolchain install nightly --component miri
MIRIFLAGS='-Zmiri-strict-provenance' cargo +nightly miri test --lib sync::
MIRIFLAGS='-Zmiri-strict-provenance' cargo +nightly miri test --lib --features gzip gzip::
MIRIFLAGS='-Zmiri-strict-provenance' cargo +nightly miri test --lib --features cookie-jar cookie_jar::
```

## no_std / embedded

The crate is `#![no_std]` + `alloc`. Check combinations and the embedded target:

```bash
cargo check --no-default-features
cargo check --all-features
cargo check --target thumbv7em-none-eabi --no-default-features
```

## Style

- Keep the library `no_std` + `alloc`. Put `std` only in `tests/`, `examples/`, `fuzz/`, `benches/`, or `#[cfg(test)]`.
- Do not add runtime `miniz_oxide` / `spin`; gzip stays hand-rolled behind `gzip`.
