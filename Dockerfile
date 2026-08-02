# ---------- Stage 1: web dashboard (Solid-Start static build, bun) ----------
FROM oven/bun:1-alpine AS web
WORKDIR /build/web
COPY web/package.json web/bun.lock* ./
RUN bun install --frozen-lockfile
COPY web/ ./
RUN bun run build

# ---------- Stage 2: rust binary (embeds web/.output/public) ----------
FROM rust:1-slim-bookworm AS rust
WORKDIR /build
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config \
    && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock* ./
COPY crates/ crates/
COPY --from=web /build/web/.output/public web/.output/public
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo build --release -p cli --features embed-web \
    && cp target/release/ninty-router /usr/local/bin/ninty-router

# ---------- Stage 3: runtime ----------
FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --uid 10001 ninty
COPY --from=rust /usr/local/bin/ninty-router /usr/local/bin/ninty-router

ENV DATA_DIR=/data \
    PORT=20128 \
    HOST=0.0.0.0
VOLUME /data
EXPOSE 20128

RUN mkdir -p /data && chown -R ninty:ninty /data
USER ninty

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -fsS http://localhost:20128/api/health || exit 1

ENTRYPOINT ["/usr/local/bin/ninty-router"]
