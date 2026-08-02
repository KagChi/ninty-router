#!/usr/bin/env bash
# Production build: web static bundle + single rust binary with embedded dashboard.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "==> web build"
(cd web && bun install --frozen-lockfile && bun run build)

echo "==> cargo build --release"
cargo build --release -p cli --features embed-web

echo "==> done: target/release/ninty-router"
