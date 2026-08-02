# 06 — OAuth core: Claude Code, Codex, GitHub Copilot, Kiro + refresh pipeline

Status: DONE (deviations below). engine::oauth::pkce (RFC7636 vector test) + refresh
(claude/codex/github-copilot/kiro fns, should_refresh, codex stale-8d, copilot remint-60s),
registry: claude (cc), codex (cx, Responses wire format, force_stream), github (gh, copilot
headers); WireFormat::Responses + translator::responses (chat→responses, stream
ResponsesToOpenAI, json) + CODEX_DEFAULT_INSTRUCTIONS verbatim; collect_forced_stream
(non-stream clients over forced-stream upstream); server::oauth_state (proactive lead refresh +
401/403 reactive refresh-retry-once, codex id_token claims); /api/oauth/{authorize,exchange,
device-code,poll} (claude paste-code, codex loopback, github + kiro device flows); dashboard
Connect modal (authorize link + paste code / device code + auto-poll).
e2e (mock): codex stream + collect + claude→codex translation verified incl. upstream request
shape. Deviations: (1) no live OAuth e2e — needs real accounts; (2) claude tool cloaking
(_cc/_ide rename) not ported; (3) kiro chat path (eventstream) still gated to M08, device flow
implemented; (4) codex loopback :1455 server not implemented — paste-redirect-URL flow instead.

## Goal

Connect subscription accounts via OAuth from dashboard; tokens auto-refresh
(proactive + reactive on 401); chat works through each.

## Tasks

1. `engine/oauth`: shared primitives — PKCE (S256) gen/verify, state gen, loopback
   callback server (axum on ephemeral/fixed port), device-code poller, token store
   into provider_connections.data. Refresh scheduler: per request `should_refresh`
   (expires_at < now + refresh_lead_ms) → refresh with per-connection async mutex;
   on 401/403 from executor → refresh + retry once. Port refresh fns:
   `$REF/open-sse/services/tokenRefresh/providers.js` sections claude/codex/github/kiro.
2. Claude Code: authorize URL `https://claude.ai/oauth/authorize`
   (client_id `9d1c250a-…`, scopes `org:create_api_key user:profile user:inference`,
   `code=true`), user pastes `code#state`; token POST JSON
   `https://api.anthropic.com/v1/oauth/token`; refresh JSON same URL, lead 4h.
   Executor: claude-cli UA + anthropic-beta headers (port `claudeHeaderCache.js`
   essentials), tool cloaking `_cc`/`_ide` rename (port `claudeCloaking.js`).
3. Codex: PKCE, loopback server on **:1455** `/auth/callback`; authorize
   `https://auth.openai.com/oauth/authorize` (client_id `app_EMoamEEZ73f0CkXaXp7hrann`,
   scope `openid profile email offline_access`, extras `id_token_add_organizations`,
   `codex_cli_simplified_flow`, `originator=codex_cli_rs`); token form POST
   `https://auth.openai.com/oauth/token`; parse id_token JWT → chatgptAccountId/planType
   into providerSpecificData; refresh form POST, lead 5d, proactive stale at 8d.
   Executor: Responses-API upstream (`backend-api/codex/responses`), port from
   `executors/codex.js` incl. instructions block.
4. GitHub Copilot: device code (`client_id Iv1.b507a08c87ecfe98`, scope `read:user`),
   poll `login/oauth/access_token`; then GET `api.github.com/copilot_internal/v2/token`
   → copilotToken (+expiresAt) in providerSpecificData; re-mint when near expiry.
   Executor from `executors/github.js` (copilot headers, editor version).
5. Kiro: AWS SSO OIDC device flow — register client
   `oidc.{region}.amazonaws.com/client/register`, device_authorization, poll token
   (camelCase JSON); store clientId/clientSecret/region/authMethod/startUrl; refresh
   reuses stored client creds. Executor from `executors/kiro.js` — port the chat path
   (CodeWhisperer eventstream). If eventstream port too heavy, gate kiro behind
   feature flag and do in milestone 08.
6. Dashboard OAuth modal: per provider → open authorize URL / show device code +
   verification URI, poll status endpoint, paste-code fallback; show connected email,
   expiry, plan type. API: `GET|POST /api/oauth/{provider}/{authorize|exchange|device-code|poll|cancel}`.
7. Model catalogs: static registry lists for cc/cx/gh/kr (port from registry files);
   copilot live model fetch optional.

## Reference

`$REF/src/lib/oauth/providers/{claude,codex,github,kiro}.js`,
`open-sse/services/tokenRefresh/{providers,dedup}.js`, `services/oauthCredentialManager.js`,
`executors/{codex,github,kiro}.js`, registry `{claude,codex,github,kiro}.js`,
`app/api/oauth/[provider]/[action]/route.js`.

## Done when

- All four: connect via dashboard, make streaming chat request, token refresh triggered
  (force by backdating expires_at) without re-login.
- 401 from upstream → refresh → transparent retry succeeds.
- Unit tests: PKCE roundtrip, should_refresh lead logic, codex stale-8d rule, copilot re-mint.
