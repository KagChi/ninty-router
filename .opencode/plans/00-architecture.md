# 00 — Architecture & scope

Local AI router. One OpenAI-compatible endpoint on localhost, routing coding CLI tools
(Claude Code, Cursor, Cline, Codex, Copilot…) to many AI providers with tiered
auto-fallback (subscription → cheap → free) and token-saving compression.

Stack: **Rust backend (axum) + Solid-Start dashboard, single binary in prod.**
Reference: 9router (Node/Next.js) at temp clone path (see AGENTS.md).

## Request flow

```
CLI tool ──► :20128/v1 (OpenAI | Claude | Gemini | Responses format)
  1. API-key auth (Bearer / x-api-key / x-goog-api-key) + per-key limits (tokens, RPM, model allow-list)
  2. Model resolve: combo name → combo chain | "provider/model" → provider+model (aliases, upstreamModelId)
  3. Account select in provider: fill-first priority (default) | round-robin (sticky N=3); skip model-locked accounts
  4. Translate request client-format → provider-format (pivot through OpenAI)
  5. Token savers in order: RTK → PXPIPE (later) → Caveman → Ponytail
  6. Executor dispatch (Default or specialized), per-status retry (502×3, 503×3, 504×2)
  7. On 401/403: refresh OAuth token, retry once
  8. Stream translate provider → client format per SSE chunk; extract usage (estimate if absent)
  9. Record usage_history; on classified error → mark account (lock/backoff/deactivate) → fallback to next account/model
```

## Repo layout

```
crates/
  core/    types, config, provider registry (data), error classification rules
  engine/  translator, rtk, caveman/ponytail, pxpipe, fallback, quota, oauth refresh, executors
  server/  axum app: /v1 gateway, /api dashboard API, rust-embed static dashboard, auth sessions
  cli/     bin `ninty-router`: flags -p/--port (20128), -H/--host, --no-browser; opens dashboard
web/       Solid-Start SPA (dashboard)
data:      ~/.ninty-router/db/data.sqlite (WAL), env DATA_DIR overrides
```

## Key dependencies

axum 0.8 · tokio · reqwest (rustls, stream) · serde/serde_json · rusqlite (bundled) ·
tokio-stream, async-stream · jsonwebtoken · bcrypt · sha2/hmac/md5/rsa/aes (qoder COSY) ·
uuid · rust-embed · thiserror · tracing

## SQLite schema

- `settings(id=1, data JSON)` — all app settings (rtkEnabled, caveman/ponytail, comboStrategy,
  requireApiKey, password hash, providerStrategies, stickyRoundRobinLimit=3…)
- `provider_connections(id, provider, auth_type, name, email, priority, is_active, data JSON, created_at, updated_at)`
  — data holds tokens, apiKey, testStatus, backoffLevel, lastUsedAt, consecutiveUseCount,
  `modelLock_<model>` ISO expiries, providerSpecificData
- `provider_nodes(id, type, name, data JSON)` — custom OpenAI/Anthropic-compatible endpoints
- `api_keys(id, key UNIQUE, name, is_active, token_limit, limit_window, rpm_limit, allowed_models JSON, limit_reset_at, created_at)`
  — key format `sk-{machine16}-{id6}-{crc8}` (HMAC-SHA256 crc, like 9router)
- `combos(id, name UNIQUE, kind, models JSON ["provider/model",…])`
- `kv(scope, key, value)` — model aliases, disabled models
- `usage_history(id, ts, provider, model, connection_id, api_key, endpoint, prompt_tokens, completion_tokens, cost, status, tokens JSON, meta JSON)`
- `usage_daily(date_key, data JSON)`
- `request_details(id, ts, provider, model, connection_id, status, data JSON)` — debug dumps

## Providers in scope (v1)

| Group | Providers | Transport |
|---|---|---|
| Passthrough (OpenAI) | custom nodes, OpenRouter, DeepSeek, Groq, Mistral, xAI, Together, Blackbox (`upstreamModelId` map) | OpenAI chat + Bearer |
| Native | Anthropic, Gemini, Vertex (service-account JWT, $300 credits) | translated |
| Cheap | GLM (z.ai coding + anthropic endpoints), MiniMax, Kimi | OpenAI/Claude variants |
| Free | OpenCode Free (no auth, model list from opencode.ai/zen/v1/models) | OpenAI |
| OAuth | Claude Code (PKCE paste-code), Codex (PKCE, loopback :1455), GitHub Copilot (device code + copilot token re-mint), Kiro (AWS SSO OIDC device), Cline (WorkOS token), CodeBuddy CN/Intl (poll flow), Qoder (device token + COSY-signed SSE) | per-registry |

## Out of scope (v1)

Media endpoints (TTS/STT/images/video/search), fusion combos, MITM, cloud sync,
proxy pools, bulk-import automation, i18n, terminal UI, tray.

## Gateway endpoints

`POST /v1/chat/completions` · `POST /v1/messages` + `/v1/messages/count_tokens` ·
`POST /v1/responses` · `GET /v1/models` ·
`POST /v1beta/models/{model}:{generateContent|streamGenerateContent}`

## Dashboard pages

Login (optional password) · Endpoint & Keys · Providers (+OAuth connect modals) ·
Combos · Usage (charts, history, per-key/provider) · Quota (per-account + reset countdowns) ·
Settings (token savers, combo strategy, security)

## Dev / prod

- Dev: `scripts/dev.sh` → `cargo run -p cli` (:20128) + vite dev (:3000, proxies /api + /v1)
- Prod: `scripts/build.sh` → web static build → rust-embed → single binary on :20128
- Docker: multi-stage `Dockerfile` (web → rust → debian-slim, non-root, `/data` volume),
  `docker-compose.yml` convenience; CI `.github/workflows/ci.yml` (fmt/clippy/test/web/docker),
  release `.github/workflows/release.yml` (binaries matrix + GHCR multi-arch on `v*` tags)

## Fallback rules (port exactly)

- Error classes (ordered): no-credentials/401–404 → 2min lock · rate-limit/429/quota →
  exponential backoff (2s×2^level, cap 5min, level ≤15) · unmatched 400/422 → NO fallback ·
  else → 30s transient. Provider `resetsAtMs` honored, capped 30min.
- Credit-exhausted patterns (`14018`, `积分不足`, `余额不足`, …) → `is_active=false`.
- Combo fallback: ordered list, capability reorder (vision/pdf floats capable models first),
  track earliest Retry-After for final 503.
