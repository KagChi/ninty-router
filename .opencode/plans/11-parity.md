# 11 — Parity: full 1:1 with 9router dashboard + contracts

Status: IN PROGRESS
Scope decisions (user, 2026-08-02): SKIP Tunnel/Tailscale Funnel. Full 1:1 all phases.
Ground truth: `$REF` (9router clone). Read reference first, port behaviour.

Completed earlier (do NOT redo): core chat/fallback/combos, token savers, OAuth
(claude/codex/github/cline/codebuddy/qoder), qoder quota page, request logs,
usage analytics, single binary, CI, 9router DB import, UI kit + Baloo 2 + dark,
mobile drawer, providers grid/detail restructure, skeletons, models fetcher +
boot preload (opencode/openrouter), codebuddy quota payload/field/split fix,
quota kv scope fix, provider test via chat pipeline (superseded by P2 below).

---

## P1 — Quota page 1:1

Backend contract (`crates/engine/src/quota.rs`): QuotaWindow becomes raw numbers
`{label, used: f64, total: f64, unlimited: bool, remaining: Option<f64>, recurring: bool,
reset_at}` (drop percent-only shape; % computed client-side as REMAINING).
QuotaReport keeps `{connection_id, provider, plan, windows, error(message), fetched_at}`.

Port missing fetchers (from `$REF/open-sse/services/usage/`):
- deepseek: GET `https://api.deepseek.com/user/balance` → `Balance (CCY)` rows,
  `{used:0,total:total_balance,unlimited:total>0}`; plan `DeepSeek` / `DeepSeek (Insufficient Balance)`.
- glm / glm-cn: GET `https://api.z.ai/api/monitor/usage/quota/limit` /
  `https://open.bigmodel.cn/api/monitor/usage/quota/limit` Bearer apiKey →
  `limits[]` type TOKENS_LIMIT → `session` {used:percentage,total:100}; plan=level.
- minimax / minimax-cn: GET `https://www.minimax.io/v1/token_plan/remains` (cn:
  `https://www.minimaxi.com/v1/api/openplatform/coding_plan/remains`) + coding_plan
  fallback → per model_remains `M-series (5h)`/`(7d)` rows (current_interval_* /
  weekly_* counts, remains_time/end_time reset).
- kimi: GET `https://api.kimi.com/coding/v1/usages` — x-api-key when apiKey else
  Bearer + X-Msh-* headers → `Weekly` (usage.used/limit/resetTime) + `Ratelimit`
  (limits[].detail); plan from membership.level map.
- qoder: GET `https://openapi.qoder.sh/api/v2/quota/usage` Bearer → `Personal`
  (userQuota) + `Organization` (orgResourcePackage) credit rows, resetAt=expiresAt.
- github paid shape: `{used:entitlement-remaining,total:entitlement}`; free:
  monthly_quotas vs limited_user_quotas, reset limited_user_reset_date.
- claude labels: `session (5h)`, `weekly (7d)`, `weekly <model> (7d)` + extraUsage.

Eligibility (the "filter"): registry ProviderDef gains
`features: {usage: bool, usage_apikey: bool}` (usage: claude, codex, github, qoder,
codebuddy-cn/intl, kimi; usage_apikey: codebuddy-cn/intl, deepseek, glm, glm-cn,
kimi, minimax, minimax-cn). api/quota.rs uses flags instead of hardcoded list.

Endpoints:
- keep `GET /api/usage/quota` (5min kv cache) for page load.
- new `GET /api/usage/quota/{connId}`: live fetch, no cache; oauth: refresh-before
  when near expiry; on auth-expired message (expired/authentication/unauthorized/401/
  re-authorize) + refreshToken → force refresh + retry once.

UI (`web/src/routes/quota.tsx`) rewrite:
- ProviderLimitCard: 40px brand icon tile, label (name||email||displayName) +
  secondary email, plan Badge, per-card refresh button (spin while loading),
  error box (red) / message box (blue) / empty state (data_usage icon).
- QuotaTable rows: emoji by REMAINING % (>70 🟢, >=30 🟡, else 🔴), name,
  h-1 progress bar (remaining%), `{used} / {total|∞}` + `{remaining}%`,
  reset column `in Xd Xh` (recurring) / `expires in Xd Xh` (non-recurring) +
  secondary `Today/Tomorrow/Mon D, h:MM AM/PM`.
- Auto-refresh 60s + countdown + toggle (localStorage quotaAutoRefresh), claude
  throttled to every 3rd tick, paused on document.hidden.
- Pagination 20/page (select 10/20/50/100, First/Prev/Next/Last), account filter
  all/active/inactive, provider dropdown (excl. codebuddy per 9router), sort
  priority/expiring-first. Depleted highlight when any row remaining <= 5%.
- Empty states: cloud_off "No Providers Connected".

## P2 — Provider test matrix 1:1

`crates/server/src/api/providers.rs` test_connection: per-provider probe table
replacing single chat-pipeline probe. From `$REF/.../test/testUtils.js`:
- tokenExists: codebuddy-cn (also opencode).
- checkExpiry (no probe): claude (refresh if near expiry).
- codebuddy-intl: POST `https://www.codebuddy.ai/v2/chat/completions`, body
  `{model:"gemini-2.5-flash",messages:[{role:"user",content:"hi"}],max_tokens:1,stream:false}`,
  headers UA `CLI/2.52.0 CodeBuddy/2.52.0`, X-Product SaaS, X-IDE-Type/Name CLI,
  X-IDE-Version 2.52.0, X-Agent-Intent craft, X-Domain www.codebuddy.ai,
  x-requested-with — valid = status != 401.
- codex: POST `https://chatgpt.com/backend-api/codex/responses` body
  `{model:"gpt-5.3-codex",input:[],stream:false,store:false}`, headers originator
  codex_cli_rs, UA codex_cli_rs/0.136.0 — accept 400 as valid; 401/403 fail.
- github: GET `https://api.github.com/user` Bearer + UA 9Router.
- cline: GET `https://api.cline.bot/api/v1/users/me`, Authorization `Bearer workos:<token>`,
  HTTP-Referer/X-Title/X-CLIENT-* headers.
- qoder oauth: GET `https://openapi.qoder.sh/api/v1/userinfo` Bearer.
- GET-models probes: deepseek/groq/mistral/xai/together (`{base}/models`),
  openrouter (`/api/v1/auth/key`), gemini (`/v1/models?key=`), blackbox (`/v1/models`).
- POST-messages probes (valid = !401 && !403): anthropic `/v1/messages`
  (x-api-key + anthropic-version), glm `api.z.ai/api/anthropic/v1/messages`,
  glm-cn `open.bigmodel.cn/api/coding/paas/v4/chat/completions`,
  minimax `api.minimax.io/anthropic/v1/messages`, minimax-cn `api.minimaxi.com/…`,
  kimi apikey `api.kimi.com/coding/v1/messages`.
- OAuth: refresh-before-probe when `expiresAt - now < lead` (+refreshToken),
  401 → refresh once → retry once.
- testStatus values `"active"/"error"` (migrate stored "ok" → treat as active in UI).

## P3 — Provider detail page 1:1

- ModelDef gains capability metadata (port `kind`/caps from `$REF` open-sse registry:
  vision, reasoning). /api/providers exposes caps; chips render CapacityBadges
  (visibility / neurology icons).
- Models card: chips `{alias}/{modelId}` mono + italic name + caps, hover actions
  test (`science`)/copy/disable (`close`), inline dashed `Add Model`, "Disabled
  models (n)" restore block, "Suggested free models" block (fetcher providers),
  `Active All`/`Disable All`.
- New APIs: `GET/POST/DELETE /api/models/custom`, `/api/models/disabled`,
  `GET/PUT/DELETE /api/models/alias` (global alias map; chat resolution consults it
  before provider prefix inference).
- Connections card: priority up/down arrows, Round Robin toggle + sticky count
  (settings providerStrategies), Import/Export JSON, Test one-by-one + Stop +
  progress summary, bulk select Enable/Disable/Delete, row: auth icon + secondary
  email + cooldown + lastError + balance pill.
- Banners: notice (blue) / deprecation (yellow); compatible-node detail card +
  "Add Anthropic Compatible" flow.

## P4 — Endpoint page 1:1

- Model Aliases card (uses P3 alias API): alias → target rows + delete all.
- api_keys migration: add `token_limit INTEGER, limit_window TEXT
  (monthly|daily|total), allowed_models TEXT(json), is_active INTEGER DEFAULT 1`.
  Per-key rows: usage progress bar (window used / limit, red at cap), pause/resume
  toggle, reset usage button. Create/Edit modal: name, token limit (0=unlimited),
  window select, RPM, allowed-models picker. "API Key Created" save-now dialog.
- Chat path enforces: key paused → 401; token limit reached → 429; allowed_models
  already partially present (allowed_models check exists at chat.rs:108).
- DEFERRED (out of scope): Cloudflare Tunnel, Tailscale Funnel, Token Limit
  Settings card, public /usage-check page.

## P5 — Shell + remaining pages 1:1

- Sticky Header: per-page icon+title+description (exact `$REF` strings), toast
  system (success/error/warning/info, fixed top-right), header menu (theme,
  shutdown, logout), header-registered search.
- Combos: 3-line strategy blurb + searchable model picker modal (replace
  free-text textarea).
- Usage: request-log detail modal fields parity + filters.
- Settings: parity pass on `$REF` profile page (change password etc.).
- Providers list: Test All → results modal (`POST /api/providers/test-batch`),
  compatible-provider cards with API-type badges, "Show all N providers" expander
  after 20 in API Key section.

---

## Verification (every phase)

`cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`,
`cd web && bun run build`, playwright screenshot pass (desktop + 390px),
live e2e against running server where credentials/mocks allow.

## Status log

- 2026-08-02: plan created. P1 started.
