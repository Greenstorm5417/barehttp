#!/usr/bin/env bash
# Generate LCOV + Cobertura + HTML coverage via cargo-llvm-cov + nextest.
# Reports lines / functions / regions (+ optional unstable branch).
# Fail if coverage drops below thresholds in coverage_baseline.toml / env.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

OUT_DIR="${COVERAGE_OUT_DIR:-target/llvm-cov}"
mkdir -p "$OUT_DIR"

read_toml_int() {
  local key="$1"
  local default="$2"
  if [[ -f coverage_baseline.toml ]]; then
    local v
    v="$(
      sed -n "s/^[[:space:]]*${key}[[:space:]]*=[[:space:]]*\\([0-9][0-9]*\\).*/\\1/p" \
        coverage_baseline.toml | head -n1
    )"
    if [[ -n "$v" ]]; then
      echo "$v"
      return
    fi
  fi
  echo "$default"
}

FAIL_LINES="${COVERAGE_FAIL_UNDER:-$(read_toml_int fail_under_lines 70)}"
FAIL_FUNCS="${COVERAGE_FAIL_UNDER_FUNCTIONS:-$(read_toml_int fail_under_functions 60)}"
FAIL_REGIONS="${COVERAGE_FAIL_UNDER_REGIONS:-$(read_toml_int fail_under_regions 55)}"
BRANCH="${COVERAGE_BRANCH:-0}"

# Paths that cannot execute on the Linux CI host (see CONTRIBUTING.md Coverage).
IGNORE_RE='src/socket/os/windows\.rs|src/dns/os/windows\.rs|src/socket/os/stub\.rs'

BRANCH_ARGS=()
if [[ "$BRANCH" == "1" || "$BRANCH" == "true" ]]; then
  BRANCH_ARGS=(--branch)
  echo "==> branch coverage enabled (unstable)"
fi

echo "==> llvm-cov nextest (collect; all-features)"
# Use NEXTEST_PROFILE: cargo-llvm-cov steals `--profile` (Cargo profile), and
# args after `--` are test-binary args, not nextest options.
NEXTEST_PROFILE="${NEXTEST_PROFILE:-ci}" cargo llvm-cov nextest \
  --locked \
  --all-features \
  --no-report \
  "${BRANCH_ARGS[@]}"

echo "==> report LCOV"
cargo llvm-cov report \
  --ignore-filename-regex "${IGNORE_RE}" \
  --lcov \
  --output-path "${OUT_DIR}/lcov.info"

echo "==> report Cobertura"
cargo llvm-cov report \
  --ignore-filename-regex "${IGNORE_RE}" \
  --cobertura \
  --output-path "${OUT_DIR}/cobertura.xml"

echo "==> report HTML"
cargo llvm-cov report \
  --ignore-filename-regex "${IGNORE_RE}" \
  --html \
  --output-dir "${OUT_DIR}/html"

echo "==> summary + fail-under (lines=${FAIL_LINES}% functions=${FAIL_FUNCS}% regions=${FAIL_REGIONS}%)"
# Thresholds applied once after artifacts exist so CI can still upload HTML/LCOV on failure.
cargo llvm-cov report \
  --ignore-filename-regex "${IGNORE_RE}" \
  --fail-under-lines "${FAIL_LINES}" \
  --fail-under-functions "${FAIL_FUNCS}" \
  --fail-under-regions "${FAIL_REGIONS}" \
  --summary-only \
  | tee "${OUT_DIR}/summary.txt"

echo "==> coverage artifacts under ${OUT_DIR}/"
ls -la "${OUT_DIR}"
