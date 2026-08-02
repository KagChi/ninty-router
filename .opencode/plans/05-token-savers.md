# 05 — Token savers: RTK full port, Caveman, Ponytail

Status: DONE. engine::rtk (12 filters + autodetect, exact JS port incl. caps/gates; 16 fixture
tests ≥30%), compress_messages (openai tool/responses + claude tool_result shapes, skip is_error),
engine::savers (caveman 6 levels + ponytail 3 levels verbatim prompts; format-aware injection:
claude array before last cache_control block, gemini systemInstruction, openai system/developer),
pipeline hook post-translation pre-executor (bypass x-9router-token-saver: off), rtk_saved_bytes
in usage meta, Settings UI (savers + gateway section). e2e: tool_result 27528→5153B via anthropic
upstream, bypass byte-exact. Note: read-numbered autodetect requires ≥250 lines in 1024-char
window (reference quirk, kept for parity).

## Goal

RTK compression (12 filters + autodetect) runs on every request post-translation;
Caveman/Ponytail system-prompt injection; per-request bypass header; settings UI.

## Tasks

1. `engine/rtk`: port from `$REF/open-sse/rtk/`:
   - gates: 500 bytes ≤ len ≤ 10 MiB; never grow; never empty; any error → original.
   - autodetect peek first 1024 chars, ordered rules (gitLog, gitDiff, gitStatus,
     buildOutput, grep, find, tree, ls, searchList, readNumbered, dedupLog, smartTruncate)
     — port regexes exactly from `rtk/autodetect.js`.
   - filters in `rtk/filters/`: git-diff (100-line/hunk cap, 3 ctx), git-status (10+10),
     git-log (200), build-output, grep (10/file), find (10/dir, 20 dirs), ls (noise-dir
     collapse, ext summary), tree (200), dedup-log (2000), search-list, read-numbered,
     smart-truncate (head 120 + tail 60 of ≥250 lines).
   - application points: openai `role:"tool"` (string|array), claude `tool_result`
     (skip is_error), responses `function_call_output`, gemini functionResponse parts.
2. Pipeline hook: after translation, before executor; skip when
   `x-9router-token-saver: off` header or settings.rtkEnabled=false.
3. `engine/savers/caveman`, `ponytail`: prompt text ported from `cavemanPrompts.js` /
   ponytail constants; injection format-aware (claude system array w/ cache_control,
   gemini systemInstruction, openai system message first). Levels: caveman on/off;
   ponytail lite|full|ultra. Settings: cavemanEnabled, ponytailEnabled, ponytailLevel.
4. Settings → Token Saver UI section (own Settings tab or page): toggles for RTK,
   Caveman, Ponytail level; show estimated savings stat (compressed vs original bytes
   recorded in usage meta).
5. Record `rtk_saved_bytes` into usage_history meta for analytics.

## Reference

`$REF/open-sse/rtk/**` (autodetect.js, constants.js, filters/*),
`open-sse/handlers/{caveman,ponytail}.js`, `cavemanPrompts.js`,
`handlers/chatCore.js` (saver order), dashboard `token-saver/page.js`.

## Done when

- Fixture tool outputs (git diff, grep, ls, tree, npm build log, 300-line file dump)
  each compress ≥30% and autodetect picks correct filter; unit test per filter + per rule.
- Round-trip: Claude Code tool_result through anthropic upstream gets compressed;
  bypass header leaves body untouched.
- Toggle in dashboard flips behaviour without restart.
