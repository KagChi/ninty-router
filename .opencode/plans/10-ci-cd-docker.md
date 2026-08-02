# 10 — CI/CD + Docker

Status: scaffolding DONE — `Dockerfile`, `.dockerignore`, `docker-compose.yml`,
`.github/workflows/{ci,release}.yml`, `scripts/{build,dev}.sh` exist in repo.
Remaining work is only verification once milestones 01+ produce the workspace.

## Goal

Reproducible CI (lint/test/build) and a multi-arch Docker image publishing to GHCR,
plus release binaries. One command local Docker run.

## Tasks

1. `Dockerfile` (multi-stage, repo root):
   - Stage `web`: `oven/bun:1-alpine`, copy `web/`, `bun install --frozen-lockfile`, `bun run build` → `/web/dist`.
   - Stage `rust`: `rust:1-slim-bookworm`, `apt install pkg-config` (no openssl needed —
     rustls), copy workspace + `web/dist` from stage web, `cargo build --release
     -p cli --features embed-web`. Cache: `cargo chef` or
     `--mount=type=cache,target=/usr/local/cargo/registry` + target dir.
   - Stage `runtime`: `debian:bookworm-slim` (glibc, small), non-root user `ninty`,
     `ENV DATA_DIR=/data PORT=20128 HOST=0.0.0.0`, `VOLUME /data`, `EXPOSE 20128`,
     `USER ninty`, `ENTRYPOINT ["/usr/local/bin/ninty-router"]`.
     Healthcheck: `curl -f http://localhost:20128/api/health` (curl via apt, or use
     binary subcommand `ninty-router health` instead to stay curl-free — prefer subcommand).
   - Final image target ≤ 120 MB.
2. `docker-compose.yml` (convenience only, not required): service `ninty-router`,
   build `.`, ports `20128:20128`, volume `ninty-data:/data`, restart unless-stopped.
3. `.dockerignore`: target/, node_modules/, web/dist, .git, .opencode/, docs/, *.md
   (keep Cargo.* and crates/, web/src etc).
4. GitHub Actions `.github/workflows/ci.yml` (on push + PR):
   - `fmt` (`cargo fmt --check`), `clippy` (`-- -D warnings`), `test` (`cargo test --workspace`),
     web job: `bun install --frozen-lockfile && bun run build` in `web/`.
   - Rust cache via `Swatinem/rust-cache@v2`. Bun via `oven-sh/setup-bun@v2`.
5. `.github/workflows/release.yml` (on tag `v*`):
   - Matrix build binaries: `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`
     (cross or native arm runner), `x86_64-apple-darwin`, `aarch64-apple-darwin`.
     Each: build web first, then `cargo build --release --features embed-web`,
     strip, tar.gz, upload to GitHub Release via `softprops/action-gh-release`.
   - Docker job: `docker/setup-buildx-action` + `docker/login-action` (GHCR,
     `GITHUB_TOKEN`) + `docker/build-push-action`: platforms `linux/amd64,linux/arm64`,
     tags `ghcr.io/<owner>/ninty-router:latest`, `:vX.Y.Z`, `:sha-<short>`.
     `packages: write` permission on the job.
6. README additions: `docker run -d -p 20128:20128 -v ninty-data:/data
   ghcr.io/<owner>/ninty-router` quickstart + compose snippet + CI badge.

## Reference

`$REF/Dockerfile`, `$REF/DOCKER.md`, `$REF/.github/` (for behaviour parity ideas —
do not copy Next.js steps; ours is rust+web two-stage).

## Done when

- `docker build -t ninty-router .` succeeds locally; `docker run -p 20128:20128
  -v ninty-data:/data ninty-router` → dashboard reachable, data persists across
  container restarts (create key, restart, key still there).
- Image runs as non-root; `/api/health` healthcheck passes (`docker inspect` healthy).
- CI green on a test PR: fmt, clippy, tests, web build.
- (Maintainer runs, not agent) tag push produces release assets + GHCR multi-arch image.
- `docker compose up` works from clean clone.
