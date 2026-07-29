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

Pinned tools are installed via [`taiki-e/install-action`](https://github.com/taiki-e/install-action) (prebuilt binaries). Versions live in workflow `env:`:

| Env | Tool |
|-----|------|
| `NEXTEST_VERSION` | cargo-nextest |
| `CARGO_FUZZ_VERSION` | cargo-fuzz |
| `LLVM_COV_VERSION` | cargo-llvm-cov |
| `CARGO_DENY_VERSION` | cargo-deny |
| `CARGO_SEMVER_CHECKS_VERSION` | cargo-semver-checks (release only) |
| `FUZZ_MAX_TOTAL_TIME` | libFuzzer `-max_total_time` (CI `60`, release `300` default) |
| `COVERAGE_FAIL_UNDER` | llvm-cov line % fail-under (also `coverage_baseline.toml`) |

Cargo build artifacts use [`Swatinem/rust-cache`](https://github.com/Swatinem/rust-cache). Miri / sanitizers use nightly toolchain components.

## Regular CI vs release CI

| Job / check | Regular CI (`.github/workflows/ci.yml`) | Release (`.github/workflows/release.yml`) |
|-------------|-------------------------------------------|---------------------------------------------|
| Feature matrix + clippy + nextest | yes | yes (extra combos) |
| Embedded `thumbv7em-none-eabi` | yes | yes |
| Miri (focused) | yes | yes |
| Fuzz | 60s / target | longer (`FUZZ_MAX_TOTAL_TIME`, default 300) |
| cargo-deny | yes | yes |
| Coverage (llvm-cov) | yes | yes + artifacts |
| trybuild / `tests/ui` | yes | yes |
| fmt / docs / examples / lockfile | yes | yes |
| Benches | smoke (short Criterion, Callgrind, dhat) | full (longer Criterion, Callgrind+Cachegrind, dhat, adversarial if present) |
| MSRV 1.97 | yes | yes |
| OS matrix (Windows / macOS) | yes | yes |
| Docker interop | **no** | yes (`scripts/run-interop.sh`) |
| ASan / LSan | **no** | yes (nightly `-Zbuild-std`) |
| MSan | **no** | **no** (impractical with OS FFI; see below) |
| Kani | **no** | yes (toolchain; proofs when present) |
| `cargo package` + test packaged tree | **no** | yes |
| cargo-semver-checks | **no** | yes (skips on first crates.io release) |
| Auto-publish crates.io | never | never |

Concurrency:

- **CI:** cancel in-progress runs for pull requests only.
- **Release:** `cancel-in-progress: false` (never cancel tag/release runs).

## Test categories

| Category | How to run |
|----------|------------|
| Unit (`#[cfg(test)]`) | `cargo nextest run --lib --all-features` |
| Fault injection (scripted transport) | `--lib` (`transport::tests::test_fault_injection`) |
| Partial writes | `--lib` (`transport::tests::test_partial_writes`) |
| Fragmentation / streaming | `--lib` (`transport::tests::test_fragmentation`) |
| Network lifecycle / pooling | `cargo nextest run --test network_lifecycle --features gzip` |
| RFC / regression corpus | `cargo nextest run --test corpus_runner --features gzip` |
| Malformed / security | `--lib` (`parser::tests::security`, `rfc9112`) |
| Property-based (proptest) | `--lib` + `--features gzip`; also `tests/properties_http.rs`, `tests/differential_gzip.rs` |
| Differential (vs flate2 / httparse) | `cargo nextest run --features gzip -E 'test(differential)'` (httparse oracle for headers/status; hyper is a full client stack, so it is omitted) |
| Panic-freedom | `cargo nextest run -E 'test(panic)'` |
| trybuild UI | `cargo test --test ui` (see `tests/ui/`) |
| Alloc / body-limit audit | `cargo nextest run -E 'test(alloc_failure)'` + `--lib` `test_limits` |
| Integration (local mock HTTP) | `cargo nextest run --test mock_http_server --features gzip` |
| Resource / body limits | `--lib` (`transport::tests::test_limits`) + mock server body-limit test |
| Decompression bombs | `tests/differential_gzip.rs` + gzip unit stored-block bomb |
| Live httpbin | ignored: `cargo test --test httpbin_test -- --ignored` |
| Kani | `cargo kani --all-features` (`src/parser/kani_proofs.rs` + `cfg(kani)` modules) |
| SemVer checks | [`scripts/semver-checks.sh`](scripts/semver-checks.sh) (skips before first crates.io baseline) |
| API compile / trybuild | `cargo test --test ui` |
| Coverage (line/function/region; optional branch) | `bash scripts/coverage.sh` (see Coverage below) |
| Docker interop | `bash scripts/run-interop.sh` (needs Docker; `BAREHTTP_INTEROP=1`) |
| cargo-deny | `cargo deny check advisories licenses bans` |
| Packaging | `cargo package --locked` then test under `target/package/barehttp-*` |

## Coverage

`bash scripts/coverage.sh` (needs `llvm-tools-preview`, `cargo-llvm-cov`, nextest) writes
`target/llvm-cov/{lcov.info,cobertura.xml,html/,summary.txt}`.

Defaults / fail-under: `coverage_baseline.toml` and CI env `COVERAGE_FAIL_UNDER` (~70% lines).
Overrides: `COVERAGE_FAIL_UNDER`, `COVERAGE_FAIL_UNDER_FUNCTIONS`, `COVERAGE_FAIL_UNDER_REGIONS`,
`COVERAGE_BRANCH=1` (unstable branch instrumentation).

`scripts/coverage.sh` ignores Windows OS bindings and the non-unix/non-windows socket stub
(`--ignore-filename-regex`). Expected gaps: OS socket/DNS FFI, ignored live httpbin, Docker
interop (unless compose is up), fuzz crate, benches.

## Docker interop

Release / nightly only. Pinned images in `docker-compose.interop.yml`:

- `nginx:1.27-alpine` → `:18080`
- `httpd:2.4-alpine` → `:18081`
- `caddy:2.8-alpine` → `:18082`
- `python:3.12-alpine` → `:18083`
- `node:22-alpine` → `:18084`
- Axum (`rust:1.85-bookworm` build) → `:18085`
- `haproxy:2.9-alpine` (fronts nginx) → `:18086`

```bash
bash scripts/run-interop.sh
# or manually:
docker compose -f docker-compose.interop.yml up -d --build
BAREHTTP_INTEROP=1 cargo nextest run --features gzip --test interop_client
docker compose -f docker-compose.interop.yml down -v
```

Deterministic endpoints: `/plain`, `/chunked`, `/gzip`, `/headers`, `/status/404`, `/close`, `/http10`.

## Benchmarks / performance

Library stays `no_std` + `alloc`; benches use `std`. Sources live under `benches/`.

```bash
cargo bench --locked --no-run --all-features

# Criterion
cargo bench --bench criterion_hot_paths --all-features
cargo bench --bench criterion_e2e --all-features
cargo bench --bench criterion_adversarial --all-features
# short smoke (matches regular CI):
cargo bench --bench criterion_hot_paths --all-features -- \
  --warm-up-time 0.2 --measurement-time 0.5 --sample-size 10
BAREHTTP_ADV_SAMPLE_SIZE=10 BAREHTTP_ADV_MEASURE_SECS=0.5 \
  cargo bench --bench criterion_adversarial --all-features -- --warm-up-time 0.2

# Gungraun (Linux only; Valgrind + gungraun-runner; needs feature bench-gungraun)
cargo bench --bench gungraun_callgrind --all-features
cargo bench --bench gungraun_cachegrind --all-features

# dhat allocation smoke
BAREHTTP_DHAT_ITERS=5 cargo bench --bench dhat_parser --all-features
BAREHTTP_DHAT_ITERS=5 cargo bench --bench dhat_gzip --features gzip
BAREHTTP_DHAT_ITERS=5 cargo bench --bench dhat_e2e --all-features
```

Regular CI compiles all benches, Callgrind soft limits (+5% Ir / +10% EstimatedCycles vs cache), short Criterion + dhat smokes. Release runs longer Criterion, Cachegrind, higher dhat iters, and an `adversarial` bench target when present.

## Fuzzing

See [`fuzz/README.md`](fuzz/README.md). CI caches `fuzz/corpus` + `fuzz/artifacts`.
Commit curated `fuzz/corpus/*/seed_*` only; runtime discoveries, crashes, and
`fuzz/target/` stay gitignored.

```bash
cargo +nightly fuzz run --target x86_64-unknown-linux-gnu parse_response -- -max_total_time=60
```

## Miri

```bash
rustup toolchain install nightly --component miri
MIRIFLAGS='-Zmiri-strict-provenance' cargo +nightly miri test --lib sync::
MIRIFLAGS='-Zmiri-strict-provenance' cargo +nightly miri test --lib --features gzip gzip::
MIRIFLAGS='-Zmiri-strict-provenance' cargo +nightly miri test --lib --features cookie-jar cookie_jar::
```

## Sanitizers (release)

ASan and LSan on Linux nightly with `-Zbuild-std`:

```bash
rustup toolchain install nightly --component rust-src
RUSTFLAGS='-Zsanitizer=address -C force-frame-pointers=yes' \
  cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu --lib --all-features
RUSTFLAGS='-Zsanitizer=leak -C force-frame-pointers=yes' \
  cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu --lib --all-features
```

**MSan** is not run: sanitized libc + OS socket/DNS FFI is impractical for this crate. Prefer ASan/LSan + Miri for memory issues.

## Kani (release)

```bash
cargo install --locked kani-verifier
cargo kani setup
# Proofs are behind cfg(kani) in src/parser/ (kani_proofs.rs + module-local harnesses):
cargo kani --all-features
```

## Fault injection

Local mock servers under `tests/` / `tests/support/` inject short writes, closes, and malformed framing. Prefer extending those over adding runtime deps.

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
- Do not add runtime test dependencies to the library graph.

## Packaging / release

Release workflow runs `cargo deny check`, `cargo package --locked`, tests the extracted package, docs (`RUSTDOCFLAGS=-D warnings`), and uploads coverage / fuzz / bench artifacts. **It does not publish to crates.io.**

`cargo-semver-checks`: the first `0.1.0` crates.io publish has no registry baseline.
The release job and `scripts/semver-checks.sh` skip, or use `--baseline-rev` of a prior git tag.

## Optional public-API tooling

```bash
cargo install cargo-public-api --locked   # optional
cargo public-api
cargo install cargo-semver-checks --locked
./scripts/semver-checks.sh
```
