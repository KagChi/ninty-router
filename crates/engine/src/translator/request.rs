//! Request translators: openai ↔ claude. Faithful port of the reference spec.

use serde_json::{json, Value};

use super::{adjust_max_tokens, DEFAULT_MAX_TOKENS};

/// Extract text from OpenAI/Claude content: string → as-is; array → text parts joined.
pub fn extract_text(content: &Value, sep: &str) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter(|p| p.get("type").and_then(|t| t.as_str()) == Some("text"))
            .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join(sep),
        _ => String::new(),
    }
}

/// Lone text part → bare string; else array as-is.
fn collapse_text_parts(parts: Vec<Value>) -> Value {
    if parts.len() == 1 && parts[0].get("type").and_then(|t| t.as_str()) == Some("text") {
        return parts[0].get("text").cloned().unwrap_or(Value::Null);
    }
    Value::Array(parts)
}

fn parse_data_uri(url: &str) -> Option<(String, String)> {
    let rest = url.strip_prefix("data:")?;
    let (mime, b64) = rest.split_once(";base64,")?;
    Some((mime.to_string(), b64.to_string()))
}

// ---------------------------------------------------------------- openai → claude

pub fn openai_to_claude(body: &Value) -> ninty_core::error::Result<Value> {
    let messages = body
        .get("messages")
        .and_then(|m| m.as_array())
        .cloned()
        .unwrap_or_default();

    // hoist system messages
    let mut system_parts: Vec<String> = Vec::new();
    for msg in &messages {
        if msg.get("role").and_then(|r| r.as_str()) == Some("system") {
            let content = msg.get("content").cloned().unwrap_or(Value::Null);
            let text = match &content {
                Value::String(s) => s.clone(),
                other => extract_text(other, "\n"),
            };
            if !text.is_empty() {
                system_parts.push(text);
            }
        }
    }

    // response_format → extra system parts
    if let Some(rf) = body.get("response_format") {
        match rf.get("type").and_then(|t| t.as_str()) {
            Some("json_schema") => {
                if let Some(schema) = rf.get("json_schema").and_then(|j| j.get("schema")) {
                    system_parts.push(format!(
                        "You must respond with valid JSON that strictly follows this JSON schema:\n```json\n{}\n```\nRespond ONLY with the JSON object, no other text.",
                        serde_json::to_string_pretty(schema).unwrap_or_default()
                    ));
                }
            }
            Some("json_object") => {
                system_parts.push(
                    "You must respond with valid JSON. Respond ONLY with a JSON object, no other text.".into(),
                );
            }
            _ => {}
        }
    }

    // message conversion + merging
    let mut out_messages: Vec<Value> = Vec::new();
    let mut cur_role: Option<&str> = None;
    let mut cur_parts: Vec<Value> = Vec::new();

    fn flush(out: &mut Vec<Value>, role: &mut Option<&str>, parts: &mut Vec<Value>) {
        if role.is_some() && !parts.is_empty() {
            out.push(json!({"role": role.take().unwrap(), "content": std::mem::take(parts)}));
        }
        *role = None;
        parts.clear();
    }

    for msg in &messages {
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user");
        if role == "system" {
            continue;
        }
        let (blocks, has_tool_use, has_tool_result) = openai_msg_to_claude_blocks(msg);
        let new_role = if role == "assistant" { "assistant" } else { "user" };

        if has_tool_result {
            flush(&mut out_messages, &mut cur_role, &mut cur_parts);
            let tr_blocks: Vec<Value> = blocks
                .iter()
                .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_result"))
                .cloned()
                .collect();
            out_messages.push(json!({"role": "user", "content": tr_blocks}));
            let rest: Vec<Value> = blocks
                .into_iter()
                .filter(|b| b.get("type").and_then(|t| t.as_str()) != Some("tool_result"))
                .collect();
            if !rest.is_empty() {
                cur_role = Some(new_role);
                cur_parts.extend(rest);
            }
            continue;
        }

        if cur_role != Some(new_role) {
            flush(&mut out_messages, &mut cur_role, &mut cur_parts);
            cur_role = Some(new_role);
        }
        cur_parts.extend(blocks);
        if has_tool_use {
            flush(&mut out_messages, &mut cur_role, &mut cur_parts);
        }
    }
    flush(&mut out_messages, &mut cur_role, &mut cur_parts);

    // system array
    let mut system_blocks = vec![json!({
        "type": "text",
        "text": "You are Claude Code, Anthropic's official CLI for Claude."
    })];
    if !system_parts.is_empty() {
        system_blocks.push(json!({
            "type": "text",
            "text": system_parts.join("\n"),
            "cache_control": {"type": "ephemeral", "ttl": "1h"}
        }));
    }

    let mut out = json!({
        "model": body.get("model").cloned().unwrap_or(Value::Null),
        "max_tokens": adjust_max_tokens(body, DEFAULT_MAX_TOKENS),
        "messages": out_messages,
        "system": system_blocks,
    });
    if let Some(t) = body.get("temperature") {
        out["temperature"] = t.clone();
    }
    if body.get("stream").is_some() {
        out["stream"] = body["stream"].clone();
    }

    // tools
    if let Some(tools) = body.get("tools").and_then(|t| t.as_array()) {
        let mut out_tools: Vec<Value> = Vec::new();
        for tool in tools {
            let ttype = tool.get("type").and_then(|t| t.as_str());
            match ttype {
                Some(t) if t != "function" => out_tools.push(tool.clone()),
                _ => {
                    let data = tool.get("function").unwrap_or(tool);
                    out_tools.push(json!({
                        "name": data.get("name").and_then(|n| n.as_str()).unwrap_or(""),
                        "description": data.get("description").and_then(|d| d.as_str()).unwrap_or(""),
                        "input_schema": data.get("parameters")
                            .or_else(|| data.get("input_schema"))
                            .cloned()
                            .unwrap_or(json!({"type": "object", "properties": {}, "required": []})),
                    }));
                }
            }
        }
        if let Some(last) = out_tools.last_mut() {
            last["cache_control"] = json!({"type": "ephemeral", "ttl": "1h"});
        }
        out["tools"] = Value::Array(out_tools);
    }

    // tool_choice
    out["tool_choice"] = match body.get("tool_choice") {
        None | Some(Value::Null) => json!({"type": "auto"}),
        Some(Value::String(s)) if s == "required" => json!({"type": "any"}),
        Some(Value::String(_)) => json!({"type": "auto"}),
        Some(Value::Object(o)) => {
            if let Some(name) = o
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())
            {
                json!({"type": "tool", "name": name})
            } else if o
                .get("type")
                .and_then(|t| t.as_str())
                .map(|t| ["auto", "any", "tool", "none"].contains(&t))
                .unwrap_or(false)
            {
                Value::Object(o.clone())
            } else {
                json!({"type": "auto"})
            }
        }
        Some(_) => json!({"type": "auto"}),
    };

    Ok(out)
}

/// OpenAI message → (claude blocks, has_tool_use, has_tool_result).
fn openai_msg_to_claude_blocks(msg: &Value) -> (Vec<Value>, bool, bool) {
    let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user");
    let content = msg.get("content").cloned().unwrap_or(Value::Null);
    let mut blocks: Vec<Value> = Vec::new();
    let mut has_tool_use = false;
    let mut has_tool_result = false;

    if role == "tool" {
        has_tool_result = true;
        blocks.push(json!({
            "type": "tool_result",
            "tool_use_id": msg.get("tool_call_id").cloned().unwrap_or(Value::Null),
            "content": content,
        }));
        return (blocks, has_tool_use, has_tool_result);
    }

    if role == "user" {
        match &content {
            Value::String(s) if !s.is_empty() => {
                blocks.push(json!({"type": "text", "text": s}));
            }
            Value::Array(parts) => {
                for part in parts {
                    match part.get("type").and_then(|t| t.as_str()) {
                        Some("text") => {
                            let text = part.get("text").and_then(|t| t.as_str()).unwrap_or("");
                            if !text.is_empty() {
                                blocks.push(json!({"type": "text", "text": text}));
                            }
                        }
                        Some("tool_result") => {
                            has_tool_result = true;
                            let mut b = json!({
                                "type": "tool_result",
                                "tool_use_id": part.get("tool_use_id").cloned().unwrap_or(Value::Null),
                                "content": part.get("content").cloned().unwrap_or(Value::Null),
                            });
                            if part.get("is_error").and_then(|e| e.as_bool()).unwrap_or(false) {
                                b["is_error"] = Value::Bool(true);
                            }
                            blocks.push(b);
                        }
                        Some("image_url") => {
                            if let Some(url) = part
                                .get("image_url")
                                .and_then(|i| i.get("url"))
                                .and_then(|u| u.as_str())
                            {
                                if let Some((mime, b64)) = parse_data_uri(url) {
                                    blocks.push(json!({
                                        "type": "image",
                                        "source": {"type": "base64", "media_type": mime, "data": b64}
                                    }));
                                } else if url.starts_with("http://") || url.starts_with("https://") {
                                    blocks.push(json!({
                                        "type": "image",
                                        "source": {"type": "url", "url": url}
                                    }));
                                }
                            }
                        }
                        Some("image") if part.get("source").is_some() => {
                            blocks.push(part.clone());
                        }
                        Some("file") => {
                            if let Some(fd) = part
                                .get("file")
                                .and_then(|f| f.get("file_data"))
                                .and_then(|d| d.as_str())
                            {
                                if let Some((mime, b64)) = parse_data_uri(fd) {
                                    if mime == "application/pdf" {
                                        blocks.push(json!({
                                            "type": "document",
                                            "source": {"type": "base64", "media_type": mime, "data": b64}
                                        }));
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        return (blocks, has_tool_use, has_tool_result);
    }

    // assistant
    match &content {
        Value::Array(parts) => {
            for part in parts {
                match part.get("type").and_then(|t| t.as_str()) {
                    Some("text") => {
                        let text = part.get("text").and_then(|t| t.as_str()).unwrap_or("");
                        if !text.is_empty() {
                            blocks.push(json!({"type": "text", "text": text}));
                        }
                    }
                    Some("tool_use") => {
                        has_tool_use = true;
                        blocks.push(part.clone());
                    }
                    Some("thinking") => {
                        let mut p = part.clone();
                        if let Some(o) = p.as_object_mut() {
                            o.remove("cache_control");
                        }
                        blocks.push(p);
                    }
                    _ => {}
                }
            }
        }
        other => {
            let text = match other {
                Value::String(s) => s.clone(),
                o => extract_text(o, "\n"),
            };
            if !text.is_empty() {
                blocks.push(json!({"type": "text", "text": text}));
            }
        }
    }
    if let Some(tool_calls) = msg.get("tool_calls").and_then(|t| t.as_array()) {
        for tc in tool_calls {
            if tc.get("type").and_then(|t| t.as_str()) != Some("function") {
                continue;
            }
            has_tool_use = true;
            let args = tc
                .get("function")
                .and_then(|f| f.get("arguments"))
                .cloned()
                .unwrap_or(Value::Null);
            let input = match &args {
                Value::String(s) => {
                    serde_json::from_str::<Value>(s).unwrap_or(Value::String(s.clone()))
                }
                other => other.clone(),
            };
            blocks.push(json!({
                "type": "tool_use",
                "id": tc.get("id").cloned().unwrap_or(Value::Null),
                "name": tc.get("function").and_then(|f| f.get("name")).cloned().unwrap_or(Value::Null),
                "input": input,
            }));
        }
    }

    (blocks, has_tool_use, has_tool_result)
}

// ---------------------------------------------------------------- claude → openai

pub fn claude_to_openai(body: &Value) -> ninty_core::error::Result<Value> {
    let mut out_messages: Vec<Value> = Vec::new();

    // system → system message (strip billing header)
    if let Some(system) = body.get("system") {
        let text = match system {
            Value::Array(parts) => parts
                .iter()
                .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                .map(strip_billing_header)
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("\n"),
            Value::String(s) => strip_billing_header(s).to_string(),
            _ => String::new(),
        };
        if !text.is_empty() {
            out_messages.push(json!({"role": "system", "content": text}));
        }
    }

    for msg in body
        .get("messages")
        .and_then(|m| m.as_array())
        .cloned()
        .unwrap_or_default()
    {
        match claude_msg_to_openai(&msg) {
            ClaudeMsgOut::One(v) => out_messages.push(v),
            ClaudeMsgOut::Many(vs) => out_messages.extend(vs),
            ClaudeMsgOut::Skip => {}
        }
    }

    fix_missing_tool_responses(&mut out_messages);

    let mut out = json!({
        "model": body.get("model").cloned().unwrap_or(Value::Null),
        "messages": out_messages,
    });
    if body.get("max_tokens").and_then(|m| m.as_i64()).filter(|v| *v > 0).is_some() {
        out["max_tokens"] = json!(adjust_max_tokens(body, DEFAULT_MAX_TOKENS));
    }
    if let Some(t) = body.get("temperature") {
        out["temperature"] = t.clone();
    }
    if let Some(s) = body.get("stream") {
        out["stream"] = s.clone();
    }
    if let Some(tools) = body.get("tools").and_then(|t| t.as_array()) {
        let out_tools: Vec<Value> = tools
            .iter()
            .map(|tool| {
                json!({
                    "type": "function",
                    "function": {
                        "name": tool.get("name").cloned().unwrap_or(Value::Null),
                        "description": tool.get("description").and_then(|d| d.as_str()).unwrap_or(""),
                        "parameters": tool.get("input_schema").cloned()
                            .unwrap_or(json!({"type": "object", "properties": {}})),
                    }
                })
            })
            .collect();
        out["tools"] = Value::Array(out_tools);
    }
    if let Some(choice) = body.get("tool_choice") {
        out["tool_choice"] = match choice {
            Value::Null => json!("auto"),
            Value::String(_) => choice.clone(),
            Value::Object(o) => match o.get("type").and_then(|t| t.as_str()) {
                Some("any") => json!("required"),
                Some("tool") => json!({
                    "type": "function",
                    "function": {"name": o.get("name").cloned().unwrap_or(Value::Null)}
                }),
                _ => json!("auto"),
            },
            _ => json!("auto"),
        };
    }
    if let Some(effort) = body
        .get("reasoning_effort")
        .or_else(|| body.get("reasoning").and_then(|r| r.get("effort")))
    {
        out["reasoning_effort"] = effort.clone();
    }
    if let Some(r) = body.get("reasoning") {
        out["reasoning"] = r.clone();
    }

    Ok(out)
}

fn strip_billing_header(s: &str) -> &str {
    let lower = s.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("x-anthropic-billing-header:") {
        let _ = rest;
        match s.find('\n') {
            Some(pos) => &s[pos + 1..],
            None => "",
        }
    } else {
        s
    }
}

enum ClaudeMsgOut {
    One(Value),
    Many(Vec<Value>),
    Skip,
}

fn claude_msg_to_openai(msg: &Value) -> ClaudeMsgOut {
    let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user");
    let content = msg.get("content").cloned().unwrap_or(Value::Null);

    if role == "system" {
        let text = match &content {
            Value::Array(parts) => parts
                .iter()
                .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("\n"),
            Value::String(s) => s.clone(),
            _ => String::new(),
        };
        if text.trim().is_empty() {
            return ClaudeMsgOut::Skip;
        }
        return ClaudeMsgOut::One(json!({
            "role": "user",
            "content": format!("<instructions>\n{text}\n</instructions>")
        }));
    }

    let eff_role = if role == "assistant" { "assistant" } else { "user" };

    match &content {
        Value::String(_) => ClaudeMsgOut::One(json!({"role": eff_role, "content": content})),
        Value::Array(blocks) => {
            let mut parts: Vec<Value> = Vec::new();
            let mut tool_calls: Vec<Value> = Vec::new();
            let mut tool_results: Vec<Value> = Vec::new();
            for block in blocks {
                match block.get("type").and_then(|t| t.as_str()) {
                    Some("text") => {
                        parts.push(json!({"type": "text", "text": block.get("text").cloned().unwrap_or(Value::Null)}));
                    }
                    Some("image") => {
                        if let Some(src) = block.get("source") {
                            if src.get("type").and_then(|t| t.as_str()) == Some("base64") {
                                let mime = src.get("media_type").and_then(|m| m.as_str()).unwrap_or("");
                                let data = src.get("data").and_then(|d| d.as_str()).unwrap_or("");
                                parts.push(json!({
                                    "type": "image_url",
                                    "image_url": {"url": format!("data:{mime};base64,{data}")}
                                }));
                            }
                        }
                    }
                    Some("tool_use") => {
                        tool_calls.push(json!({
                            "id": block.get("id").cloned().unwrap_or(Value::Null),
                            "type": "function",
                            "function": {
                                "name": block.get("name").cloned().unwrap_or(Value::Null),
                                "arguments": serde_json::to_string(
                                    block.get("input").unwrap_or(&json!({}))
                                ).unwrap_or_else(|_| "{}".into()),
                            }
                        }));
                    }
                    Some("tool_result") => {
                        let c = block.get("content").cloned().unwrap_or(Value::Null);
                        let content_out = match &c {
                            Value::String(_) => c,
                            Value::Array(parts2) => {
                                let text = parts2
                                    .iter()
                                    .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                                    .collect::<Vec<_>>()
                                    .join("\n");
                                if text.is_empty() {
                                    Value::String(serde_json::to_string(&c).unwrap_or_default())
                                } else {
                                    Value::String(text)
                                }
                            }
                            Value::Null => Value::String(String::new()),
                            other => Value::String(serde_json::to_string(other).unwrap_or_default()),
                        };
                        tool_results.push(json!({
                            "role": "tool",
                            "tool_call_id": block.get("tool_use_id").cloned().unwrap_or(Value::Null),
                            "content": content_out,
                        }));
                    }
                    _ => {}
                }
            }

            if !tool_results.is_empty() {
                let mut out = tool_results;
                if !parts.is_empty() {
                    out.push(json!({"role": "user", "content": collapse_text_parts(parts)}));
                }
                return ClaudeMsgOut::Many(out);
            }
            if !tool_calls.is_empty() {
                let mut m = json!({"role": "assistant", "tool_calls": tool_calls});
                if !parts.is_empty() {
                    m["content"] = collapse_text_parts(parts);
                }
                return ClaudeMsgOut::One(m);
            }
            if !parts.is_empty() {
                return ClaudeMsgOut::One(json!({"role": eff_role, "content": collapse_text_parts(parts)}));
            }
            if blocks.is_empty() {
                return ClaudeMsgOut::One(json!({"role": eff_role, "content": ""}));
            }
            ClaudeMsgOut::Skip
        }
        _ => ClaudeMsgOut::Skip,
    }
}

/// Insert `[No response received]` tool replies for unanswered tool_calls.
fn fix_missing_tool_responses(messages: &mut Vec<Value>) {
    let mut i = 0;
    while i < messages.len() {
        let ids: Vec<String> = {
            let msg = &messages[i];
            if msg.get("role").and_then(|r| r.as_str()) != Some("assistant") {
                i += 1;
                continue;
            }
            msg.get("tool_calls")
                .and_then(|t| t.as_array())
                .map(|tcs| {
                    tcs.iter()
                        .filter_map(|tc| tc.get("id").and_then(|id| id.as_str()).map(String::from))
                        .collect()
                })
                .unwrap_or_default()
        };
        if ids.is_empty() {
            i += 1;
            continue;
        }
        let mut responded: Vec<String> = Vec::new();
        let mut j = i + 1;
        while j < messages.len() {
            let m = &messages[j];
            if m.get("role").and_then(|r| r.as_str()) == Some("tool") {
                if let Some(id) = m.get("tool_call_id").and_then(|v| v.as_str()) {
                    responded.push(id.to_string());
                }
                j += 1;
            } else {
                break;
            }
        }
        let missing: Vec<&String> = ids.iter().filter(|id| !responded.contains(id)).collect();
        for (k, id) in missing.iter().enumerate() {
            messages.insert(
                j + k,
                json!({
                    "role": "tool",
                    "tool_call_id": id,
                    "content": "[No response received]"
                }),
            );
        }
        i = j + missing.len();
    }
}
