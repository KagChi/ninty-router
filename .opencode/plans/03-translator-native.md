# 03 — Translator + native providers (Anthropic, Gemini, GLM, MiniMax, Kimi, Vertex, OpenCode Free)

Status: DONE. Pivot translator (openai↔claude↔gemini, two-stage stream pipeline),
/v1/messages, /v1beta, anthropic/gemini/glm/glm-cn/minimax/minimax-cn/kimi/opencode/vertex
registry entries, vertex SA JWT mint, kimi X-Msh headers, opencode model fetch (10min kv cache).
35 tests. Deviation: gemini schema cleaning implements blocklist/type-flatten/ensure-object/
required-cleanup only (not full 9-step); thinking unified intent only via reasoning_content/
thought parts (level→budget mapping deferred).

## Goal

Format translation layer working pivot-through-OpenAI. Client can speak OpenAI, Claude,
or Gemini; providers include native Anthropic and Gemini families plus cheap tier.

## Tasks

1. `engine/translator`: trait `translate_request(src,dst,body)` / `translate_response(dst,src,chunk,state)`.
   Registry of direct routes; fallback src→openai→dst. Port:
   - request: openai→claude, claude→openai, openai→gemini, gemini→openai, openai→vertex,
     openai→responses, responses→openai
   - response (SSE, stateful): claude→openai, openai→claude, gemini→openai,
     openai→responses (chat SSE → Responses events), plus stream→JSON collectors
   - concerns: tool-call id fixing, finish reasons, thinking blocks, image blocks,
     param stripping per target (max_tokens defaults).
2. Endpoint format detection: `/v1/chat/completions`=openai, `/v1/messages`=claude,
   `/v1/responses`=responses, `/v1beta/...`=gemini (path model:method parsing).
   Body-shape sniffing for safety (contents→gemini, input→responses, etc.).
3. New endpoints: `POST /v1/messages` (+`count_tokens` → estimate), `POST /v1/responses`,
   `POST /v1beta/models/{model}:{generateContent|streamGenerateContent}` (accept
   `x-goog-api-key`).
4. Providers: anthropic (x-api-key + anthropic-version, claude transport), gemini
   (generateContent), vertex (service-account JSON → JWT bearer mint, project discovery),
   glm (two transports: z.ai coding `/paas/v4` openai + `/anthropic/v1/messages` claude,
   resolve by client format), minimax, kimi (header hooks from reference), opencode
   (no-auth; model list fetched from `https://opencode.ai/zen/v1/models`, cached 1h).
5. Multi-transport resolution: registry `transports[]`, pick transport matching client
   source format when available (zero-translation fast path).
6. Thinking/reasoning: unified thinking intent capture/apply per target format
   (port `translator/concerns/thinkingUnified.js` essentials).
7. Dashboard: model list per provider shows native ids + aliases; `/v1/models`
   includes new providers.

## Reference

`$REF/open-sse/translator/**` (esp. `request/`, `response/`, `concerns/`,
`formats/`), `app/api/v1beta/models/[...path]/route.js`, registry entries
`{anthropic,gemini,vertex,glm,minimax,kimi,opencode}.js`, `services/tokenRefresh/providers.js`
(vertex JWT section).

## Done when

- Claude Code (`/v1/messages`) works against OpenRouter (openai upstream) AND anthropic key.
- Gemini CLI-style call (`/v1beta`) works against gemini + openai upstreams.
- `stream:true` and `false` both correct in all three client formats; usage recorded.
- Unit tests: each translator pair on fixture payloads (system msg, tool calls, images, thinking).
