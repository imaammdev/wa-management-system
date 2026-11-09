#!/usr/bin/env bash
# Runs the API, the worker and the dashboard together. Ctrl-C stops all three.
set -euo pipefail
cd "$(dirname "$0")/.."

if [ ! -f .env ]; then
  echo "No .env found. Run scripts/setup.sh first." >&2
  exit 1
fi

export PATH="$HOME/.cargo/bin:$PATH"

pids=()
cleanup() {
  for pid in "${pids[@]:-}"; do kill "$pid" 2>/dev/null || true; done
}
trap cleanup EXIT INT TERM

echo "→ building…"
cargo build -p mms-api -p mms-worker

echo "→ api      http://127.0.0.1:8080"
cargo run -q -p mms-api & pids+=($!)

echo "→ worker"
cargo run -q -p mms-worker & pids+=($!)

echo "→ dashboard http://localhost:5173"
(cd web && pnpm dev) & pids+=($!)

wait
