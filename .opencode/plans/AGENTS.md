# AGENTS.md — ninty-router agent behaviour

Rules for any agent executing plans in this directory. Read before touching code.

## Ground truth

- Reference implementation (Node.js, "9router") is read from a throwaway clone,
  referred to in all plans as `$REF`. NEVER hardcode an absolute path to it —
  it lives in a temp dir and may vanish between sessions.
- Before any porting task, resolve `$REF`:
  1. Check candidates in order: `$REF` env var, `${TMPDIR:-/tmp}/9router-ref`.
  2. If missing → clone fresh: `git clone --depth 1 https://github.com/KagChi/9router.git "${TMPDIR:-/tmp}/9router-ref"`.
  3. If upstream moved/changed, adapt from latest master; note drift in your status.
- When porting logic, READ the reference file first. Port behaviour, not style.
- If reference and plan disagree, reference wins for protocol details (URLs, headers,
  auth shapes, retry rules). Plan wins for architecture and scope.

## Scope discipline

- Execute ONE milestone file at a time, in order (01 → 09). Do not jump ahead.
- Do not build out-of-scope features: no media endpoints (TTS/STT/image/video),
  no fusion combos, no MITM, no cloud sync, no i18n framework, no CLI terminal UI.
- When unsure whether something is in scope, check `00-architecture.md` scope table.
  Still unsure → ask the user, do not guess.

## Engineering rules

- Rust edition 2021, `cargo fmt` + `cargo clippy -- -D warnings` clean before
  marking a milestone done.
- No `unwrap()` in request-path code. Errors flow through typed error enums
  (`thiserror`) and become proper OpenAI-shaped JSON error responses.
- Config-driven over code: new providers are registry data, not new executors.
  Specialized executor only when the protocol demands it (qoder, copilot, codex, kiro).
- SQLite access only through the db layer in `crates/server` (or `crates/core` if hoisted).
  All writes parameterized. WAL mode on.
- Never log API keys, tokens, or Authorization headers. Redact in tracing output.
- SSE code must handle: client disconnect (abort upstream), stall timeouts
  (first chunk 200s, inter-chunk 360s), non-SSE error bodies from upstream.
- Fallback must never loop forever. Every retry/fallback path has a bounded chain
  and returns the earliest `Retry-After` when exhausted.

## Token savers (RTK) invariants

- Compression NEVER grows output, NEVER returns empty, NEVER throws — any failure
  keeps the original text.
- Only compress tool-result text between 500 bytes and 10 MiB.
- Honour `x-9router-token-saver: off` header bypass.

## Frontend rules

- Solid-Start SPA, Tailwind v4, material-symbols icons. Dark mode default.
- All data via `/api/*` fetch. No direct DB, no server secrets in client bundle.
- Keep components small; one component per file; signals for local state.

## Verification per milestone

- Each milestone file ends with "Done when". ALL items must pass before moving on.
- Run `cargo test` (unit) and the smoke commands listed in the milestone.
- Actually run the binary and curl the endpoints. Do not claim done from compile alone.

## Git discipline

- Do NOT commit unless the user asks. Never push.
- Keep changes minimal and on-track. No drive-by refactors of working code.

## Communication

- Terse status updates. No summaries unless asked.
- Surface blockers immediately (upstream protocol drift, missing creds, ambiguous spec).
