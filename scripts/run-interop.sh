#!/usr/bin/env bash
# Start docker-compose.interop.yml, wait for readiness, run interop_client tests, tear down.
# Release / nightly workflow only.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

COMPOSE_FILE="${COMPOSE_FILE:-docker-compose.interop.yml}"
LOG_DIR="${INTEROP_LOG_DIR:-target/interop-logs}"
mkdir -p "$LOG_DIR"

if ! command -v docker >/dev/null 2>&1; then
  echo "error: docker is required for interop (not available in this environment)" >&2
  exit 1
fi

COMPOSE=(docker compose -f "$COMPOSE_FILE")
if ! docker compose version >/dev/null 2>&1; then
  COMPOSE=(docker-compose -f "$COMPOSE_FILE")
fi

cleanup() {
  "${COMPOSE[@]}" down -v --remove-orphans >/dev/null 2>&1 || true
}
trap cleanup EXIT

echo "==> chmod CGI scripts"
chmod +x tests/interop/apache/cgi-bin/*.sh || true

echo "==> starting interop stack"
"${COMPOSE[@]}" up -d --build

wait_url() {
  local url="$1"
  local name="$2"
  local i
  for i in $(seq 1 90); do
    if curl -fsS --max-time 2 "$url" >/dev/null 2>&1; then
      echo "  ready: $name ($url)"
      return 0
    fi
    sleep 1
  done
  echo "error: timed out waiting for $name ($url)" >&2
  return 1
}

echo "==> readiness"
set +e
FAILED=0
wait_url "http://127.0.0.1:18080/plain" nginx || FAILED=1
wait_url "http://127.0.0.1:18081/plain" httpd || FAILED=1
wait_url "http://127.0.0.1:18082/plain" caddy || FAILED=1
wait_url "http://127.0.0.1:18083/plain" python || FAILED=1
wait_url "http://127.0.0.1:18084/plain" node || FAILED=1
wait_url "http://127.0.0.1:18085/plain" axum || FAILED=1
wait_url "http://127.0.0.1:18086/plain" haproxy || FAILED=1
set -e

if [[ "$FAILED" -ne 0 ]]; then
  echo "==> dumping compose logs" >&2
  "${COMPOSE[@]}" ps | tee "$LOG_DIR/ps.txt" || true
  "${COMPOSE[@]}" logs --no-color | tee "$LOG_DIR/compose.log" || true
  exit 1
fi

echo "==> running interop_client (BAREHTTP_INTEROP=1)"
set +e
BAREHTTP_INTEROP=1 cargo nextest run --profile ci --locked --features gzip --test interop_client
STATUS=$?
set -e

if [[ "$STATUS" -ne 0 ]]; then
  echo "==> interop failed; dumping logs" >&2
  "${COMPOSE[@]}" ps | tee "$LOG_DIR/ps.txt" || true
  "${COMPOSE[@]}" logs --no-color | tee "$LOG_DIR/compose.log" || true
  exit "$STATUS"
fi

echo "==> interop ok"
exit 0
