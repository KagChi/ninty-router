# 07 — OAuth extended: Cline, CodeBuddy CN/Intl + API-key extras

Status: DONE. Registry: cline (cl), codebuddy-cn (cbcn), codebuddy-intl (cbai) with reference
headers/models; cline X-PLATFORM/X-CLIENT-* headers + workos: prefix (unit test); codebuddy
forceStream via generic collect (stage1-driven accumulator, works for openai-chunk + responses
upstreams); oauth API: cline authorize/base64-code exchange (+POST fallback), codebuddy
state→authUrl→poll token (GET ?state=, code 11217=pending) both platforms; refresh fns
refresh_cline/refresh_codebuddy (X-Refresh-Token header) wired into oauth_state (lead 300s) +
401-reactive. e2e (mock): cbcn alias resolve, force-stream collect JSON, passthrough streaming.
Deviations: (1) no live OAuth e2e (real accounts needed); refresh backdate test unit-only —
refresh URLs are compile-time consts (no mock override); (2) cline refresh endpoint shape assumed
{refresh_token} JSON (reference has no cline refresh fn); (3) codebuddy quota endpoint deferred
to M08 UI wholesale.

## Goal

Cline, CodeBuddy CN, CodeBuddy Intl connectable (OAuth or token); cheap-tier polish.

## Tasks

1. Cline (`registry/cline.js` + `shared/clineAuth.js`):
   - OAuth: authorize `https://api.cline.bot/api/v1/auth/authorize`, token
     `.../auth/token`, refresh `.../auth/refresh` (port refresh fn from
     `tokenRefresh/providers.js` cline section).
   - Token handling: prefix `workos:` when absent; Authorization `Bearer workos:…`.
   - Headers: `HTTP-Referer: https://cline.bot`, `X-Title: Cline`, plus
     `X-PLATFORM`, `X-CLIENT-TYPE`, `X-CLIENT-VERSION`, `X-CORE-VERSION`,
     `X-IS-MULTIROOT:false` (build from `buildClineHeaders`).
   - Also accept paste-token connect (authType access_token).
2. CodeBuddy CN (`codebuddy-cn.js`) and Intl (`codebuddy-intl.js`):
   - OAuth poll flow: GET `{base}/v2/plugin/auth/state` → open login → poll
     `{base}/v2/plugin/auth/token` every 5s → store; refresh via
     `{base}/v2/plugin/auth/token/refresh`. CN platform=`CLI`, UA
     `CLI/2.108.1 CodeBuddy/2.108.1`; Intl platform=`ide`, UA `IDE/…`.
   - Transport: `{base}/v2/chat/completions`, `forceStream: true` (client wants JSON →
     collect SSE then emit JSON), headers `X-Product: SaaS`, `X-IDE-Type`,
     `X-IDE-Name`, `x-requested-with: XMLHttpRequest`, `x-codebuddy-request: 1`,
     Bearer auth. `thinkingFormat: openai` (reasoning_effort shape).
   - Model lists: port both registry model arrays (identical lineup).
   - API-key mode also allowed (authModes both).
3. Force-stream generic support in executor: upstream always SSE, client JSON →
   sseToJson collector (port `sseToJsonHandler.js` behaviour).
4. Quota endpoints (wire into milestone 08 UI): cline n/a; codebuddy
   `POST {base}/v2/billing/meter/get-user-resource` → parse `data.Response.Data.Accounts[]`.
5. Dashboard connect modals for the three providers (device/poll style with QR-less
   link + countdown), paste-token fallback for each.

## Reference

`$REF/open-sse/providers/registry/{cline,codebuddy-cn,codebuddy-intl}.js`,
`shared/clineAuth.js`, `executors/{codebuddy,codebuddy-cn,codebuddy-intl}.js`,
`services/tokenRefresh/providers.js` (cline/codebuddy sections),
`services/usage/codebuddy-cn.js`, `handlers/chatCore/sseToJsonHandler.js`.

## Done when

- All three connect via dashboard poll flow and via pasted token; streaming + forced-stream
  JSON both work; model prefix aliases (`cl/`, `cbcn/`, `cbai/`) resolve.
- Refresh works for cline + codebuddy (backdate expiry test).
- Unit tests: workos prefix, codebuddy header set, force-stream collect, poll-state machine.
