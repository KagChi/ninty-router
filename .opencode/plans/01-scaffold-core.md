# 01 — Scaffold: workspace, DB, settings, API keys, Solid-Start shell

## Goal

Runnable skeleton: Rust workspace compiles, axum server serves health + settings + keys
API, SQLite persists, Solid-Start dev server proxies and shows login + layout.

## Tasks

1. Cargo workspace: `crates/{core,engine,server,cli}`, root `Cargo.toml`.
2. `core`: error types (`thiserror`), config (DATA_DIR default `~/.ninty-router`, PORT 20128),
   settings struct with defaults (rtkEnabled=true, comboStrategy="fallback",
   stickyRoundRobinLimit=3, requireApiKey=false initially, requireLogin=false).
3. `server/db`: rusqlite bundled, WAL, `migrate()` creating schema from
   `00-architecture.md` (settings, provider_connections, provider_nodes, api_keys, combos,
   kv, usage_history, usage_daily, request_details). Sync calls wrapped in
   `tokio::task::spawn_blocking`.
4. Repos: settings (get/patch JSON blob), api_keys (CRUD + `generate` →
   `sk-{machine16}-{id6}-{crc8}`, crc = HMAC-SHA256(key=API_KEY_SECRET env or default,
   machine+id)[..8]; machine id from `/etc/machine-id` fallback hostname hash).
5. `server`: axum router, `GET /api/health`, `GET|PATCH /api/settings`,
   `GET|POST /api/keys`, `PUT|DELETE /api/keys/{id}`, `POST /api/keys/{id}/reset`.
   Password auth: `POST /api/auth/login` (bcrypt verify against settings.password),
   session cookie (signed, jsonwebtoken), `GET /api/auth/status`, middleware enforcing
   auth on /api when password set. `POST /api/auth/set-password`.
6. `cli` bin: clap flags `-p/--port` (20128), `-H/--host` (127.0.0.1), `--no-browser`;
   starts server, opens browser to `/`.
7. `web`: Solid-Start app (static preset), Tailwind v4, material-symbols font, dark theme
   CSS vars. Pages so far: Login, layout with sidebar nav (Endpoint, Providers, Combos,
   Usage, Quota, Settings — dead links OK), Endpoint page showing base URL + keys table +
   create-key modal. vite dev proxy `/api` + `/v1` → `localhost:20128`.
8. `scripts/dev.sh` (run server + web), `.gitignore` (target, node_modules, dist).

## Reference

`$REF/src/lib/db/schema.js`, `repos/apiKeysRepo.js`, `shared/utils/apiKey.js`,
`src/sse/services/auth.js`, `app/api/keys/*`.

## Done when

- `cargo clippy -- -D warnings` clean, `cargo test` passes (key crc roundtrip unit test).
- `curl localhost:20128/api/health` → ok; create key via curl, row visible in sqlite.
- `bun run dev` serves login → Endpoint page; create/list/delete key from UI.
