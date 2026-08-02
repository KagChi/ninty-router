# 02 — Core chat path: OpenAI passthrough, SSE streaming, providers, usage

## Goal

`POST /v1/chat/completions` works end-to-end for OpenAI-compatible providers
(custom nodes, OpenRouter, DeepSeek, Groq, Mistral, xAI, Together, Blackbox).
Streaming + non-streaming. Usage recorded. Providers manageable from dashboard.

## Tasks

1. `core/registry`: provider registry as data — per provider: id, alias(es), category
   (apikey|oauth|free), transport {base_url, headers, auth descriptor, force_stream,
   timeout_ms, retry overrides}, models [{id, name, upstream_model_id}]. Port entries
   from `$REF/open-sse/providers/registry/{openrouter,deepseek,groq,mistral,xai,together,blackbox}.js`.
   Provider nodes (custom OpenAI-compatible) resolve as dynamic providers from DB.
2. Model resolution: `provider/model` (alias-aware), bare model → infer provider by
   prefix match against registry models; apply `upstreamModelId` before sending.
3. `engine/executor/default`: build request (Bearer apiKey from connection.data),
   per-status retry from `runtimeConfig` (429=0, 502=3×3s, 503=3×2s, 504=2×3s),
   connect timeout 60s. Reqwest streaming response.
4. `server` chat handler: parse body, auth API key (Bearer/x-api-key) with
   requireApiKey check + token-limit + RPM sliding window + model allow-list,
   resolve model → pick connection (fill-first priority; only is_active, skip
   model-locked) → execute → return SSE or JSON.
5. SSE plumbing: upstream bytes → line parser → pass through chunks → `[DONE]`;
   usage extraction from final chunk (estimation fallback: chars/4); client-disconnect
   aborts upstream; stall timeouts (first 200s, inter 360s); upstream non-SSE error body
   → sanitized OpenAI error JSON.
6. Usage recording: insert `usage_history` (provider, model, connection, key, tokens,
   cost via static price table in registry, status), update `usage_daily`.
7. Dashboard Providers page: list registry providers, add API-key connection (modal),
   per-connection test button (`POST /api/providers/{id}/test` → small chat request),
   activate/deactivate, delete, priority reorder. `GET /api/providers` strips secrets.
8. `GET /v1/models`: aggregate enabled providers' models as `provider/model` (+ combos later).

## Reference

`$REF/open-sse/executors/{base,default}.js`, `config/runtimeConfig.js`,
`utils/{stream,streamHandler,usageTracking,error}.js`, `src/sse/handlers/chat.js`,
`app/api/providers/*`, `app/api/v1/models/route.js`.

## Done when

- curl OpenRouter (or a mock) through `/v1/chat/completions` streams correct SSE; usage row written.
- Unit tests: model resolve, retry config, usage estimation, SSE line parser.
- Dashboard: add/test/delete a connection; key limits enforced (exceed RPM → 429 JSON).
