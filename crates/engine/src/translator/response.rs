//! Non-streaming response JSON translators: claude ↔ openai.

use serde_json::{json, Value};

use super::{from_openai_finish, to_openai_finish};

pub fn claude_json_to_openai(body: &Value, model: &str) -> ninty_core::error::Result<Value> {
    let mut text = String::new();
    let mut tool_calls: Vec<Value> = Vec::new();
    for block in body
        .get("content")
        .and_then(|c| c.as_array())
        .cloned()
        .unwrap_or_default()
    {
        match block.get("type").and_then(|t| t.as_str()) {
            Some("text") => {
                if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                    text.push_str(t);
                }
            }
            Some("tool_use") => {
                tool_calls.push(json!({
                    "id": block.get("id").cloned().unwrap_or(Value::Null),
                    "type": "function",
                    "function": {
                        "name": block.get("name").cloned().unwrap_or(Value::Null),
                        "arguments": serde_json::to_string(block.get("input").unwrap_or(&json!({})))
                            .unwrap_or_else(|_| "{}".into()),
                    }
                }));
            }
            _ => {}
        }
    }

    let mut message = json!({"role": "assistant", "content": text});
    if !tool_calls.is_empty() {
        message["tool_calls"] = Value::Array(tool_calls);
    }

    let stop = body
        .get("stop_reason")
        .and_then(|s| s.as_str())
        .map(to_openai_finish)
        .unwrap_or("stop");

    let usage_in = body.get("usage");
    let input = usage_in
        .and_then(|u| u.get("input_tokens"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let output = usage_in
        .and_then(|u| u.get("output_tokens"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let cache_read = usage_in
        .and_then(|u| u.get("cache_read_input_tokens"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let cache_create = usage_in
        .and_then(|u| u.get("cache_creation_input_tokens"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let prompt = input + cache_read + cache_create;
    let mut usage = json!({
        "prompt_tokens": prompt,
        "completion_tokens": output,
        "total_tokens": prompt + output,
    });
    if cache_read > 0 || cache_create > 0 {
        let mut details = json!({});
        if cache_read > 0 {
            details["cached_tokens"] = json!(cache_read);
        }
        if cache_create > 0 {
            details["cache_creation_tokens"] = json!(cache_create);
        }
        usage["prompt_tokens_details"] = details;
    }

    Ok(json!({
        "id": body.get("id").and_then(|i| i.as_str()).map(|i| format!("chatcmpl-{i}")).unwrap_or_else(|| "chatcmpl-unknown".into()),
        "object": "chat.completion",
        "created": chrono::Utc::now().timestamp(),
        "model": body.get("model").and_then(|m| m.as_str()).unwrap_or(model),
        "choices": [{
            "index": 0,
            "message": message,
            "finish_reason": stop,
        }],
        "usage": usage,
    }))
}

/// OpenAI chat.completion → gemini generateContent response (wrapped in `response`).
pub fn openai_json_to_gemini(body: &Value, model: &str) -> ninty_core::error::Result<Value> {
    let choice = body
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|c| c.first())
        .cloned()
        .unwrap_or(Value::Null);
    let message = choice.get("message").cloned().unwrap_or(Value::Null);

    let mut parts: Vec<Value> = Vec::new();
    if let Some(t) = message.get("content").and_then(|c| c.as_str()) {
        if !t.is_empty() {
            parts.push(json!({"text": t}));
        }
    }
    if let Some(tcs) = message.get("tool_calls").and_then(|t| t.as_array()) {
        for tc in tcs {
            let args: Value = serde_json::from_str(
                tc.get("function")
                    .and_then(|f| f.get("arguments"))
                    .and_then(|a| a.as_str())
                    .unwrap_or("{}"),
            )
            .unwrap_or(json!({}));
            parts.push(json!({
                "functionCall": {
                    "name": tc.get("function").and_then(|f| f.get("name")).cloned().unwrap_or(Value::Null),
                    "args": args,
                }
            }));
        }
    }

    let finish = match choice.get("finish_reason").and_then(|f| f.as_str()) {
        Some("length") => "MAX_TOKENS",
        _ => "STOP",
    };

    let u = body.get("usage").cloned().unwrap_or(json!({}));
    let prompt = u.get("prompt_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
    let completion = u
        .get("completion_tokens")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    Ok(json!({
        "candidates": [{
            "content": {"role": "model", "parts": parts},
            "finishReason": finish,
            "index": 0,
        }],
        "usageMetadata": {
            "promptTokenCount": prompt,
            "candidatesTokenCount": completion,
            "totalTokenCount": prompt + completion,
        },
        "modelVersion": body.get("model").and_then(|m| m.as_str()).unwrap_or(model),
        "responseId": body.get("id").and_then(|i| i.as_str()).unwrap_or("unknown"),
    }))
}

pub fn openai_json_to_claude(body: &Value, model: &str) -> ninty_core::error::Result<Value> {
    let choice = body
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|c| c.first())
        .cloned()
        .unwrap_or(Value::Null);
    let message = choice.get("message").cloned().unwrap_or(Value::Null);

    let mut content: Vec<Value> = Vec::new();
    if let Some(t) = message.get("content").and_then(|c| c.as_str()) {
        if !t.is_empty() {
            content.push(json!({"type": "text", "text": t}));
        }
    }
    if let Some(tcs) = message.get("tool_calls").and_then(|t| t.as_array()) {
        for tc in tcs {
            let args = tc
                .get("function")
                .and_then(|f| f.get("arguments"))
                .and_then(|a| a.as_str())
                .unwrap_or("{}");
            let input: Value = serde_json::from_str(args).unwrap_or(json!({}));
            content.push(json!({
                "type": "tool_use",
                "id": tc.get("id").cloned().unwrap_or(Value::Null),
                "name": tc.get("function").and_then(|f| f.get("name")).cloned().unwrap_or(Value::Null),
                "input": input,
            }));
        }
    }

    let finish = choice
        .get("finish_reason")
        .and_then(|f| f.as_str())
        .map(from_openai_finish)
        .unwrap_or("end_turn");

    let usage_in = body.get("usage");
    let prompt = usage_in
        .and_then(|u| u.get("prompt_tokens"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let output = usage_in
        .and_then(|u| u.get("completion_tokens"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let id = body
        .get("id")
        .and_then(|i| i.as_str())
        .map(|i| i.strip_prefix("chatcmpl-").unwrap_or(i).to_string())
        .unwrap_or_else(|| "msg_unknown".into());

    Ok(json!({
        "id": id,
        "type": "message",
        "role": "assistant",
        "model": body.get("model").and_then(|m| m.as_str()).unwrap_or(model),
        "content": content,
        "stop_reason": finish,
        "stop_sequence": null,
        "usage": {"input_tokens": prompt, "output_tokens": output},
    }))
}
