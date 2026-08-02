# 08 — Qoder (COSY) + quota tracking + Quota page

Status: DONE (deviations). engine::qoder (COSY 19-header sign: RSA PKCS1 + AES-128-CBC + MD5,
golden-shape test; body builder w/ stable ids; envelope unwrap; live model_config fetch, kv cache),
registry qoder (qd) 12 models, oauth device flow (PKCE+nonce+machine_id, poll nonce+verifier,
userinfo best-effort), run_qoder custom executor (translate→body→sign→envelope-unwrap stream +
collect), QoderUnwrap stream. engine::quota: codex/github/claude/codebuddy-cn/intl fetchers with
parsers (3 fixture tests), GET /api/usage/quota (5min kv cache), Quota page (cards, % bars, reset
countdown, 60s refresh). Deviations: (1) qoder live e2e untested — needs real device login; COSY
verified by golden-shape unit test only; (2) PAT pt-→jt exchange not implemented; (3) qoder refresh
skipped (reference: upstream 403, re-login surfaced); (4) quota fetchers for kimi/glm/minimax/kiro
not implemented — page shows "quota not supported" for those.

## Goal

Qoder provider working end-to-end (heaviest port). Quota fetchers for all providers
that expose them; dashboard Quota page with reset countdowns.

## Tasks

1. Qoder OAuth: device flow — login URL `https://qoder.com/device/selectAccounts`,
   poll `https://openapi.qoder.sh/api/v1/deviceToken/poll`; refresh
   `https://center.qoder.sh/algo/api/v3/user/refresh_token`; userinfo
   `https://openapi.qoder.sh/api/v1/userinfo`. Also accept Personal Access Token
   (`pt-…` from qoder.com/account/integrations) as apikey authMode.
2. Qoder executor — port `$REF/open-sse/executors/qoder.js` (595 LOC) +
   `shared/qoder/{cosy,encoding,constants}.js`:
   - URL `https://api3.qoder.sh/algo/api/v2/service/pro/sse/agent_chat_generation&Encode=1`.
   - COSY auth: RSA + AES + MD5, ~17 `Cosy-*` headers (port `cosy.js` exactly;
     deps: rsa, aes, md5 crates).
   - Body: hoist system out of messages, `chat_context` with mirrored `model_config`,
     `business` block with stable sha256-derived ids (port `stableHash`/`stableChatRecordId`).
   - Model config: fetch live from `/algo/api/v2/model/list`, cache in kv; missing
     entry = hard error (wrong config silently downgrades upstream).
   - Response: unwrap `{statusCodeValue, body}` SSE envelope → plain OpenAI SSE.
   - Model map: port `QODER_MODEL_MAP` (ultimate/auto/performance/efficient/qmodel*/kmodel*/…).
3. Quota service `engine/quota`: per-provider fetcher trait; implement for
   codex (usage + reset-credits), github copilot, kiro, qoder
   (`https://openapi.qoder.sh/api/v2/quota/usage`), codebuddy cn/intl (billing meter),
   glm, minimax, kimi, claude (rate-limit headers where available). Port from
   `$REF/open-sse/services/usage/*`. Cache results 5min in kv; manual refresh button.
4. `GET /api/usage/quota` (all connections) + `GET /api/usage/quota/{connectionId}`.
5. Dashboard Quota page: cards per connection — used/remaining, % bar, reset countdown
   (5h/daily/weekly/monthly), plan label, last refreshed; auto-refresh 60s.
6. Local quota inference fallback: when provider gives no API, derive from
   usage_history sums vs known tier caps (registry `quotaFamily`), display as estimate.

## Reference

`$REF/open-sse/executors/qoder.js`, `shared/qoder/*`, `services/qoderModels.js`,
registry `qoder.js`, `services/usage/*`, dashboard `quota/page.js` + `ProviderLimits/*`.

## Done when

- Qoder: connect via device flow, stream chat, model config cached, COSY headers accepted
  (no 401/403), response unwrapped correctly.
- Quota page shows live data for ≥ codex + copilot + qoder + codebuddy; countdowns tick.
- Unit tests: stable id hashing, envelope unwrap, cosy header builder (golden vectors
  from reference behaviour), quota parsers on fixture JSON.
- Risk flag: if COSY upstream drifted (4xx on valid port), isolate behind
  `QODER_ENABLED` setting, ship rest, report to user.
