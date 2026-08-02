# 09 — PXPIPE, request logging, production build, docs

Status: DONE (deviations). engine::pxpipe — pxpipe-proxy invoked via subprocess shim
(bun/node, $DATA_DIR/pxpipe/shim.mjs) instead of in-process import; gates/summary/log
format ported 1:1; live-verified (54k system slab → 2 images, 77.5% est saved, health
checks green). /api/pxpipe/{status,install,health} + Token Saver UI (toggle, gates,
install button w/ poll). Request logs: DetailCtx in chat pipeline — client body +
post-savers provider body (64KB trunc), latency, pxpipe meta; ring buffer 1000;
/api/usage/request-logs + Usage → Request Details tab. Analytics: /api/usage/
{stats,history,providers} + Usage page (stat cards, 30d bar chart, by-provider/model,
CSV export). Prod: web_static (rust-embed web/.output/public, SPA fallback, immutable
assets) behind embed-web; cli feature forward; build.sh verified standalone from /tmp
(dashboard, SPA, /v1, port-conflict). CLI: PORT/HOST env, port conflict detection,
banner, SIGTERM WAL checkpoint. README with quickstart (verified paths), per-tool
endpoint table, saver table, fallback explainer, cost-as-savings note. Deviations:
(1) pxpipe is subprocess not in-process (Rust has no pxpipe renderer); Docker runtime
has no node → pxpipe unavailable there, fail-open; (2) per-key/per-connection usage
filters not implemented (aggregates only); (3) headroom proxy saver not ported
(optional in reference too); (4) qoder path doesn't write request_details extras.

## Goal

Remaining savers, observability, single-binary prod build, README.

## Tasks

1. PXPIPE (`engine/pxpipe`): image-context compression for Claude-format bodies —
   port `$REF/open-sse/handlers/pxpipe.js`: gate `pxpipeMinChars=25000`,
   `pxpipeTimeoutMs=15000`, fail-open on timeout/error. Settings toggles + Token Saver UI.
2. Request logging: settings.enableRequestLogs → write request_details (headers redacted,
   body truncated 64KB); `GET /api/usage/request-logs` + dashboard Usage → Request Details
   tab. Ring buffer cap 1000 rows.
3. Usage analytics polish: `/api/usage/{stats,chart,history,providers}` aggregations;
   Usage page charts (tokens/day, cost-as-savings note like 9router FAQ), per-key and
   per-connection filters, CSV export.
4. Production build:
   - `web`: Solid-Start static preset → `web/.output/public`.
   - `server`: rust-embed embeds `web/.output/public` (copied as `web/dist` in Docker) (feature `embed-web`), SPA fallback to
     index.html for non-/api,/v1 routes.
   - `scripts/build.sh`: build web → `cargo build --release --features embed-web` →
     `target/release/ninty-router`. Verify binary standalone (move to /tmp, run, dashboard
     + chat work).
   - `scripts/dev.sh`: backend + vite dev with proxy.
5. CLI polish: port conflict detection, `--no-browser`, startup banner with URLs,
   graceful shutdown (SIGTERM flush DB).
6. README.md: quickstart (binary + dev), endpoint config snippet per CLI tool
   (Claude Code, Cursor, Cline, Codex, Copilot), provider connect notes, token-saver
   table, fallback explainer, FAQ cost-as-savings note. Root AGENTS.md pointing to
   .opencode/plans/.
7. Final sweep: `cargo clippy -- -D warnings`, `cargo test`, fmt; web `bun run build`
   clean; manual end-to-end matrix: {claude-code, curl-openai, curl-gemini} ×
   {openrouter, anthropic, glm, copilot} × {stream, non-stream} with RTK on/off.

## Done when

- Single binary runs from any cwd: dashboard, all endpoints, savers, fallback.
- PXPIPE compresses a >25k image-heavy claude body without breaking request.
- Request logs viewable in UI with secrets redacted.
- README quickstart verified by following it exactly.
