#!/usr/bin/env bash
# Dev: rust API on :20128 + Solid-Start vite dev on :3000 (proxies /api + /v1).
set -euo pipefail
cd "$(dirname "$0")/.."

trap 'kill 0' EXIT

cargo run -p cli &
(cd web && bun run dev) &

wait
