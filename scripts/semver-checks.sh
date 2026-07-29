#!/usr/bin/env bash
# Run cargo-semver-checks against crates.io / an optional baseline root or git rev.
# No registry baseline before the first publish: skip gracefully
# (or set BAREHTTP_SEMVER_BASELINE_REV / a prior git tag).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

if ! command -v cargo-semver-checks >/dev/null 2>&1; then
  echo "cargo-semver-checks not installed."
  echo "  cargo install cargo-semver-checks --locked"
  exit 1
fi

BASELINE_ROOT="${BAREHTTP_SEMVER_BASELINE_ROOT:-}"
if [[ -n "$BASELINE_ROOT" ]]; then
  echo "Checking against --baseline-root=$BASELINE_ROOT"
  exec cargo semver-checks check-release --baseline-root "$BASELINE_ROOT"
fi

BASELINE_REV="${BAREHTTP_SEMVER_BASELINE_REV:-}"
if [[ -n "$BASELINE_REV" ]]; then
  echo "Checking against --baseline-rev=$BASELINE_REV"
  exec cargo semver-checks check-release --baseline-rev "$BASELINE_REV"
fi

set +e
cargo semver-checks check-release 2>/tmp/barehttp-semver-err.txt
status=$?
set -e
if [[ "$status" -eq 0 ]]; then
  echo "semver-checks vs crates.io baseline: ok"
  exit 0
fi

if grep -qiE 'not found on crates.io|no.*baseline|does not exist|failed to get.*crate' \
  /tmp/barehttp-semver-err.txt 2>/dev/null; then
  PREV_TAG="$(git describe --tags --abbrev=0 HEAD^ 2>/dev/null || true)"
  if [[ -n "${PREV_TAG}" ]]; then
    echo "No crates.io baseline; using --baseline-rev ${PREV_TAG}"
    exec cargo semver-checks check-release --baseline-rev "${PREV_TAG}"
  fi
  echo "First release / no prior tag or crates.io version; skipping cargo-semver-checks."
  cat /tmp/barehttp-semver-err.txt || true
  exit 0
fi

cat /tmp/barehttp-semver-err.txt
exit "$status"
