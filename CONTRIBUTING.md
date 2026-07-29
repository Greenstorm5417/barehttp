# Contributing

## Tests

```bash
cargo install cargo-nextest --locked   # once locally
cargo nextest run --all-features
cargo nextest run --lib --features gzip-decompression
cargo nextest run --test '*'           # integration tests only
```

Profiles: `.config/nextest.toml` (`default`, `ci`).

`cargo test` works too; CI uses nextest.

## CI tooling

CI installs pinned `cargo-nextest` / `cargo-fuzz` via [`taiki-e/install-action`](https://github.com/taiki-e/install-action) (prebuilt binaries, not `cargo install` each run). Versions live in `.github/workflows/ci.yml` (`NEXTEST_VERSION`, `CARGO_FUZZ_VERSION`). Cargo build artifacts use [`Swatinem/rust-cache`](https://github.com/Swatinem/rust-cache). Miri comes from the nightly toolchain component.

## Test categories

| Category | How to run |
|----------|------------|
| Unit (`#[cfg(test)]`) | `cargo nextest run --lib --all-features` |
| Fragmentation / streaming | `--lib` (`transport::tests::test_fragmentation`) |
| Malformed / security | `--lib` (`parser::tests::security`, `rfc9112`) |
| Property-based (proptest) | `--lib` + `--features gzip-decompression` for gzip props; also `tests/differential_gzip.rs` |
| Differential (vs flate2) | `cargo nextest run --features gzip-decompression -E 'test(differential)'` |
| Integration (local mock HTTP) | `cargo nextest run --test mock_http_server --features gzip-decompression` |
| Resource / body limits | `--lib` (`transport::tests::test_limits`) + mock server body-limit test |
| Decompression bombs | `tests/differential_gzip.rs` + gzip unit stored-block bomb |
| Live httpbin | ignored: `cargo test --test httpbin_test -- --ignored` |

## Fuzzing

See [`fuzz/README.md`](fuzz/README.md). CI builds fuzz targets on push/PR; longer runs are local.

## Miri

```bash
rustup toolchain install nightly --component miri
MIRIFLAGS='-Zmiri-strict-provenance' cargo +nightly miri test --lib sync::
MIRIFLAGS='-Zmiri-strict-provenance' cargo +nightly miri test --lib --features gzip-decompression gzip::
```

## Features / embedded

The crate is `#![no_std]` + `alloc`. Host feature matrix and an embedded target check:

```bash
cargo check --no-default-features
cargo check --all-features
cargo check --target thumbv7em-none-eabi --no-default-features
```

## Style

- Keep the library `no_std` + `alloc`. `std` belongs in `tests/`, `examples/`, `fuzz/`, and `#[cfg(test)]` where needed.
- Do not add runtime `miniz_oxide` / `spin`; gzip stays hand-rolled behind `gzip-decompression`.
