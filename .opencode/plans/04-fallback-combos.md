# 04 — Account fallback, error classification, combos

Status: DONE. engine::fallback (ERROR_RULES port: text rules, 429 exp-backoff 2s×2^level cap 5min
maxLevel 15, resetsAt cap 30min, credit patterns → deactivate, 400/422 no-fallback),
mark_unavailable/clear_error (modelLock_, backoffLevel, lastError, sticky counters), account loop +
combo model loop in v1/chat.rs, round-robin + sticky strategy ordering, capability reorder
(prefix-based vision float), combos repo + /api/combos + Combos UI, connection data.baseUrl
override. chat.rs fully rewritten (lost prior uncommitted version; reimplemented incl. two-stage
stream pipeline, gemini ?key= auth, vertex SA mint). e2e: 429→fallback (stream+non-stream), lock
persist+skip, combo dispatch, NoFallback surfacing, claude→openai two-stage stream.

## Goal

Multi-account per provider with rotation and automatic fallback; combos (ordered
fallback groups + round-robin) selectable as model names.

## Tasks

1. `engine/fallback`: port error classification from `$REF/open-sse/services/accountFallback.js`
   + `config/errorConfig.js`: ordered rules (text match then status), results =
   {should_fallback, cooldown_ms | resets_at, backoff_level bump, deactivate}.
2. Account state writes on failure: `modelLock_<model>` = ISO expiry in connection.data,
   backoffLevel increment, testStatus/lastError; credit-exhaust patterns → is_active=false.
   On success: clear succeeded model lock + expired locks, reset backoff, update lastUsedAt.
3. Selection strategies: `fill-first` (priority asc) default; `round-robin` with sticky
   limit (settings.stickyRoundRobinLimit=3, consecutiveUseCount). Mutex around selection.
   Exclude failed connection ids within one request; loop accounts.
4. `engine/combo`: combos table CRUD (`/api/combos*`), strategy `fallback` (ordered)
   and `round-robin` (in-memory rotation state). Request with `model = combo name`:
   iterate models, per-model run full account pipeline; on fallbackable error continue;
   collect earliest Retry-After; final 503 `unavailableResponse` when exhausted.
5. Capability reorder: scan last user message for image/pdf blocks → float models with
   matching capabilities (registry model `capabilities`) to front.
6. Dashboard Combos page: create/edit/delete combo, pick models across providers,
   drag to reorder, strategy select. `/v1/models` lists combos as their names.
7. Dashboard Providers: show cooldown/lock state per connection with countdown,
   manual "clear lock" action.

## Reference

`$REF/src/sse/handlers/chat.js` (handleSingleModelChat account loop),
`open-sse/services/{accountFallback,combo}.js`, `config/errorConfig.js`,
`app/api/combos/*`, dashboard `combos/page.js`, `ConnectionRow.js`, `CooldownTimer.js`.

## Done when

- Two accounts same provider: kill first (bad key) → request auto-uses second; lock row visible.
- Combo of 3 models: force failures → falls through chain; correct 503 with Retry-After when all down.
- Round-robin distributes 3-sticky then rotates.
- Unit tests: every ERROR_RULES class, backoff cap 5min, credit-exhaust deactivation, capability reorder.
