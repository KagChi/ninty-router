# ninty-router

Local AI router: one OpenAI-compatible endpoint on your machine, tiered provider
fallback, token-saver compression. Rust + Solid-Start, single binary.

Point any AI coding tool at `http://localhost:20128` and ninty-router routes the
request across your provider accounts — free tiers first, paid as fallback — with
automatic account rotation on rate limits, token-saving compression, and a
dashboard for connections, combos, quota, and usage.

## Quickstart

### Binary

```bash
# download a release asset, or build:
./scripts/build.sh        # web bundle + cargo release → target/release/ninty-router

./target/release/ninty-router            # dashboard opens automatically
./target/release/ninty-router --port 9000 --no-browser
```

Dashboard: <http://localhost:20128> — add a provider connection (API key or
OAuth), then point your tools at the endpoint.

### Docker

```bash
docker compose up --build      # or: docker build -t ninty-router . && docker run -p 20128:20128 -v ninty-data:/data ninty-router
```

### Dev

```bash
./scripts/dev.sh    # rust API on :20128 + vite dev on :3000 (proxied)
```

## Endpoint configuration

Base URL is always `http://localhost:20128` (or your `--port`). Any API key
works unless you enable *Require API key* in Settings (then create keys in the
dashboard).

| Tool | Setting | Value |
|---|---|---|
| Claude Code | `ANTHROPIC_BASE_URL` | `http://localhost:20128` |
| | `ANTHROPIC_AUTH_TOKEN` | any string / your router key |
| Cursor | OpenAI Base URL override | `http://localhost:20128/v1` |
| Cline (VS Code) | OpenAI Compatible provider | base `http://localhost:20128/v1` |
| Codex CLI | `OPENAI_BASE_URL` | `http://localhost:20128/v1` |
| Copilot-style | model `github/<model>` via `/v1/chat/completions` | — |

Endpoints:

- `POST /v1/chat/completions` — OpenAI chat (stream + non-stream)
- `GET  /v1/models` — OpenAI model list
- `POST /v1/messages` — Anthropic Messages (Claude Code native)
- `POST /v1beta/models/{model}:generateContent|streamGenerateContent` — Gemini

Model ids are `provider/model`, e.g. `deepseek/deepseek-chat`,
`claude/claude-sonnet-4-5`, `glm/glm-4.6`. Combo ids route across providers.

## Connecting providers

Dashboard → Providers → Connect:

- **API key providers** (openrouter, deepseek, groq, mistral, xai, together,
  blackbox, glm, kimi, minimax, anthropic): paste key, done. Multiple accounts
  per provider = automatic rotation.
- **OAuth providers** (claude, codex, github copilot, cline, codebuddy, qoder):
  click Connect, follow the browser/device flow. Tokens refresh automatically
  (proactive + on 401).
- **Vertex**: paste a service-account JSON; access tokens are minted locally.

## Fallback

Per model, accounts are tried in priority order:

- no credentials / 401–404 → 2 min lock, next account
- 429 / rate limit → exponential backoff (2s × 2^level, cap 5 min), next account
- credit/quota exhaustion (provider-specific patterns) → account deactivated
- 400/422 → surfaced to the client immediately (no fallback — your request is wrong)

**Combos** (Dashboard → Combos) chain models across providers:
`free = glm/glm-4.6 → openrouter/free → copilot/gpt-4o`. Strategies: fallback
(first healthy), round-robin with sticky limit.

## Token savers

Per-request bypass: header `x-9router-token-saver: off`.

| Saver | What | Typical saving |
|---|---|---|
| RTK | Compresses tool outputs (git diff/log/status, grep, ls, build logs…) | 20–40% on tool-heavy sessions |
| Caveman | Injects terse-response system prompt | shorter answers |
| Ponytail | Injects lazy-senior-dev prompt (minimal code bias, 3 levels) | fewer rewrites |
| PXPIPE | Renders bulky Claude-format system context as dense images | ~75% on huge system prompts |

PXPIPE needs an optional install (Settings → Token Savers → Install
pxpipe-proxy) and a JS runtime (node or bun) on the host; it fails open — if
anything is missing or times out, the request goes through untouched. Docker
image ships without node, so PXPIPE is unavailable there (all other savers work).

## Usage & quota

Dashboard → Usage: tokens/day chart, per-provider and per-model breakdown,
CSV export. Enable **Request logs** in Settings to capture per-request
client/provider bodies (truncated at 64KB, auth headers never stored, ring
buffer of 1000) — the Request Details tab shows exactly what was sent upstream
after savers ran.

Dashboard → Quota: remaining quota for OAuth providers that expose it
(codex, github copilot, claude, codebuddy), with reset countdowns.

> **Cost note:** like 9router, "cost" in the dashboard is a *savings tracker* —
> it shows what you avoided paying by routing to free tiers, not a bill.

## Data

Everything lives in `$DATA_DIR` (default `~/.ninty-router`, override with the
`DATA_DIR` env var): `db/data.sqlite` (WAL), OAuth tokens, pxpipe install.
Backup = copy the directory.

## Development

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
cd web && bun run build
```

Execution plans and architecture docs: [`.opencode/plans/`](.opencode/plans/).
