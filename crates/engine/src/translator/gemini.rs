//! Gemini ↔ OpenAI translation. Port of the reference spec.
//! Simplification vs reference: JSON-schema cleaning implements the blocklist,
//! type-array flattening, ensure-object-type and required cleanup — not the
//! full 9-step pipeline (const→enum, allOf merge, anyOf/oneOf flatten,
//! placeholder injection are omitted).

use serde_json::{json, Value};

use super::{adjust_max_tokens, DEFAULT_MAX_TOKENS};

const SAFETY: &[&str] = &[
    "HARM_CATEGORY_HATE_SPEECH",
    "HARM_CATEGORY_DANGEROUS_CONTENT",
    "HARM_CATEGORY_SEXUALLY_EXPLICIT",
    "HARM_CATEGORY_HARASSMENT",
    "HARM_CATEGORY_CIVIC_INTEGRITY",
];

pub fn sanitize_function_name(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "_.:-".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    if out.is_empty() {
        return "_unknown".into();
    }
    if !out
        .chars()
        .next()
        .map(|c| c.is_ascii_alphabetic() || c == '_')
        .unwrap_or(false)
    {
        out = format!("_{out}");
    }
    out.truncate(64);
    out
}

// ---------------------------------------------------------------- openai → gemini

pub fn openai_to_gemini(body: &Value) -> ninty_core::error::Result<Value> {
    let messages = body
        .get("messages")
        .and_then(|m| m.as_array())
        .cloned()
        .unwrap_or_default();

    // pre-pass: tool_call id → name; tool_call_id → response content
    let mut id_to_name: std::collections::HashMap<String, String> = Default::default();
    let mut tool_responses: std::collections::HashMap<String, String> = Default::default();
    for msg in &messages {
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
        if role == "assistant" {
            if let Some(tcs) = msg.get("tool_calls").and_then(|t| t.as_array()) {
                for tc in tcs {
                    if tc.get("type").and_then(|t| t.as_str()) == Some("function") {
                        if let (Some(id), Some(name)) = (
                            tc.get("id").and_then(|i| i.as_str()),
                            tc.get("function")
                                .and_then(|f| f.get("name"))
                                .and_then(|n| n.as_str()),
                        ) {
                            id_to_name.insert(id.to_string(), name.to_string());
                        }
                    }
                }
            }
        } else if role == "tool" {
            if let Some(id) = msg.get("tool_call_id").and_then(|i| i.as_str()) {
                let content = msg
                    .get("content")
                    .and_then(|c| c.as_str())
                    .unwrap_or("")
                    .to_string();
                tool_responses.insert(id.to_string(), content);
            }
        }
    }

    let mut contents: Vec<Value> = Vec::new();
    let mut system_instruction = Value::Null;

    for msg in &messages {
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user");
        let content = msg.get("content").cloned().unwrap_or(Value::Null);
        match role {
            "system" | "developer" => {
                if messages.len() > 1 {
                    let text = match &content {
                        Value::String(s) => s.clone(),
                        other => super::request::extract_text(other, ""),
                    };
                    system_instruction = json!({"role": "user", "parts": [{"text": text}]});
                } else {
                    let parts = content_to_parts(&content);
                    if !parts.is_empty() {
                        contents.push(json!({"role": "user", "parts": parts}));
                    }
                }
            }
            "user" => {
                let parts = content_to_parts(&content);
                if !parts.is_empty() {
                    contents.push(json!({"role": "user", "parts": parts}));
                }
            }
            "assistant" => {
                let mut parts: Vec<Value> = Vec::new();
                if let Some(rc) = msg.get("reasoning_content").and_then(|r| r.as_str()) {
                    parts.push(json!({"thought": true, "text": rc}));
                }
                let text = match &content {
                    Value::String(s) => s.clone(),
                    other => super::request::extract_text(other, ""),
                };
                if !text.is_empty() {
                    parts.push(json!({"text": text}));
                }
                if let Some(tcs) = msg.get("tool_calls").and_then(|t| t.as_array()) {
                    for tc in tcs {
                        if tc.get("type").and_then(|t| t.as_str()) != Some("function") {
                            continue;
                        }
                        let args_raw = tc
                            .get("function")
                            .and_then(|f| f.get("arguments"))
                            .and_then(|a| a.as_str())
                            .unwrap_or("{}");
                        let args: Value = serde_json::from_str(args_raw).unwrap_or(Value::Null);
                        parts.push(json!({
                            "functionCall": {
                                "id": tc.get("id").cloned().unwrap_or(Value::Null),
                                "name": sanitize_function_name(
                                    tc.get("function").and_then(|f| f.get("name")).and_then(|n| n.as_str()).unwrap_or("")
                                ),
                                "args": args,
                            }
                        }));
                    }
                }
                if !parts.is_empty() {
                    contents.push(json!({"role": "model", "parts": parts}));
                }
                // inline tool responses right after the model turn
                if let Some(tcs) = msg.get("tool_calls").and_then(|t| t.as_array()) {
                    let mut fr_parts: Vec<Value> = Vec::new();
                    for tc in tcs {
                        let Some(id) = tc.get("id").and_then(|i| i.as_str()) else {
                            continue;
                        };
                        let Some(raw) = tool_responses.get(id) else {
                            continue;
                        };
                        let name = id_to_name
                            .get(id)
                            .cloned()
                            .unwrap_or_else(|| id.to_string());
                        let result: Value = serde_json::from_str(raw)
                            .ok()
                            .filter(|v: &Value| v.is_object())
                            .unwrap_or_else(|| json!({"result": serde_json::from_str::<Value>(raw).unwrap_or(Value::String(raw.clone()))}));
                        fr_parts.push(json!({
                            "functionResponse": {
                                "id": id,
                                "name": sanitize_function_name(&name),
                                "response": result,
                            }
                        }));
                    }
                    if !fr_parts.is_empty() {
                        contents.push(json!({"role": "user", "parts": fr_parts}));
                    }
                }
            }
            "tool" => {} // consumed after assistant turn
            _ => {}
        }
    }

    let mut out = json!({
        "contents": normalize_contents(contents),
        "safetySettings": SAFETY.iter().map(|c| json!({"category": c, "threshold": "OFF"})).collect::<Vec<_>>(),
    });
    if !system_instruction.is_null() {
        out["systemInstruction"] = system_instruction;
    }

    let mut gen = json!({});
    gen["maxOutputTokens"] = json!(adjust_max_tokens(body, DEFAULT_MAX_TOKENS));
    if let Some(t) = body.get("temperature") {
        gen["temperature"] = t.clone();
    }
    if let Some(t) = body.get("top_p") {
        gen["topP"] = t.clone();
    }
    if let Some(t) = body.get("top_k") {
        gen["topK"] = t.clone();
    }
    out["generationConfig"] = gen;

    if let Some(tools) = body.get("tools").and_then(|t| t.as_array()) {
        let decls: Vec<Value> = tools
            .iter()
            .map(|t| {
                let (name, desc, params) = if t.get("input_schema").is_some() {
                    (
                        t.get("name").and_then(|n| n.as_str()).unwrap_or(""),
                        t.get("description").and_then(|d| d.as_str()).unwrap_or(""),
                        t.get("input_schema")
                            .cloned()
                            .unwrap_or(json!({"type":"object","properties":{}})),
                    )
                } else {
                    let f = t.get("function").unwrap_or(t);
                    (
                        f.get("name").and_then(|n| n.as_str()).unwrap_or(""),
                        f.get("description").and_then(|d| d.as_str()).unwrap_or(""),
                        f.get("parameters")
                            .cloned()
                            .unwrap_or(json!({"type":"object","properties":{}})),
                    )
                };
                json!({
                    "name": sanitize_function_name(name),
                    "description": desc,
                    "parameters": clean_schema(&params),
                })
            })
            .collect();
        if !decls.is_empty() {
            out["tools"] = json!([{"functionDeclarations": decls}]);
        }
    }

    Ok(out)
}

fn content_to_parts(content: &Value) -> Vec<Value> {
    match content {
        Value::String(s) => {
            if s.is_empty() {
                vec![]
            } else {
                vec![json!({"text": s})]
            }
        }
        Value::Array(parts) => parts
            .iter()
            .filter_map(|part| match part.get("type").and_then(|t| t.as_str()) {
                Some("text") => part
                    .get("text")
                    .and_then(|t| t.as_str())
                    .filter(|t| !t.is_empty())
                    .map(|t| json!({"text": t})),
                Some("image_url") => {
                    let url = part
                        .get("image_url")
                        .and_then(|i| i.get("url"))
                        .and_then(|u| u.as_str())
                        .unwrap_or("");
                    if let Some(rest) = url.strip_prefix("data:") {
                        let mime = rest.split(';').next().unwrap_or("image/png");
                        let data = rest.split_once(',').map(|x| x.1).unwrap_or("");
                        Some(json!({"inlineData": {"mime_type": mime, "data": data}}))
                    } else if url.starts_with("http://") || url.starts_with("https://") {
                        Some(json!({"fileData": {"fileUri": url, "mimeType": "image/*"}}))
                    } else {
                        None
                    }
                }
                Some("file") => part
                    .get("file")
                    .and_then(|f| f.get("file_data"))
                    .and_then(|d| d.as_str())
                    .and_then(|d| d.strip_prefix("data:"))
                    .map(|rest| {
                        let mime = rest.split(';').next().unwrap_or("application/octet-stream");
                        let data = rest.split_once(',').map(|x| x.1).unwrap_or("");
                        json!({"inlineData": {"mime_type": mime, "data": data}})
                    }),
                _ => None,
            })
            .collect(),
        _ => vec![],
    }
}

fn normalize_contents(contents: Vec<Value>) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    for c in contents {
        let parts_empty = c
            .get("parts")
            .and_then(|p| p.as_array())
            .map(|p| p.is_empty())
            .unwrap_or(true);
        if parts_empty || c.get("role").is_none() {
            continue;
        }
        if let Some(last) = out.last_mut() {
            if last.get("role") == c.get("role") {
                let mut last_parts = last
                    .get("parts")
                    .and_then(|p| p.as_array())
                    .cloned()
                    .unwrap_or_default();
                last_parts.extend(
                    c.get("parts")
                        .and_then(|p| p.as_array())
                        .cloned()
                        .unwrap_or_default(),
                );
                last["parts"] = Value::Array(last_parts);
                continue;
            }
        }
        out.push(c);
    }
    out
}

const SCHEMA_BLOCKLIST: &[&str] = &[
    "minLength",
    "maxLength",
    "exclusiveMinimum",
    "exclusiveMaximum",
    "minItems",
    "maxItems",
    "format",
    "default",
    "examples",
    "$schema",
    "$defs",
    "definitions",
    "const",
    "$ref",
    "$comment",
    "deprecated",
    "readOnly",
    "writeOnly",
    "additionalProperties",
    "propertyNames",
    "patternProperties",
    "anyOf",
    "oneOf",
    "allOf",
    "not",
    "dependencies",
    "dependentSchemas",
    "dependentRequired",
    "title",
    "if",
    "then",
    "else",
    "contentMediaType",
    "contentEncoding",
];

pub fn clean_schema(v: &Value) -> Value {
    match v {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, val) in map {
                if SCHEMA_BLOCKLIST.contains(&k.as_str()) || k.starts_with("x-") {
                    continue;
                }
                out.insert(k.clone(), clean_schema(val));
            }
            // flatten type arrays → first non-null
            if let Some(Value::Array(types)) = out.get("type").cloned() {
                let first = types
                    .iter()
                    .find(|t| t.as_str() != Some("null"))
                    .cloned()
                    .unwrap_or(Value::String("string".into()));
                out.insert("type".into(), first);
            }
            // has properties but no type → object
            if out.contains_key("properties") && !out.contains_key("type") {
                out.insert("type".into(), Value::String("object".into()));
            }
            // required ⊆ properties keys
            if let Some(Value::Array(req)) = out.get("required").cloned() {
                let filtered: Vec<Value> = match out.get("properties").and_then(|p| p.as_object()) {
                    Some(props) => req
                        .into_iter()
                        .filter(|r| r.as_str().map(|s| props.contains_key(s)).unwrap_or(false))
                        .collect(),
                    None => req,
                };
                if filtered.is_empty() {
                    out.remove("required");
                } else {
                    out.insert("required".into(), Value::Array(filtered));
                }
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(clean_schema).collect()),
        other => other.clone(),
    }
}

// ---------------------------------------------------------------- gemini → openai (request pivot)

pub fn gemini_to_openai(body: &Value) -> ninty_core::error::Result<Value> {
    let mut messages: Vec<Value> = Vec::new();

    if let Some(si) = body.get("systemInstruction") {
        let text: String = si
            .get("parts")
            .and_then(|p| p.as_array())
            .map(|parts| {
                parts
                    .iter()
                    .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default();
        if !text.is_empty() {
            messages.push(json!({"role": "system", "content": text}));
        }
    }

    for content in body
        .get("contents")
        .and_then(|c| c.as_array())
        .cloned()
        .unwrap_or_default()
    {
        let role = content
            .get("role")
            .and_then(|r| r.as_str())
            .unwrap_or("user");
        let eff_role = if role == "user" { "user" } else { "assistant" };
        let mut parts: Vec<Value> = Vec::new();
        let mut tool_calls: Vec<Value> = Vec::new();
        let mut tool_msg: Option<Value> = None;

        for part in content
            .get("parts")
            .and_then(|p| p.as_array())
            .cloned()
            .unwrap_or_default()
        {
            if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                parts.push(json!({"type": "text", "text": text}));
            } else if let Some(inline) = part.get("inlineData").or_else(|| part.get("inline_data"))
            {
                let mime = inline
                    .get("mimeType")
                    .or_else(|| inline.get("mime_type"))
                    .and_then(|m| m.as_str())
                    .unwrap_or("image/png");
                let data = inline.get("data").and_then(|d| d.as_str()).unwrap_or("");
                parts.push(json!({
                    "type": "image_url",
                    "image_url": {"url": format!("data:{mime};base64,{data}")}
                }));
            } else if let Some(fc) = part.get("functionCall") {
                let name = fc.get("name").and_then(|n| n.as_str()).unwrap_or("");
                tool_calls.push(json!({
                    "id": fc.get("id").and_then(|i| i.as_str()).map(String::from)
                        .unwrap_or_else(|| format!("call_{name}")),
                    "type": "function",
                    "function": {
                        "name": name,
                        "arguments": serde_json::to_string(fc.get("args").unwrap_or(&json!({})))
                            .unwrap_or_else(|_| "{}".into()),
                    }
                }));
            } else if let Some(fr) = part.get("functionResponse") {
                let name = fr.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let result = fr
                    .get("response")
                    .and_then(|r| r.get("result"))
                    .or_else(|| fr.get("response"))
                    .cloned()
                    .unwrap_or(json!({}));
                tool_msg = Some(json!({
                    "role": "tool",
                    "tool_call_id": fr.get("id").and_then(|i| i.as_str()).map(String::from)
                        .unwrap_or_else(|| format!("call_{name}")),
                    "content": serde_json::to_string(&result).unwrap_or_else(|_| "{}".into()),
                }));
                break;
            }
        }

        if let Some(tm) = tool_msg {
            messages.push(tm);
            continue;
        }
        if !tool_calls.is_empty() {
            let mut m = json!({"role": "assistant", "tool_calls": tool_calls});
            if !parts.is_empty() {
                m["content"] = collapse(parts);
            }
            messages.push(m);
        } else if !parts.is_empty() {
            messages.push(json!({"role": eff_role, "content": collapse(parts)}));
        }
    }

    let gen = body.get("generationConfig").cloned().unwrap_or(json!({}));
    let mut out = json!({
        "model": body.get("model").cloned().unwrap_or(Value::Null),
        "messages": messages,
    });
    if let Some(t) = gen.get("temperature") {
        out["temperature"] = t.clone();
    }
    if let Some(t) = gen.get("topP") {
        out["top_p"] = t.clone();
    }
    out["max_tokens"] = json!(adjust_max_tokens(
        &json!({"max_tokens": gen.get("maxOutputTokens").cloned().unwrap_or(Value::Null), "tools": body.get("tools").cloned().unwrap_or(Value::Null)}),
        DEFAULT_MAX_TOKENS
    ));
    if let Some(tools) = body.get("tools").and_then(|t| t.as_array()) {
        let mut out_tools: Vec<Value> = Vec::new();
        for tool in tools {
            if let Some(decls) = tool.get("functionDeclarations").and_then(|d| d.as_array()) {
                for d in decls {
                    out_tools.push(json!({
                        "type": "function",
                        "function": {
                            "name": d.get("name").cloned().unwrap_or(Value::Null),
                            "description": d.get("description").and_then(|x| x.as_str()).unwrap_or(""),
                            "parameters": d.get("parameters").cloned().unwrap_or(json!({"type":"object","properties":{}})),
                        }
                    }));
                }
            }
        }
        if !out_tools.is_empty() {
            out["tools"] = Value::Array(out_tools);
        }
    }

    Ok(out)
}

fn collapse(parts: Vec<Value>) -> Value {
    if parts.len() == 1 && parts[0].get("type").and_then(|t| t.as_str()) == Some("text") {
        return parts[0].get("text").cloned().unwrap_or(Value::Null);
    }
    Value::Array(parts)
}

// ---------------------------------------------------------------- gemini → openai (response)

pub fn gemini_finish_to_openai(reason: &str, tool_calls_seen: i64) -> &'static str {
    let mapped = match reason.to_ascii_uppercase().as_str() {
        "MAX_TOKENS" => "length",
        "SAFETY" | "RECITATION" | "BLOCKLIST" | "PROHIBITED_CONTENT" => "content_filter",
        _ => "stop",
    };
    if mapped == "stop" && tool_calls_seen > 0 {
        "tool_calls"
    } else {
        mapped
    }
}

/// Non-stream gemini JSON → openai chat.completion.
pub fn gemini_json_to_openai(body: &Value, model: &str) -> ninty_core::error::Result<Value> {
    let resp = body.get("response").unwrap_or(body);
    let candidate = resp
        .get("candidates")
        .and_then(|c| c.as_array())
        .and_then(|c| c.first())
        .cloned()
        .unwrap_or(Value::Null);

    let mut content = String::new();
    let mut reasoning = String::new();
    let mut tool_calls: Vec<Value> = Vec::new();
    for (i, part) in candidate
        .get("content")
        .and_then(|c| c.get("parts"))
        .and_then(|p| p.as_array())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .enumerate()
    {
        if part
            .get("thought")
            .and_then(|t| t.as_bool())
            .unwrap_or(false)
        {
            if let Some(t) = part.get("text").and_then(|t| t.as_str()) {
                reasoning.push_str(t);
            }
        } else if let Some(t) = part.get("text").and_then(|t| t.as_str()) {
            content.push_str(t);
        } else if let Some(fc) = part.get("functionCall") {
            let name = fc.get("name").and_then(|n| n.as_str()).unwrap_or("");
            tool_calls.push(json!({
                "id": format!("call_{name}_{i}"),
                "type": "function",
                "function": {
                    "name": name,
                    "arguments": serde_json::to_string(fc.get("args").unwrap_or(&json!({})))
                        .unwrap_or_else(|_| "{}".into()),
                }
            }));
        }
    }

    let mut message = json!({"role": "assistant"});
    if !content.is_empty() {
        message["content"] = Value::String(content);
    }
    if !reasoning.is_empty() {
        message["reasoning_content"] = Value::String(reasoning);
    }
    if !tool_calls.is_empty() {
        message["tool_calls"] = Value::Array(tool_calls.clone());
    }
    if message.get("content").is_none() && message.get("tool_calls").is_none() {
        message["content"] = Value::String(String::new());
    }

    let raw_finish = candidate
        .get("finishReason")
        .and_then(|f| f.as_str())
        .unwrap_or("STOP");
    let finish = gemini_finish_to_openai(raw_finish, tool_calls.len() as i64);

    let um = resp.get("usageMetadata").cloned().unwrap_or(json!({}));
    let prompt = um
        .get("promptTokenCount")
        .and_then(|v| v.as_i64())
        .unwrap_or(0)
        + um.get("thoughtsTokenCount")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
    let completion = um
        .get("candidatesTokenCount")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let total = um
        .get("totalTokenCount")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let mut usage = json!({
        "prompt_tokens": prompt,
        "completion_tokens": completion,
        "total_tokens": total,
    });
    let thoughts = um
        .get("thoughtsTokenCount")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    if thoughts > 0 {
        usage["completion_tokens_details"] = json!({"reasoning_tokens": thoughts});
    }

    let response_id = resp
        .get("responseId")
        .and_then(|r| r.as_str())
        .unwrap_or("unknown");
    Ok(json!({
        "id": format!("chatcmpl-{response_id}"),
        "object": "chat.completion",
        "created": chrono::Utc::now().timestamp(),
        "model": resp.get("modelVersion").and_then(|m| m.as_str()).unwrap_or(model),
        "choices": [{"index": 0, "message": message, "finish_reason": finish}],
        "usage": usage,
    }))
}

/// Streaming gemini → openai, stateful.
#[derive(Default)]
pub struct GeminiToOpenAI {
    message_id: Option<String>,
    model: String,
    function_index: i64,
    tool_call_count: i64,
    usage: Option<Value>,
    finish_sent: bool,
}

impl GeminiToOpenAI {
    pub fn new() -> Self {
        Self::default()
    }

    fn chunk(&self, delta: Value, finish: Option<&str>, with_usage: bool) -> Value {
        let mut c = json!({
            "id": format!("chatcmpl-{}", self.message_id.clone().unwrap_or_else(|| "unknown".into())),
            "object": "chat.completion.chunk",
            "created": chrono::Utc::now().timestamp(),
            "model": self.model,
            "choices": [{"index": 0, "delta": delta, "finish_reason": finish}],
        });
        if with_usage {
            if let Some(u) = &self.usage {
                c["usage"] = u.clone();
            }
        }
        c
    }

    pub fn handle(&mut self, chunk: &Value) -> Vec<Value> {
        let resp = chunk.get("response").unwrap_or(chunk);
        // usage may arrive on usage-only events; read before the candidates guard
        if let Some(um) = resp
            .get("usageMetadata")
            .or_else(|| chunk.get("usageMetadata"))
        {
            self.usage = Some(map_usage(um));
        }
        let Some(candidate) = resp
            .get("candidates")
            .and_then(|c| c.as_array())
            .and_then(|c| c.first())
        else {
            return vec![];
        };

        let mut out: Vec<Value> = Vec::new();
        if self.message_id.is_none() {
            self.message_id = Some(
                resp.get("responseId")
                    .and_then(|r| r.as_str())
                    .map(String::from)
                    .unwrap_or_else(|| format!("msg_{}", chrono::Utc::now().timestamp())),
            );
            self.model = resp
                .get("modelVersion")
                .and_then(|m| m.as_str())
                .unwrap_or("gemini")
                .to_string();
            out.push(self.chunk(json!({"role": "assistant"}), None, false));
        }

        for part in candidate
            .get("content")
            .and_then(|c| c.get("parts"))
            .and_then(|p| p.as_array())
            .cloned()
            .unwrap_or_default()
        {
            let is_thought = part
                .get("thought")
                .and_then(|t| t.as_bool())
                .unwrap_or(false);
            if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                if !text.is_empty() {
                    let delta = if is_thought {
                        json!({"reasoning_content": text})
                    } else {
                        json!({"content": text})
                    };
                    out.push(self.chunk(delta, None, false));
                }
            }
            if let Some(fc) = part.get("functionCall") {
                let name = fc.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let idx = self.function_index;
                self.function_index += 1;
                self.tool_call_count += 1;
                out.push(self.chunk(
                    json!({"tool_calls": [{
                        "id": format!("{name}-{}-{idx}", chrono::Utc::now().timestamp()),
                        "index": idx,
                        "type": "function",
                        "function": {
                            "name": name,
                            "arguments": serde_json::to_string(fc.get("args").unwrap_or(&json!({})))
                                .unwrap_or_else(|_| "{}".into()),
                        }
                    }]}),
                    None,
                    false,
                ));
            }
        }

        if let Some(reason) = candidate.get("finishReason").and_then(|f| f.as_str()) {
            let finish = gemini_finish_to_openai(reason, self.tool_call_count);
            self.finish_sent = true;
            out.push(self.chunk(json!({}), Some(finish), true));
        }

        out
    }

    pub fn usage(&self) -> Option<(i64, i64)> {
        let u = self.usage.as_ref()?;
        Some((
            u.get("prompt_tokens")?.as_i64()?,
            u.get("completion_tokens")?.as_i64()?,
        ))
    }

    pub fn flush(&mut self) -> Vec<Value> {
        if self.finish_sent || self.message_id.is_none() {
            return vec![];
        }
        self.finish_sent = true;
        let finish = if self.tool_call_count > 0 {
            "tool_calls"
        } else {
            "stop"
        };
        vec![self.chunk(json!({}), Some(finish), true)]
    }
}

fn map_usage(um: &Value) -> Value {
    let prompt = um
        .get("promptTokenCount")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let mut candidates = um
        .get("candidatesTokenCount")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let thoughts = um
        .get("thoughtsTokenCount")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let total = um
        .get("totalTokenCount")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    if candidates == 0 && total > 0 {
        candidates = (total - prompt - thoughts).max(0);
    }
    let cached = um
        .get("cachedContentTokenCount")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let mut usage = json!({
        "prompt_tokens": prompt,
        "completion_tokens": candidates + thoughts,
        "total_tokens": total,
    });
    if cached > 0 {
        usage["prompt_tokens_details"] = json!({"cached_tokens": cached});
    }
    if thoughts > 0 {
        usage["completion_tokens_details"] = json!({"reasoning_tokens": thoughts});
    }
    usage
}

/// Streaming openai → gemini, stateful. Emits gemini stream chunks.
#[derive(Default)]
pub struct OpenAIToGemini {
    sent_first: bool,
    response_id: String,
    model: String,
    tool_call_count: i64,
    prompt_tokens: i64,
    completion_tokens: i64,
    has_usage: bool,
    finish_sent: bool,
}

impl OpenAIToGemini {
    pub fn new() -> Self {
        Self::default()
    }

    fn wrap(&self, parts: Vec<Value>, finish: Option<&str>) -> Value {
        let mut candidate = json!({
            "content": {"role": "model", "parts": parts},
            "index": 0,
        });
        if let Some(f) = finish {
            candidate["finishReason"] = Value::String(f.to_string());
        }
        let mut out = json!({
            "candidates": [candidate],
            "modelVersion": self.model,
            "responseId": self.response_id,
        });
        if finish.is_some() && self.has_usage {
            out["usageMetadata"] = json!({
                "promptTokenCount": self.prompt_tokens,
                "candidatesTokenCount": self.completion_tokens,
                "totalTokenCount": self.prompt_tokens + self.completion_tokens,
            });
        }
        out
    }

    pub fn handle(&mut self, chunk: &Value) -> Vec<Value> {
        if chunk
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|c| c.first())
            .is_none()
        {
            // usage-only chunk
            if let Some(u) = chunk.get("usage") {
                self.prompt_tokens = u.get("prompt_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
                self.completion_tokens = u
                    .get("completion_tokens")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                self.has_usage = true;
            }
            return vec![];
        }
        if self.response_id.is_empty() {
            self.response_id = chunk
                .get("id")
                .and_then(|i| i.as_str())
                .unwrap_or("unknown")
                .to_string();
            self.model = chunk
                .get("model")
                .and_then(|m| m.as_str())
                .unwrap_or("gemini")
                .to_string();
        }
        if let Some(u) = chunk.get("usage") {
            self.prompt_tokens = u.get("prompt_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
            self.completion_tokens = u
                .get("completion_tokens")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            self.has_usage = true;
        }
        self.sent_first = true;

        let choice = &chunk["choices"][0];
        let delta = choice.get("delta").cloned().unwrap_or(json!({}));
        let mut out: Vec<Value> = Vec::new();

        let mut parts: Vec<Value> = Vec::new();
        if let Some(t) = delta.get("content").and_then(|c| c.as_str()) {
            if !t.is_empty() {
                parts.push(json!({"text": t}));
            }
        }
        if let Some(r) = delta.get("reasoning_content").and_then(|c| c.as_str()) {
            if !r.is_empty() {
                parts.push(json!({"thought": true, "text": r}));
            }
        }
        if let Some(tcs) = delta.get("tool_calls").and_then(|t| t.as_array()) {
            for tc in tcs {
                // gemini takes whole calls; emit when we see id+name (args buffered upstream rare)
                let name = tc
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("");
                if !name.is_empty() {
                    self.tool_call_count += 1;
                    let args: Value = serde_json::from_str(
                        tc.get("function")
                            .and_then(|f| f.get("arguments"))
                            .and_then(|a| a.as_str())
                            .unwrap_or("{}"),
                    )
                    .unwrap_or(json!({}));
                    parts.push(json!({"functionCall": {"name": name, "args": args}}));
                }
            }
        }
        if !parts.is_empty() {
            out.push(self.wrap(parts, None));
        }
        if let Some(finish) = choice.get("finish_reason").and_then(|f| f.as_str()) {
            self.finish_sent = true;
            let reason = match finish {
                "length" => "MAX_TOKENS",
                "content_filter" => "SAFETY",
                _ => "STOP",
            };
            out.push(self.wrap(vec![json!({"text": ""})], Some(reason)));
        }
        out
    }

    pub fn usage(&self) -> Option<(i64, i64)> {
        if self.has_usage {
            Some((self.prompt_tokens, self.completion_tokens))
        } else {
            None
        }
    }

    pub fn flush(&mut self) -> Vec<Value> {
        if self.finish_sent || !self.sent_first {
            return vec![];
        }
        self.finish_sent = true;
        vec![self.wrap(vec![json!({"text": ""})], Some("STOP"))]
    }
}
