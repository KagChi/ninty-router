//! Provider registry: static data for built-in providers.
//! Ported from the reference `open-sse/providers/registry/*.js`.
//! Custom OpenAI-compatible nodes come from the DB, not here.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    ApiKey,
    OAuth,
    Free,
}

#[derive(Debug, Clone, Copy)]
pub struct ModelDef {
    pub id: &'static str,
    pub name: &'static str,
    /// Model id sent upstream when it differs from our id.
    pub upstream_model_id: Option<&'static str>,
    /// USD per 1M input tokens (0 = free/unknown).
    pub price_in: f64,
    /// USD per 1M output tokens.
    pub price_out: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireFormat {
    Openai,
    Claude,
    Gemini,
    /// OpenAI Responses API (codex).
    Responses,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthStyle {
    Bearer,
    XApiKey,
    /// API key as `?key=` query param (Gemini API).
    QueryKey,
    /// No auth needed; literal `Bearer public` sent (opencode free).
    PublicToken,
}

/// How the final upstream URL is built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlStyle {
    /// Static URL from transport.base_url + url_suffix.
    Plain,
    /// `{base}/{model}:{action}` with `?alt=sse` when streaming (Gemini).
    ModelAction,
    /// Vertex: projects/{p}/locations/{l}/publishers/google/models/{model}:{action}.
    VertexModelAction,
}

#[derive(Debug, Clone, Copy)]
pub struct Transport {
    pub base_url: &'static str,
    pub headers: &'static [(&'static str, &'static str)],
    pub force_stream: bool,
    pub timeout_ms: u64,
    pub format: WireFormat,
    pub auth: AuthStyle,
    pub url_style: UrlStyle,
    pub url_suffix: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct ProviderDef {
    pub id: &'static str,
    pub alias: &'static str,
    pub category: Category,
    pub display_name: &'static str,
    pub notice_url: Option<&'static str>,
    pub transport: Transport,
    /// Alternate transports picked when one matches the client's wire format
    /// (zero-translation fast path). First match wins.
    pub alt_transports: &'static [Transport],
    /// No credential needed (free providers); literal "public" bearer used.
    pub no_auth: bool,
    pub models: &'static [ModelDef],
}

const fn t(base_url: &'static str) -> Transport {
    Transport {
        base_url,
        headers: &[],
        force_stream: false,
        timeout_ms: 120_000,
        format: WireFormat::Openai,
        auth: AuthStyle::Bearer,
        url_style: UrlStyle::Plain,
        url_suffix: "",
    }
}

/// Claude-format transport with z.ai-style `?beta=true` suffix.
const fn tc(base_url: &'static str) -> Transport {
    Transport {
        format: WireFormat::Claude,
        auth: AuthStyle::XApiKey,
        url_suffix: "?beta=true",
        ..t(base_url)
    }
}

const fn m(id: &'static str, name: &'static str) -> ModelDef {
    ModelDef {
        id,
        name,
        upstream_model_id: None,
        price_in: 0.0,
        price_out: 0.0,
    }
}

const fn mu(id: &'static str, name: &'static str, upstream: &'static str) -> ModelDef {
    ModelDef {
        id,
        name,
        upstream_model_id: Some(upstream),
        price_in: 0.0,
        price_out: 0.0,
    }
}

pub static PROVIDERS: &[ProviderDef] = &[
    ProviderDef {
        id: "anthropic",
        alias: "an",
        category: Category::ApiKey,
        display_name: "Anthropic",
        notice_url: Some("https://console.anthropic.com/settings/keys"),
        transport: Transport {
            headers: &[("anthropic-version", "2023-06-01")],
            format: WireFormat::Claude,
            auth: AuthStyle::XApiKey,
            ..t("https://api.anthropic.com/v1/messages")
        },
        alt_transports: &[],
        no_auth: false,

        models: &[
            m("claude-opus-4.7", "Claude Opus 4.7"),
            m("claude-sonnet-4.6", "Claude Sonnet 4.6"),
            m("claude-haiku-4.5", "Claude Haiku 4.5"),
        ],
    },
    ProviderDef {
        id: "openrouter",
        alias: "or",
        category: Category::ApiKey,
        display_name: "OpenRouter",
        notice_url: Some("https://openrouter.ai/keys"),
        transport: Transport {
            headers: &[
                ("HTTP-Referer", "https://ninty-router.local"),
                ("X-Title", "ninty-router"),
            ],
            ..t("https://openrouter.ai/api/v1/chat/completions")
        },
        // passthrough: any model id accepted; static list is a convenience subset
        alt_transports: &[],
        no_auth: false,

        models: &[
            m("anthropic/claude-sonnet-4.6", "Claude Sonnet 4.6"),
            m("openai/gpt-5.4", "GPT-5.4"),
            m("google/gemini-3.1-pro-preview", "Gemini 3.1 Pro"),
            m("deepseek/deepseek-v4-flash", "DeepSeek V4 Flash"),
        ],
    },
    ProviderDef {
        id: "deepseek",
        alias: "ds",
        category: Category::ApiKey,
        display_name: "DeepSeek",
        notice_url: Some("https://platform.deepseek.com/api_keys"),
        transport: t("https://api.deepseek.com/chat/completions"),
        alt_transports: &[],
        no_auth: false,

        models: &[
            m("deepseek-chat", "DeepSeek Chat"),
            m("deepseek-reasoner", "DeepSeek Reasoner"),
        ],
    },
    ProviderDef {
        id: "groq",
        alias: "gq",
        category: Category::ApiKey,
        display_name: "Groq",
        notice_url: Some("https://console.groq.com/keys"),
        transport: t("https://api.groq.com/openai/v1/chat/completions"),
        alt_transports: &[],
        no_auth: false,

        models: &[
            m("llama-3.3-70b-versatile", "Llama 3.3 70B"),
            m("qwen-qwq-32b", "Qwen QwQ 32B"),
        ],
    },
    ProviderDef {
        id: "mistral",
        alias: "mi",
        category: Category::ApiKey,
        display_name: "Mistral",
        notice_url: Some("https://console.mistral.ai/api-keys"),
        transport: t("https://api.mistral.ai/v1/chat/completions"),
        alt_transports: &[],
        no_auth: false,

        models: &[
            m("mistral-large-latest", "Mistral Large"),
            m("codestral-latest", "Codestral"),
        ],
    },
    ProviderDef {
        id: "xai",
        alias: "xai",
        category: Category::ApiKey,
        display_name: "xAI",
        notice_url: Some("https://console.x.ai/"),
        transport: t("https://api.x.ai/v1/chat/completions"),
        alt_transports: &[],
        no_auth: false,

        models: &[
            m("grok-4.3", "Grok 4.3"),
            m("grok-code-fast-1", "Grok Code Fast 1"),
        ],
    },
    ProviderDef {
        id: "together",
        alias: "tg",
        category: Category::ApiKey,
        display_name: "Together AI",
        notice_url: Some("https://api.together.xyz/settings/api-keys"),
        transport: t("https://api.together.xyz/v1/chat/completions"),
        alt_transports: &[],
        no_auth: false,

        models: &[
            m(
                "Qwen/Qwen3-Coder-480B-A35B-Instruct-FP8",
                "Qwen3 Coder 480B",
            ),
            m("deepseek-ai/DeepSeek-V4-Pro", "DeepSeek V4 Pro"),
        ],
    },
    ProviderDef {
        id: "blackbox",
        alias: "bb",
        category: Category::ApiKey,
        display_name: "Blackbox AI",
        notice_url: Some("https://www.blackbox.ai/api-management"),
        transport: t("https://api.blackbox.ai/v1/chat/completions"),
        alt_transports: &[],
        no_auth: false,

        models: &[
            mu(
                "claude-fable-5",
                "Claude Fable 5",
                "blackboxai/anthropic/claude-fable-5",
            ),
            mu(
                "claude-opus-4.8",
                "Claude Opus 4.8",
                "blackboxai/anthropic/claude-opus-4.8",
            ),
            mu(
                "claude-sonnet-4.6",
                "Claude Sonnet 4.6",
                "blackboxai/anthropic/claude-sonnet-4.6",
            ),
            mu("gpt-5.5", "GPT-5.5", "blackboxai/openai/gpt-5.5"),
            mu(
                "gpt-5.4-pro",
                "GPT-5.4 Pro",
                "blackboxai/openai/gpt-5.4-pro",
            ),
            mu("gpt-5.4", "GPT-5.4", "blackboxai/openai/gpt-5.4"),
            mu(
                "gpt-5.3-codex",
                "GPT-5.3 Codex",
                "blackboxai/openai/gpt-5.3-codex",
            ),
            mu(
                "gpt-5.4-nano",
                "GPT-5.4 Nano",
                "blackboxai/openai/gpt-5.4-nano",
            ),
            mu(
                "deepseek-v4-flash",
                "DeepSeek V4 Flash",
                "blackboxai/deepseek/deepseek-v4-flash",
            ),
            mu("grok-4.3", "Grok 4.3", "blackboxai/x-ai/grok-4.3"),
        ],
    },
];


const CLAUDE_API_HEADERS: &[(&str, &str)] = &[
    ("Anthropic-Version", "2023-06-01"),
    ("Anthropic-Beta", "claude-code-20250219,interleaved-thinking-2025-05-14"),
];

static GLM_ALT: &[Transport] = &[t("https://api.z.ai/api/coding/paas/v4/chat/completions")];
static MINIMAX_ALT: &[Transport] = &[t("https://api.minimax.io/v1/chat/completions")];
static MINIMAX_CN_ALT: &[Transport] = &[t("https://api.minimaxi.com/v1/chat/completions")];
static KIMI_ALT: &[Transport] = &[t("https://api.kimi.com/coding/v1/chat/completions")];

/// OAuth subscription providers (M06). Kiro gated (eventstream, M08).
pub static OAUTH_PROVIDERS: &[ProviderDef] = &[
    ProviderDef {
        id: "claude",
        alias: "cc",
        category: Category::OAuth,
        display_name: "Claude Code (OAuth)",
        notice_url: Some("https://claude.ai/settings"),
        transport: Transport {
            headers: &[
                ("anthropic-version", "2023-06-01"),
                ("anthropic-beta", "claude-code-20250219,oauth-2025-04-20,interleaved-thinking-2025-05-14"),
                ("anthropic-dangerous-direct-browser-access", "true"),
                ("user-agent", "claude-cli/2.1.92 (external, sdk-cli)"),
                ("x-app", "cli"),
            ],
            format: WireFormat::Claude,
            auth: AuthStyle::Bearer,
            url_suffix: "?beta=true",
            ..t("https://api.anthropic.com/v1/messages")
        },
        alt_transports: &[],
        no_auth: false,
        models: &[
            m("claude-opus-5", "Claude Opus 5"),
            m("claude-fable-5", "Claude Fable 5"),
            m("claude-sonnet-5", "Claude Sonnet 5"),
            m("claude-haiku-4-5-20251001", "Claude 4.5 Haiku"),
        ],
    },
    ProviderDef {
        id: "codex",
        alias: "cx",
        category: Category::OAuth,
        display_name: "OpenAI Codex (OAuth)",
        notice_url: Some("https://chatgpt.com/codex"),
        transport: Transport {
            headers: &[
                ("originator", "codex_cli_rs"),
                ("user-agent", "codex_cli_rs/0.136.0"),
            ],
            format: WireFormat::Responses,
            auth: AuthStyle::Bearer,
            force_stream: true,
            ..t("https://chatgpt.com/backend-api/codex/responses")
        },
        alt_transports: &[],
        no_auth: false,
        models: &[
            m("gpt-5.6-sol", "GPT 5.6 Sol"),
            m("gpt-5.6-terra", "GPT 5.6 Terra"),
            m("gpt-5.6-luna", "GPT 5.6 Luna"),
            m("gpt-5.5", "GPT 5.5"),
            m("gpt-5.4", "GPT 5.4"),
            m("gpt-5.4-mini", "GPT 5.4 Mini"),
            m("gpt-5.3-codex-spark", "GPT 5.3 Codex Spark"),
        ],
    },
    ProviderDef {
        id: "github",
        alias: "gh",
        category: Category::OAuth,
        display_name: "GitHub Copilot (OAuth)",
        notice_url: Some("https://github.com/features/copilot"),
        transport: Transport {
            headers: &[
                ("copilot-integration-id", "vscode-chat"),
                ("editor-version", "vscode/1.110.0"),
                ("editor-plugin-version", "copilot-chat/0.38.0"),
                ("user-agent", "GitHubCopilotChat/0.38.0"),
                ("openai-intent", "conversation-panel"),
                ("x-github-api-version", "2025-04-01"),
                ("x-vscode-user-agent-library-version", "electron-fetch"),
                ("x-initiator", "user"),
            ],
            ..t("https://api.githubcopilot.com/chat/completions")
        },
        alt_transports: &[],
        no_auth: false,
        models: &[
            m("gpt-5.4", "GPT-5.4"),
            m("gpt-5.4-mini", "GPT-5.4 Mini"),
            m("gpt-5.3-codex", "GPT-5.3 Codex"),
            m("gpt-5.2-codex", "GPT-5.2 Codex"),
            m("gpt-5.2", "GPT-5.2"),
            m("claude-haiku-4.5", "Claude Haiku 4.5"),
            m("claude-sonnet-4.6", "Claude Sonnet 4.6"),
            m("claude-opus-4.7", "Claude Opus 4.7"),
            m("gemini-3.1-pro-preview", "Gemini 3.1 Pro"),
            m("grok-code-fast-1", "Grok Code Fast 1"),
        ],
    },
    ProviderDef {
        id: "cline",
        alias: "cl",
        category: Category::OAuth,
        display_name: "Cline",
        notice_url: Some("https://cline.bot"),
        transport: Transport {
            headers: &[
                ("http-referer", "https://cline.bot"),
                ("x-title", "Cline"),
            ],
            ..t("https://api.cline.bot/api/v1/chat/completions")
        },
        alt_transports: &[],
        no_auth: false,
        models: &[
            m("anthropic/claude-opus-4.7", "Claude Opus 4.7"),
            m("anthropic/claude-sonnet-4.6", "Claude Sonnet 4.6"),
            m("anthropic/claude-opus-4.6", "Claude Opus 4.6"),
            m("openai/gpt-5.3-codex", "GPT-5.3 Codex"),
            m("openai/gpt-5.4", "GPT-5.4"),
            m("google/gemini-3.1-pro-preview", "Gemini 3.1 Pro Preview"),
            m("google/gemini-3.1-flash-lite-preview", "Gemini 3.1 Flash Lite"),
            m("kwaipilot/kat-coder-pro", "KAT Coder Pro"),
        ],
    },
    ProviderDef {
        id: "codebuddy-cn",
        alias: "cbcn",
        category: Category::OAuth,
        display_name: "CodeBuddy CN",
        notice_url: Some("https://copilot.tencent.com"),
        transport: Transport {
            headers: &[
                ("user-agent", "CLI/2.108.1 CodeBuddy/2.108.1"),
                ("x-product", "SaaS"),
                ("x-ide-type", "CLI"),
                ("x-ide-name", "CLI"),
                ("x-requested-with", "XMLHttpRequest"),
                ("x-codebuddy-request", "1"),
            ],
            force_stream: true,
            ..t("https://copilot.tencent.com/v2/chat/completions")
        },
        alt_transports: &[],
        no_auth: false,
        models: &[
            m("glm-5.2", "GLM-5.2"),
            m("glm-5.1", "GLM-5.1"),
            m("glm-5.0", "GLM-5.0"),
            m("glm-5.0-turbo", "GLM-5.0-Turbo"),
            m("glm-5v-turbo", "GLM-5v-Turbo"),
            m("glm-4.7", "GLM-4.7"),
            m("minimax-m3", "MiniMax-M3"),
            m("minimax-m2.7", "MiniMax-M2.7"),
            m("kimi-k2.7", "Kimi-K2.7-Code"),
            m("kimi-k2.6", "Kimi-K2.6"),
            m("kimi-k2.5", "Kimi-K2.5"),
            m("hy3-preview", "Hy3 Preview"),
            m("deepseek-v4-pro", "DeepSeek-V4-Pro"),
            m("deepseek-v4-flash", "DeepSeek-V4-Flash"),
            m("deepseek-v3-2-volc", "DeepSeek-V3.2"),
        ],
    },
    ProviderDef {
        id: "codebuddy-intl",
        alias: "cbai",
        category: Category::OAuth,
        display_name: "CodeBuddy",
        notice_url: Some("https://www.codebuddy.ai/profile/keys"),
        transport: Transport {
            headers: &[
                ("user-agent", "IDE/2.108.1 CodeBuddy/2.108.1"),
                ("x-product", "SaaS"),
                ("x-ide-type", "IDE"),
                ("x-ide-name", "IDE"),
                ("x-requested-with", "XMLHttpRequest"),
                ("x-codebuddy-request", "1"),
            ],
            force_stream: true,
            ..t("https://www.codebuddy.ai/v2/chat/completions")
        },
        alt_transports: &[],
        no_auth: false,
        models: &[
            m("gemini-3.1-pro", "Gemini 3.1 Pro"),
            m("gemini-3.1-flash-lite", "Gemini 3.1 Flash Lite"),
            m("gemini-3.0-flash", "Gemini 3.0 Flash"),
            m("gemini-2.5-pro", "Gemini 2.5 Pro"),
            m("gemini-2.5-flash", "Gemini 2.5 Flash"),
            m("gpt-5.5", "GPT-5.5"),
            m("gpt-5.4", "GPT-5.4"),
            m("gpt-5.2", "GPT-5.2"),
            m("gpt-5.3-codex", "GPT-5.3 Codex"),
            m("gpt-5.2-codex", "GPT-5.2 Codex"),
            m("gpt-5.1", "GPT-5.1"),
            m("gpt-5.1-codex", "GPT-5.1 Codex"),
            m("gpt-5.1-codex-max", "GPT-5.1 Codex Max"),
            m("gpt-5.1-codex-mini", "GPT-5.1 Codex Mini"),
            m("deepseek-v3-2-volc", "DeepSeek V3.2"),
            m("claude-opus-4.6", "Claude Opus 4.6"),
            m("claude-opus-4.7-1m", "Claude Opus 4.7 (1M)"),
            m("kimi-k2.5", "Kimi K2.5"),
        ],
    },
    ProviderDef {
        id: "qoder",
        alias: "qd",
        category: Category::OAuth,
        display_name: "Qoder",
        notice_url: Some("https://qoder.com/account/integrations"),
        transport: Transport {
            // custom executor path (COSY-signed, envelope SSE); base unused
            force_stream: true,
            ..t("https://api3.qoder.sh")
        },
        alt_transports: &[],
        no_auth: false,
        models: &[
            m("auto", "Auto"),
            m("ultimate", "Ultimate"),
            m("performance", "Performance"),
            m("efficient", "Efficient"),
            m("lite", "Lite"),
            m("qmodel", "QModel"),
            m("qmodel_latest", "QModel Latest"),
            m("dmodel", "DModel"),
            m("dfmodel", "DFModel"),
            m("gm51model", "GM51 Model"),
            m("kmodel", "KModel"),
            m("mmodel", "MModel"),
        ],
    },
];

pub static EXTRA_PROVIDERS: &[ProviderDef] = &[
    ProviderDef {
        id: "gemini",
        alias: "gm",
        category: Category::ApiKey,
        display_name: "Google Gemini",
        notice_url: Some("https://aistudio.google.com/apikey"),
        transport: Transport {
            format: WireFormat::Gemini,
            auth: AuthStyle::QueryKey,
            url_style: UrlStyle::ModelAction,
            ..t("https://generativelanguage.googleapis.com/v1beta/models")
        },
        alt_transports: &[],
        no_auth: false,
        models: &[
            m("gemini-3.1-pro-preview", "Gemini 3.1 Pro"),
            m("gemini-3.1-flash-lite-preview", "Gemini 3.1 Flash Lite"),
            m("gemini-2.5-flash", "Gemini 2.5 Flash"),
        ],
    },
    ProviderDef {
        id: "glm",
        alias: "glm",
        category: Category::ApiKey,
        display_name: "GLM (z.ai)",
        notice_url: Some("https://z.ai/manage-apikey/apikey-list"),
        transport: Transport {
            headers: CLAUDE_API_HEADERS,
            ..tc("https://api.z.ai/api/anthropic/v1/messages")
        },
        alt_transports: GLM_ALT,
        no_auth: false,
        models: &[
            m("glm-5.2", "GLM-5.2"),
            m("glm-5.1", "GLM-5.1"),
            m("glm-5", "GLM-5"),
            m("glm-4.7", "GLM-4.7"),
            m("glm-4.6v", "GLM-4.6V"),
        ],
    },
    ProviderDef {
        id: "glm-cn",
        alias: "glm-cn",
        category: Category::ApiKey,
        display_name: "GLM CN (bigmodel)",
        notice_url: Some("https://open.bigmodel.cn/usercenter/proj-mgmt/apikeys"),
        transport: t("https://open.bigmodel.cn/api/coding/paas/v4/chat/completions"),
        alt_transports: &[],
        no_auth: false,
        models: &[
            m("glm-5.2", "GLM-5.2"),
            m("glm-5.1", "GLM-5.1"),
            m("glm-5", "GLM-5"),
            m("glm-4.7", "GLM-4.7"),
            m("glm-4.6", "GLM-4.6"),
            m("glm-4.5-air", "GLM-4.5-Air"),
        ],
    },
    ProviderDef {
        id: "minimax",
        alias: "mm",
        category: Category::ApiKey,
        display_name: "MiniMax",
        notice_url: Some("https://platform.minimax.io/user-center/basic-information/interface-key"),
        transport: Transport {
            headers: CLAUDE_API_HEADERS,
            ..tc("https://api.minimax.io/anthropic/v1/messages")
        },
        alt_transports: MINIMAX_ALT,
        no_auth: false,
        models: &[
            m("MiniMax-M3", "MiniMax-M3"),
            m("MiniMax-M2.7", "MiniMax-M2.7"),
            m("MiniMax-M2.5", "MiniMax-M2.5"),
            m("MiniMax-M2.1", "MiniMax-M2.1"),
        ],
    },
    ProviderDef {
        id: "minimax-cn",
        alias: "mmcn",
        category: Category::ApiKey,
        display_name: "MiniMax CN",
        notice_url: Some("https://www.minimaxi.com/user-center/basic-information/interface-key"),
        transport: Transport {
            headers: CLAUDE_API_HEADERS,
            ..tc("https://api.minimaxi.com/anthropic/v1/messages")
        },
        alt_transports: MINIMAX_CN_ALT,
        no_auth: false,
        models: &[
            m("MiniMax-M3", "MiniMax-M3"),
            m("MiniMax-M2.7", "MiniMax-M2.7"),
            m("MiniMax-M2.5", "MiniMax-M2.5"),
            m("MiniMax-M2.1", "MiniMax-M2.1"),
        ],
    },
    ProviderDef {
        id: "kimi",
        alias: "kimi",
        category: Category::ApiKey,
        display_name: "Kimi (Moonshot)",
        notice_url: Some("https://platform.moonshot.ai/console/api-keys"),
        transport: Transport {
            headers: CLAUDE_API_HEADERS,
            ..tc("https://api.kimi.com/coding/v1/messages")
        },
        alt_transports: KIMI_ALT,
        no_auth: false,
        models: &[
            m("kimi-k2.7-code", "Kimi K2.7 Code"),
            m("kimi-k2.6", "Kimi K2.6"),
            m("kimi-k2.5", "Kimi K2.5"),
            m("kimi-k2.5-thinking", "Kimi K2.5 Thinking"),
            m("kimi-latest", "Kimi Latest"),
        ],
    },
    ProviderDef {
        id: "opencode",
        alias: "oc",
        category: Category::Free,
        display_name: "OpenCode Free",
        notice_url: Some("https://opencode.ai"),
        transport: Transport {
            headers: &[("x-opencode-client", "desktop")],
            auth: AuthStyle::PublicToken,
            ..t("https://opencode.ai/zen/v1/chat/completions")
        },
        alt_transports: &[],
        no_auth: true,
        models: &[],
    },
    ProviderDef {
        id: "vertex",
        alias: "vx",
        category: Category::Free,
        display_name: "Vertex AI",
        notice_url: Some("https://console.cloud.google.com/"),
        transport: Transport {
            format: WireFormat::Gemini,
            auth: AuthStyle::Bearer,
            url_style: UrlStyle::VertexModelAction,
            ..t("https://aiplatform.googleapis.com")
        },
        alt_transports: &[],
        no_auth: false,
        models: &[
            m("gemini-3.1-pro-preview", "Gemini 3.1 Pro"),
            m("gemini-3.1-flash-lite-preview", "Gemini 3.1 Flash Lite"),
            m("gemini-3-flash-preview", "Gemini 3 Flash"),
            m("gemini-2.5-flash", "Gemini 2.5 Flash"),
        ],
    },
];

/// All built-in providers (core + extra).
pub fn all_providers() -> impl Iterator<Item = &'static ProviderDef> {
    PROVIDERS.iter().chain(OAUTH_PROVIDERS.iter()).chain(EXTRA_PROVIDERS.iter())
}

/// Find a provider by id or alias.
pub fn find_provider(id_or_alias: &str) -> Option<&'static ProviderDef> {
    PROVIDERS
        .iter()
        .chain(OAUTH_PROVIDERS.iter())
        .chain(EXTRA_PROVIDERS.iter())
        .find(|p| p.id == id_or_alias || p.alias == id_or_alias)
}

/// Find a model inside a provider definition.
pub fn find_model<'a>(provider: &'a ProviderDef, model: &str) -> Option<&'a ModelDef> {
    provider.models.iter().find(|m| m.id == model)
}

/// Resolve "provider/model" (alias-aware) to (provider, model_id).
/// Bare model ids resolve when the first segment is not a known provider:
/// returns None so callers can try prefix inference or custom nodes.
pub fn resolve(spec: &str) -> Option<(&'static ProviderDef, String)> {
    let (provider_part, model_part) = spec.split_once('/')?;
    let provider = find_provider(provider_part)?;
    if model_part.is_empty() {
        return None;
    }
    Some((provider, model_part.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_alias() {
        let (p, m) = resolve("bb/claude-opus-4.8").unwrap();
        assert_eq!(p.id, "blackbox");
        assert_eq!(m, "claude-opus-4.8");
    }

    #[test]
    fn upstream_mapping() {
        let p = find_provider("blackbox").unwrap();
        let m = find_model(p, "gpt-5.5").unwrap();
        assert_eq!(m.upstream_model_id, Some("blackboxai/openai/gpt-5.5"));
    }

    #[test]
    fn unknown_provider_none() {
        assert!(resolve("nope/model").is_none());
    }
}
