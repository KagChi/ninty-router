//! OpenAI Responses API (codex) ↔ OpenAI chat translation.

use serde_json::{json, Value};

use crate::codex_instructions::CODEX_DEFAULT_INSTRUCTIONS;

/// Chat completion request → Responses request (always stream, store=false).
pub fn openai_to_responses(body: &Value) -> ninty_core::error::Result<Value> {
    let mut input: Vec<Value> = vec![];
    let mut instructions: Option<String> = None;

    for m in body
        .get("messages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        let role = m.get("role").and_then(Value::as_str).unwrap_or("user");
        match role {
            "system" | "developer" => {
                let text = content_text(&m);
                instructions = Some(match instructions {
                    Some(prev) => format!("{prev}\n\n{text}"),
                    None => text,
                });
            }
            "tool" => {
                input.push(json!({
                    "type": "function_call_output",
                    "call_id": m.get("tool_call_id").and_then(Value::as_str).unwrap_or(""),
                    "output": content_text(&m),
                }));
            }
            _ => {
                // assistant tool_calls → function_call items
                if let Some(calls) = m.get("tool_calls").and_then(Value::as_array) {
                    for c in calls {
                        let f = c.get("function").cloned().unwrap_or(Value::Null);
                        input.push(json!({
                            "type": "function_call",
                            "call_id": c.get("id").and_then(Value::as_str).unwrap_or(""),
                            "name": f.get("name").and_then(Value::as_str).unwrap_or(""),
                            "arguments": f.get("arguments").and_then(Value::as_str).unwrap_or(""),
                        }));
                    }
                }
                let part_type = if role == "assistant" {
                    "output_text"
                } else {
                    "input_text"
                };
                input.push(json!({
                    "type": "message",
                    "role": role,
                    "content": [{"type": part_type, "text": content_text(&m)}],
                }));
            }
        }
    }

    let mut out = json!({
        "model": body.get("model").and_then(Value::as_str).unwrap_or(""),
        "input": input,
        "instructions": instructions.filter(|s| !s.trim().is_empty()).unwrap_or_else(|| CODEX_DEFAULT_INSTRUCTIONS.to_string()),
        "stream": true,
        "store": false,
    });
    if let Some(tools) = body.get("tools").and_then(Value::as_array) {
        let converted: Vec<Value> = tools
            .iter()
            .filter_map(|t| {
                let f = t.get("function")?;
                Some(json!({
                    "type": "function",
                    "name": f.get("name"),
                    "description": f.get("description"),
                    "parameters": f.get("parameters"),
                }))
            })
            .collect();
        if !converted.is_empty() {
            out["tools"] = Value::Array(converted);
        }
    }
    Ok(out)
}

fn content_text(m: &Value) -> String {
    match m.get("content") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|p| p.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

/// Responses (non-stream) JSON → chat.completion JSON.
pub fn responses_json_to_openai(body: &Value, model: &str) -> ninty_core::error::Result<Value> {
    let mut text = String::new();
    let mut tool_calls: Vec<Value> = vec![];
    if let Some(items) = body.get("output").and_then(Value::as_array) {
        for item in items {
            match item.get("type").and_then(Value::as_str) {
                Some("message") => {
                    if let Some(parts) = item.get("content").and_then(Value::as_array) {
                        for p in parts {
                            if let Some(t) = p.get("text").and_then(Value::as_str) {
                                text.push_str(t);
                            }
                        }
                    }
                }
                Some("function_call") => {
                    tool_calls.push(json!({
                        "id": item.get("call_id").or_else(|| item.get("id")).and_then(Value::as_str).unwrap_or(""),
                        "type": "function",
                        "function": {
                            "name": item.get("name").and_then(Value::as_str).unwrap_or(""),
                            "arguments": item.get("arguments").and_then(Value::as_str).unwrap_or(""),
                        },
                    }));
                }
                _ => {}
            }
        }
    }
    let mut message = json!({"role": "assistant", "content": text});
    if !tool_calls.is_empty() {
        message["tool_calls"] = Value::Array(tool_calls);
    }
    let usage = body.get("usage").cloned().unwrap_or(Value::Null);
    Ok(json!({
        "id": body.get("id").and_then(Value::as_str).unwrap_or("chatcmpl-resp"),
        "object": "chat.completion",
        "model": model,
        "choices": [{"index": 0, "message": message, "finish_reason": "stop"}],
        "usage": {
            "prompt_tokens": usage.get("input_tokens").and_then(Value::as_i64).unwrap_or(0),
            "completion_tokens": usage.get("output_tokens").and_then(Value::as_i64).unwrap_or(0),
        },
    }))
}

/// Streaming: Responses SSE events → openai chat chunks.
#[derive(Default)]
pub struct ResponsesToOpenAI {
    id: Option<String>,
    tool_index: i64,
    input_tokens: i64,
    output_tokens: i64,
    has_usage: bool,
    finish_sent: bool,
}

impl ResponsesToOpenAI {
    pub fn new() -> Self {
        Self::default()
    }

    fn chunk(&self, delta: Value, finish: Option<&str>) -> Value {
        let mut c = json!({
            "id": self.id.clone().unwrap_or_else(|| "chatcmpl-resp".into()),
            "object": "chat.completion.chunk",
            "choices": [{"index": 0, "delta": delta, "finish_reason": finish}],
        });
        if self.has_usage && finish.is_some() {
            c["usage"] = json!({
                "prompt_tokens": self.input_tokens,
                "completion_tokens": self.output_tokens,
            });
        }
        c
    }

    pub fn handle(&mut self, event: &Value) -> Vec<Value> {
        let ty = event.get("type").and_then(Value::as_str).unwrap_or("");
        match ty {
            "response.created" => {
                self.id = event
                    .get("response")
                    .and_then(|r| r.get("id"))
                    .and_then(Value::as_str)
                    .map(String::from);
                vec![self.chunk(json!({"role": "assistant"}), None)]
            }
            "response.output_text.delta" => {
                let d = event.get("delta").and_then(Value::as_str).unwrap_or("");
                if d.is_empty() {
                    vec![]
                } else {
                    vec![self.chunk(json!({"content": d}), None)]
                }
            }
            "response.output_item.done" => {
                let item = event.get("item").cloned().unwrap_or(Value::Null);
                if item.get("type").and_then(Value::as_str) == Some("function_call") {
                    self.tool_index += 1;
                    vec![self.chunk(
                        json!({"tool_calls": [{
                            "index": self.tool_index - 1,
                            "id": item.get("call_id").or_else(|| item.get("id")).and_then(Value::as_str).unwrap_or(""),
                            "type": "function",
                            "function": {
                                "name": item.get("name").and_then(Value::as_str).unwrap_or(""),
                                "arguments": item.get("arguments").and_then(Value::as_str).unwrap_or(""),
                            },
                        }]}),
                        None,
                    )]
                } else {
                    vec![]
                }
            }
            "response.completed" => {
                if let Some(u) = event.get("response").and_then(|r| r.get("usage")) {
                    self.input_tokens = u.get("input_tokens").and_then(Value::as_i64).unwrap_or(0);
                    self.output_tokens =
                        u.get("output_tokens").and_then(Value::as_i64).unwrap_or(0);
                    self.has_usage = true;
                }
                let finish = if self.tool_index > 0 {
                    "tool_calls"
                } else {
                    "stop"
                };
                self.finish_sent = true;
                vec![self.chunk(json!({}), Some(finish))]
            }
            _ => vec![],
        }
    }

    pub fn usage(&self) -> Option<(i64, i64)> {
        self.has_usage
            .then_some((self.input_tokens, self.output_tokens))
    }

    pub fn flush(&mut self) -> Vec<Value> {
        if self.finish_sent {
            return vec![];
        }
        self.finish_sent = true;
        vec![self.chunk(json!({}), Some("stop"))]
    }
}
