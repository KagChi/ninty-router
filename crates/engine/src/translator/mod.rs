//! Format translation, pivot through OpenAI.
//! Ported from the reference `open-sse/translator/` per the extracted spec.

pub mod gemini;
pub mod request;
pub mod response;
pub mod stream;

pub const DEFAULT_MAX_TOKENS: i64 = 64_000;
pub const DEFAULT_MIN_TOKENS_WITH_TOOLS: i64 = 32_000;

pub mod responses;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Openai,
    Claude,
    Gemini,
    Responses,
}

impl Format {
    pub fn name(self) -> &'static str {
        match self {
            Format::Openai => "openai",
            Format::Claude => "claude",
            Format::Gemini => "gemini",
            Format::Responses => "responses",
        }
    }
}

/// Claude stop_reason → OpenAI finish_reason.
pub fn to_openai_finish(claude: &str) -> &'static str {
    match claude {
        "max_tokens" => "length",
        "tool_use" => "tool_calls",
        _ => "stop", // end_turn, stop_sequence, anything else
    }
}

/// OpenAI finish_reason → Claude stop_reason.
pub fn from_openai_finish(openai: &str) -> &'static str {
    match openai {
        "length" => "max_tokens",
        "tool_calls" => "tool_use",
        _ => "end_turn", // stop, content_filter, anything else
    }
}

/// Translate any request to OpenAI format.
pub fn to_openai_request(
    src: Format,
    body: &serde_json::Value,
) -> Result<serde_json::Value, ninty_core::error::Error> {
    match src {
        Format::Openai => Ok(body.clone()),
        Format::Claude => request::claude_to_openai(body),
        Format::Gemini => gemini::gemini_to_openai(body),
        Format::Responses => Err(ninty_core::error::Error::BadRequest(
            "responses→openai request translation unsupported".into(),
        )),
    }
}

/// Translate an OpenAI request to any target format.
pub fn from_openai_request(
    dst: Format,
    body: &serde_json::Value,
) -> Result<serde_json::Value, ninty_core::error::Error> {
    match dst {
        Format::Openai => Ok(body.clone()),
        Format::Claude => request::openai_to_claude(body),
        Format::Gemini => gemini::openai_to_gemini(body),
        Format::Responses => responses::openai_to_responses(body),
    }
}

/// Translate a request body between formats via the OpenAI pivot.
pub fn translate_request(
    src: Format,
    dst: Format,
    body: &serde_json::Value,
) -> Result<serde_json::Value, ninty_core::error::Error> {
    if src == dst {
        return Ok(body.clone());
    }
    from_openai_request(dst, &to_openai_request(src, body)?)
}

/// Translate an upstream (non-streaming) response JSON to OpenAI format.
pub fn to_openai_json(
    src: Format,
    body: &serde_json::Value,
    model: &str,
) -> Result<serde_json::Value, ninty_core::error::Error> {
    match src {
        Format::Openai => Ok(body.clone()),
        Format::Claude => response::claude_json_to_openai(body, model),
        Format::Gemini => gemini::gemini_json_to_openai(body, model),
        Format::Responses => responses::responses_json_to_openai(body, model),
    }
}

/// Translate an OpenAI chat.completion JSON to any client format.
pub fn from_openai_json(
    dst: Format,
    body: &serde_json::Value,
    model: &str,
) -> Result<serde_json::Value, ninty_core::error::Error> {
    match dst {
        Format::Openai => Ok(body.clone()),
        Format::Claude => response::openai_json_to_claude(body, model),
        Format::Gemini => response::openai_json_to_gemini(body, model),
        Format::Responses => Ok(body.clone()), // clients never request responses format
    }
}

/// Translate a complete (non-streaming) response JSON between formats.
pub fn translate_response_json(
    src: Format,
    dst: Format,
    body: &serde_json::Value,
    model: &str,
) -> Result<serde_json::Value, ninty_core::error::Error> {
    if src == dst {
        return Ok(body.clone());
    }
    from_openai_json(dst, &to_openai_json(src, body, model)?, model)
}

/// max_tokens rule shared by both request directions:
/// default 64000; ≥32000 when tools present; > budget_tokens+1024 when thinking set;
/// clamp to ceiling.
pub fn adjust_max_tokens(body: &serde_json::Value, ceiling: i64) -> i64 {
    let mut max = body
        .get("max_tokens")
        .and_then(|v| v.as_i64())
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_MAX_TOKENS);
    let has_tools = body
        .get("tools")
        .and_then(|t| t.as_array())
        .map(|t| !t.is_empty())
        .unwrap_or(false);
    if has_tools && max < DEFAULT_MIN_TOKENS_WITH_TOOLS {
        max = DEFAULT_MIN_TOKENS_WITH_TOOLS;
    }
    if let Some(budget) = body
        .get("thinking")
        .and_then(|t| t.get("budget_tokens"))
        .and_then(|b| b.as_i64())
    {
        if max <= budget {
            max = budget + 1024;
        }
    }
    max.min(ceiling)
}
