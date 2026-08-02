//! Streaming response translators (SSE event → SSE event), stateful per request.
//! Faithful port of the reference response translators.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use super::{from_openai_finish, to_openai_finish};

fn now_secs() -> i64 {
    chrono::Utc::now().timestamp()
}

// ============================================================ claude → openai

#[derive(Default)]
pub struct ClaudeToOpenAI {
    message_id: Option<String>,
    model: String,
    tool_call_index: i64,
    // usage parts
    input_tokens: i64,
    cache_read: i64,
    cache_create: i64,
    output_tokens: i64,
    has_usage: bool,
    // block tracking
    tool_calls: BTreeMap<i64, (i64, String)>, // block_index → (tc_index, id)
    server_tool_block_index: i64,
    in_thinking_block: bool,
    current_block_index: i64,
    finish_reason: Option<&'static str>,
    finish_sent: bool,
}

impl ClaudeToOpenAI {
    pub fn new() -> Self {
        Self {
            server_tool_block_index: -1,
            current_block_index: -1,
            ..Default::default()
        }
    }

    fn chunk(&self, delta: Value, finish: Option<&str>) -> Value {
        let mut c = json!({
            "id": format!("chatcmpl-{}", self.message_id.clone().unwrap_or_else(|| "unknown".into())),
            "object": "chat.completion.chunk",
            "created": now_secs(),
            "model": self.model,
            "choices": [{"index": 0, "delta": delta, "finish_reason": finish}],
        });
        if finish.is_some() && self.has_usage {
            let prompt = self.input_tokens + self.cache_read + self.cache_create;
            let mut usage = json!({
                "prompt_tokens": prompt,
                "completion_tokens": self.output_tokens,
                "total_tokens": prompt + self.output_tokens,
            });
            if self.cache_read > 0 || self.cache_create > 0 {
                let mut details = json!({});
                if self.cache_read > 0 {
                    details["cached_tokens"] = json!(self.cache_read);
                }
                if self.cache_create > 0 {
                    details["cache_creation_tokens"] = json!(self.cache_create);
                }
                usage["prompt_tokens_details"] = details;
            }
            c["usage"] = usage;
        }
        c
    }

    pub fn handle(&mut self, event: &Value) -> Vec<Value> {
        let ty = event.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match ty {
            "message_start" => {
                let msg = event.get("message").cloned().unwrap_or(json!({}));
                self.message_id = Some(
                    msg.get("id")
                        .and_then(|i| i.as_str())
                        .map(String::from)
                        .unwrap_or_else(|| format!("msg_{}", now_secs())),
                );
                self.model = msg
                    .get("model")
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                self.tool_call_index = 0;
                if let Some(u) = msg.get("usage") {
                    self.input_tokens = num(u.get("input_tokens"));
                    self.cache_read = num(u.get("cache_read_input_tokens"));
                    self.cache_create = num(u.get("cache_creation_input_tokens"));
                    self.has_usage = true;
                }
                vec![self.chunk(json!({"role": "assistant"}), None)]
            }
            "content_block_start" => {
                let index = event.get("index").and_then(|i| i.as_i64()).unwrap_or(0);
                let block = event.get("content_block").cloned().unwrap_or(json!({}));
                match block.get("type").and_then(|t| t.as_str()) {
                    Some("server_tool_use") => {
                        self.server_tool_block_index = index;
                        vec![]
                    }
                    Some("text") | Some("thinking") => {
                        if block.get("type").and_then(|t| t.as_str()) == Some("thinking") {
                            self.in_thinking_block = true;
                            self.current_block_index = index;
                            return vec![self.chunk(json!({"content": "<think>"}), None)];
                        }
                        vec![]
                    }
                    Some("tool_use") => {
                        let tc_index = self.tool_call_index;
                        self.tool_call_index += 1;
                        let id = block.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string();
                        let name = block
                            .get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or("")
                            .to_string();
                        self.tool_calls.insert(index, (tc_index, id.clone()));
                        vec![self.chunk(
                            json!({"tool_calls": [{
                                "index": tc_index,
                                "id": id,
                                "type": "function",
                                "function": {"name": name, "arguments": ""}
                            }]}),
                            None,
                        )]
                    }
                    _ => vec![],
                }
            }
            "content_block_delta" => {
                let index = event.get("index").and_then(|i| i.as_i64()).unwrap_or(0);
                if index == self.server_tool_block_index {
                    return vec![];
                }
                let delta = event.get("delta").cloned().unwrap_or(json!({}));
                match delta.get("type").and_then(|t| t.as_str()) {
                    Some("text_delta") => {
                        let text = delta.get("text").and_then(|t| t.as_str()).unwrap_or("");
                        if text.is_empty() {
                            vec![]
                        } else {
                            vec![self.chunk(json!({"content": text}), None)]
                        }
                    }
                    Some("thinking_delta") => {
                        let t = delta.get("thinking").and_then(|t| t.as_str()).unwrap_or("");
                        if t.is_empty() {
                            vec![]
                        } else {
                            vec![self.chunk(json!({"reasoning_content": t}), None)]
                        }
                    }
                    Some("input_json_delta") => {
                        let partial = delta.get("partial_json").and_then(|p| p.as_str()).unwrap_or("");
                        if partial.is_empty() {
                            return vec![];
                        }
                        match self.tool_calls.get(&index) {
                            Some((tc_index, id)) => vec![self.chunk(
                                json!({"tool_calls": [{
                                    "index": tc_index,
                                    "id": id,
                                    "function": {"arguments": partial}
                                }]}),
                                None,
                            )],
                            None => vec![],
                        }
                    }
                    _ => vec![],
                }
            }
            "content_block_stop" => {
                let index = event.get("index").and_then(|i| i.as_i64()).unwrap_or(0);
                if index == self.server_tool_block_index {
                    self.server_tool_block_index = -1;
                    return vec![];
                }
                if self.in_thinking_block && index == self.current_block_index {
                    self.in_thinking_block = false;
                    return vec![self.chunk(json!({"content": "</think>"}), None)];
                }
                vec![]
            }
            "message_delta" => {
                if let Some(u) = event.get("usage") {
                    self.input_tokens = opt_num(u.get("input_tokens")).unwrap_or(self.input_tokens);
                    self.output_tokens = opt_num(u.get("output_tokens")).unwrap_or(0);
                    self.cache_read = opt_num(u.get("cache_read_input_tokens")).unwrap_or(self.cache_read);
                    self.cache_create =
                        opt_num(u.get("cache_creation_input_tokens")).unwrap_or(self.cache_create);
                    self.has_usage = true;
                }
                let stop = event
                    .get("delta")
                    .and_then(|d| d.get("stop_reason"))
                    .and_then(|s| s.as_str())
                    .map(String::from);
                if let Some(stop) = stop {
                    let finish = to_openai_finish(&stop);
                    self.finish_reason = Some(finish);
                    self.finish_sent = true;
                    return vec![self.chunk(json!({}), Some(finish))];
                }
                vec![]
            }
            "message_stop" => {
                if self.finish_sent {
                    return vec![];
                }
                self.finish_sent = true;
                let finish = self.finish_reason.unwrap_or(if !self.tool_calls.is_empty() {
                    "tool_calls"
                } else {
                    "stop"
                });
                vec![self.chunk(json!({}), Some(finish))]
            }
            _ => vec![],
        }
    }

    /// prompt/completion for usage recording.
    pub fn usage(&self) -> Option<(i64, i64)> {
        if self.has_usage {
            Some((
                self.input_tokens + self.cache_read + self.cache_create,
                self.output_tokens,
            ))
        } else {
            None
        }
    }

    /// Stream ended without message_stop/message_delta: emit finish chunk.
    pub fn flush(&mut self) -> Vec<Value> {
        if self.finish_sent || self.message_id.is_none() {
            return vec![];
        }
        self.finish_sent = true;
        let finish = self.finish_reason.unwrap_or(if !self.tool_calls.is_empty() {
            "tool_calls"
        } else {
            "stop"
        });
        vec![self.chunk(json!({}), Some(finish))]
    }
}

// ============================================================ openai → claude

#[derive(Default)]
pub struct OpenAIToClaude {
    message_start_sent: bool,
    message_id: String,
    model: String,
    next_block_index: i64,
    thinking_started: bool,
    thinking_index: i64,
    text_started: bool,
    text_index: i64,
    text_closed: bool,
    tool_calls: BTreeMap<i64, (String, String, i64)>, // tc idx → (id, name, block_index)
    arg_buffers: BTreeMap<i64, String>,
    input_tokens: i64,
    output_tokens: i64,
    has_usage: bool,
}

impl OpenAIToClaude {
    pub fn new() -> Self {
        Self {
            next_block_index: 0,
            thinking_index: -1,
            text_index: -1,
            ..Default::default()
        }
    }

    fn ensure_message_start(&mut self, chunk: &Value) -> Vec<Value> {
        if self.message_start_sent {
            return vec![];
        }
        self.message_start_sent = true;
        let raw_id = chunk.get("id").and_then(|i| i.as_str()).unwrap_or("");
        let stripped = raw_id.strip_prefix("chatcmpl-").unwrap_or(raw_id);
        self.message_id = if stripped.is_empty() || stripped == "chat" || stripped.len() < 8 {
            format!("msg_{}", now_secs())
        } else {
            stripped.to_string()
        };
        self.model = chunk
            .get("model")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown")
            .to_string();
        vec![json!({
            "type": "message_start",
            "message": {
                "id": self.message_id,
                "type": "message",
                "role": "assistant",
                "model": self.model,
                "content": [],
                "stop_reason": null,
                "stop_sequence": null,
                "usage": {"input_tokens": 0, "output_tokens": 0}
            }
        })]
    }

    fn stop_thinking(&mut self) -> Vec<Value> {
        if self.thinking_started {
            self.thinking_started = false;
            return vec![json!({"type": "content_block_stop", "index": self.thinking_index})];
        }
        vec![]
    }

    fn stop_text(&mut self) -> Vec<Value> {
        if self.text_started && !self.text_closed {
            self.text_closed = true;
            self.text_started = false;
            return vec![json!({"type": "content_block_stop", "index": self.text_index})];
        }
        vec![]
    }

    pub fn handle(&mut self, chunk: &Value) -> Vec<Value> {
        if chunk.get("choices").and_then(|c| c.as_array()).and_then(|c| c.first()).is_none() {
            return vec![];
        }

        // usage tracking
        if let Some(u) = chunk.get("usage") {
            let prompt = num(u.get("prompt_tokens"));
            self.output_tokens = num(u.get("completion_tokens"));
            let cached = u
                .get("prompt_tokens_details")
                .and_then(|d| d.get("cached_tokens"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let created = u
                .get("prompt_tokens_details")
                .and_then(|d| d.get("cache_creation_tokens"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            self.input_tokens = prompt - cached - created;
            self.has_usage = true;
        }

        let mut out = self.ensure_message_start(chunk);
        let choice = &chunk["choices"][0];
        let delta = choice.get("delta").cloned().unwrap_or(json!({}));

        // reasoning → thinking
        let reasoning = extract_reasoning(&delta);
        if !reasoning.is_empty() {
            out.extend(self.stop_text());
            if !self.thinking_started {
                self.thinking_index = self.next_block_index;
                self.next_block_index += 1;
                self.thinking_started = true;
                out.push(json!({
                    "type": "content_block_start",
                    "index": self.thinking_index,
                    "content_block": {"type": "thinking", "thinking": ""}
                }));
            }
            out.push(json!({
                "type": "content_block_delta",
                "index": self.thinking_index,
                "delta": {"type": "thinking_delta", "thinking": reasoning}
            }));
        }

        // text
        if let Some(text) = delta.get("content").and_then(|c| c.as_str()) {
            if !text.is_empty() {
                out.extend(self.stop_thinking());
                if !self.text_started {
                    self.text_index = self.next_block_index;
                    self.next_block_index += 1;
                    self.text_started = true;
                    self.text_closed = false;
                    out.push(json!({
                        "type": "content_block_start",
                        "index": self.text_index,
                        "content_block": {"type": "text", "text": ""}
                    }));
                }
                out.push(json!({
                    "type": "content_block_delta",
                    "index": self.text_index,
                    "delta": {"type": "text_delta", "text": text}
                }));
            }
        }

        // tool calls (open blocks on id; buffer args)
        if let Some(tcs) = delta.get("tool_calls").and_then(|t| t.as_array()) {
            for tc in tcs {
                let idx = tc.get("index").and_then(|i| i.as_i64()).unwrap_or(0);
                if let Some(id) = tc.get("id").and_then(|i| i.as_str()) {
                    if !self.tool_calls.contains_key(&idx) {
                        out.extend(self.stop_thinking());
                        out.extend(self.stop_text());
                        let block_index = self.next_block_index;
                        self.next_block_index += 1;
                        let raw_name = tc
                            .get("function")
                            .and_then(|f| f.get("name"))
                            .and_then(|n| n.as_str())
                            .unwrap_or("");
                        let name = raw_name.strip_prefix("proxy_").unwrap_or(raw_name).to_string();
                        self.tool_calls.insert(idx, (id.to_string(), name.clone(), block_index));
                        out.push(json!({
                            "type": "content_block_start",
                            "index": block_index,
                            "content_block": {"type": "tool_use", "id": id, "name": name, "input": {}}
                        }));
                    }
                }
                if let Some(args) = tc
                    .get("function")
                    .and_then(|f| f.get("arguments"))
                    .and_then(|a| a.as_str())
                {
                    if !args.is_empty() && self.tool_calls.contains_key(&idx) {
                        self.arg_buffers.entry(idx).or_default().push_str(args);
                    }
                }
            }
        }

        // finish
        if let Some(finish) = choice.get("finish_reason").and_then(|f| f.as_str()) {
            out.extend(self.stop_thinking());
            out.extend(self.stop_text());
            let tool_calls = std::mem::take(&mut self.tool_calls);
            for (idx, (_id, name, block_index)) in &tool_calls {
                if let Some(args) = self.arg_buffers.get(idx) {
                    let sanitized = sanitize_tool_args(name, args);
                    out.push(json!({
                        "type": "content_block_delta",
                        "index": block_index,
                        "delta": {"type": "input_json_delta", "partial_json": sanitized}
                    }));
                }
                out.push(json!({"type": "content_block_stop", "index": block_index}));
            }
            let usage = if self.has_usage {
                json!({"input_tokens": self.input_tokens, "output_tokens": self.output_tokens})
            } else {
                json!({"input_tokens": 0, "output_tokens": 0})
            };
            out.push(json!({
                "type": "message_delta",
                "delta": {"stop_reason": from_openai_finish(finish)},
                "usage": usage
            }));
            out.push(json!({"type": "message_stop"}));
        }

        out
    }

    pub fn usage(&self) -> Option<(i64, i64)> {
        if self.has_usage {
            Some((self.input_tokens, self.output_tokens))
        } else {
            None
        }
    }

    /// Stream ended without finish_reason: close open blocks, flush buffered tool
    /// args, emit terminal message_delta/message_stop.
    pub fn flush(&mut self) -> Vec<Value> {
        if !self.message_start_sent {
            return vec![];
        }
        let mut out = self.stop_thinking();
        out.extend(self.stop_text());
        let tool_calls = std::mem::take(&mut self.tool_calls);
        for (idx, (_id, name, block_index)) in &tool_calls {
            if let Some(args) = self.arg_buffers.get(idx) {
                let sanitized = sanitize_tool_args(name, args);
                out.push(json!({
                    "type": "content_block_delta",
                    "index": block_index,
                    "delta": {"type": "input_json_delta", "partial_json": sanitized}
                }));
            }
            out.push(json!({"type": "content_block_stop", "index": block_index}));
        }
        let usage = if self.has_usage {
            json!({"input_tokens": self.input_tokens, "output_tokens": self.output_tokens})
        } else {
            json!({"input_tokens": 0, "output_tokens": 0})
        };
        let stop_reason = if tool_calls.is_empty() { "end_turn" } else { "tool_use" };
        out.push(json!({
            "type": "message_delta",
            "delta": {"stop_reason": stop_reason},
            "usage": usage
        }));
        out.push(json!({"type": "message_stop"}));
        self.message_start_sent = false; // flushed; no double terminal events
        out
    }
}

fn extract_reasoning(delta: &Value) -> String {
    if let Some(r) = delta.get("reasoning_content").and_then(|r| r.as_str()) {
        return r.to_string();
    }
    if let Some(r) = delta.get("reasoning").and_then(|r| r.as_str()) {
        return r.to_string();
    }
    if let Some(details) = delta.get("reasoning_details").and_then(|d| d.as_array()) {
        return details
            .iter()
            .map(|d| {
                if let Some(s) = d.as_str() {
                    s.to_string()
                } else {
                    d.get("text")
                        .or_else(|| d.get("content"))
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string()
                }
            })
            .collect();
    }
    String::new()
}

/// Port of reference sanitizeToolArgs (Read-tool numeric coercions).
fn sanitize_tool_args(name: &str, args_json: &str) -> String {
    let name = name.strip_prefix("proxy_").unwrap_or(name);
    let Ok(mut v) = serde_json::from_str::<Value>(args_json) else {
        return args_json.to_string();
    };
    if name == "Read" {
        if let Some(obj) = v.as_object_mut() {
            for key in ["limit", "offset"] {
                if let Some(Value::String(s)) = obj.get(key) {
                    if let Ok(n) = s.parse::<i64>() {
                        obj.insert(key.into(), json!(n));
                    }
                }
            }
            if let Some(n) = obj.get("limit").and_then(|v| v.as_i64()) {
                if n > 2000 {
                    obj.insert("limit".into(), json!(2000));
                } else if n < 1 {
                    obj.remove("limit");
                }
            }
            if let Some(n) = obj.get("offset").and_then(|v| v.as_i64()) {
                if n < 0 {
                    obj.insert("offset".into(), json!(0));
                }
            }
            let pages_valid = obj
                .get("file_path")
                .and_then(|f| f.as_str())
                .map(|f| f.to_ascii_lowercase().ends_with(".pdf"))
                .unwrap_or(false)
                && obj
                    .get("pages")
                    .and_then(|p| p.as_str())
                    .map(|p| {
                        let mut parts = p.split('-');
                        let ok_len = matches!(p.matches('-').count(), 0 | 1);
                        ok_len && parts.all(|x| !x.is_empty() && x.chars().all(|c| c.is_ascii_digit()))
                    })
                    .unwrap_or(false);
            if obj.contains_key("pages") && !pages_valid {
                obj.remove("pages");
            }
        }
    }
    serde_json::to_string(&v).unwrap_or_else(|_| args_json.to_string())
}

fn num(v: Option<&Value>) -> i64 {
    v.and_then(|v| v.as_i64()).unwrap_or(0)
}

fn opt_num(v: Option<&Value>) -> Option<i64> {
    v.and_then(|v| v.as_i64())
}
