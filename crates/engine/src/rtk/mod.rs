//! RTK token-saver compression: autodetect + 12 filters.
//! Port of the reference `open-sse/rtk/` (constants.js, autodetect.js, filters/*).

pub mod autodetect;
pub mod filters;

pub const RAW_CAP: usize = 10 * 1024 * 1024;
pub const MIN_COMPRESS_SIZE: usize = 500;
pub const DETECT_WINDOW: usize = 1024;

#[derive(Debug, Clone)]
pub struct CompressResult {
    pub text: String,
    pub filter: Option<&'static str>,
    pub saved_bytes: i64,
}

type FilterFn = fn(&str) -> String;

pub fn resolve_filter(name: &str) -> Option<(&'static str, FilterFn)> {
    use filters::*;
    Some(match name {
        "git-diff" => ("git-diff", git_diff::git_diff as FilterFn),
        "git-status" => ("git-status", git_status::git_status),
        "git-log" => ("git-log", git_log::git_log),
        "build-output" => ("build-output", build_output::build_output),
        "grep" | "rg" => ("grep", grep::grep),
        "find" | "fd" => ("find", find::find),
        "ls" => ("ls", ls::ls),
        "tree" => ("tree", tree::tree),
        "dedup-log" => ("dedup-log", dedup_log::dedup_log),
        "smart-truncate" => ("smart-truncate", smart_truncate::smart_truncate),
        "read-numbered" => ("read-numbered", read_numbered::read_numbered),
        "search-list" => ("search-list", search_list::search_list),
        _ => return None,
    })
}

/// Compress one tool-output blob. Gates: size window, never grow, never empty.
pub fn compress(text: &str) -> CompressResult {
    let bytes_in = text.len();
    let unchanged = || CompressResult {
        text: text.to_string(),
        filter: None,
        saved_bytes: 0,
    };
    if !(MIN_COMPRESS_SIZE..=RAW_CAP).contains(&bytes_in) {
        return unchanged();
    }
    let Some((name, f)) = autodetect::auto_detect(text) else {
        return unchanged();
    };
    let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(text)))
        .unwrap_or_else(|_| text.to_string());
    if out.is_empty() || out.len() >= bytes_in {
        return unchanged();
    }
    CompressResult {
        saved_bytes: (bytes_in - out.len()) as i64,
        text: out,
        filter: Some(name),
    }
}

/// Stats over one request body compression pass.
#[derive(Debug, Default, Clone)]
pub struct Stats {
    pub bytes_before: u64,
    pub bytes_after: u64,
    pub hits: Vec<(String, String, i64)>, // (shape, filter, saved)
}

impl Stats {
    pub fn saved(&self) -> i64 {
        self.bytes_before as i64 - self.bytes_after as i64
    }
}

fn compress_text(text: &str, stats: &mut Stats, shape: &str) -> String {
    stats.bytes_before += text.len() as u64;
    let r = compress(text);
    match r.filter {
        Some(f) => {
            stats.bytes_after += r.text.len() as u64;
            stats.hits.push((shape.into(), f.into(), r.saved_bytes));
            r.text
        }
        None => {
            stats.bytes_after += text.len() as u64;
            text.to_string()
        }
    }
}

/// Compress tool-result content in an LLM request body (openai/claude shapes),
/// in place. Returns stats; None when body has no messages/input array.
pub fn compress_messages(body: &mut serde_json::Value) -> Option<Stats> {
    use serde_json::Value;
    let key = if body.get("messages").is_some() {
        "messages"
    } else if body.get("input").is_some() {
        "input"
    } else {
        return None;
    };
    let items = body.get_mut(key).and_then(Value::as_array_mut)?;
    let mut stats = Stats::default();
    for msg in items.iter_mut() {
        // openai responses: {type:"function_call_output", output}
        if msg.get("type").and_then(Value::as_str) == Some("function_call_output") {
            if let Some(out) = msg.get_mut("output") {
                if let Some(s) = out.as_str() {
                    let s = s.to_string();
                    *out = Value::String(compress_text(&s, &mut stats, "openai-responses-string"));
                } else if let Some(arr) = out.as_array_mut() {
                    for part in arr.iter_mut() {
                        if part.get("type").and_then(Value::as_str) == Some("input_text") {
                            if let Some(t) = part.get("text").and_then(Value::as_str) {
                                let t = t.to_string();
                                part["text"] =
                                    Value::String(compress_text(&t, &mut stats, "openai-responses-array"));
                            }
                        }
                    }
                }
            }
            continue;
        }
        let role = msg.get("role").and_then(Value::as_str).unwrap_or("");
        // openai tool message
        if role == "tool" {
            if let Some(content) = msg.get_mut("content") {
                match content {
                    Value::String(s) => {
                        let t = s.clone();
                        *content = Value::String(compress_text(&t, &mut stats, "openai-tool"));
                    }
                    Value::Array(parts) => {
                        for part in parts.iter_mut() {
                        if part.get("type").and_then(Value::as_str) == Some("text") {
                            if let Some(t) = part.get("text").and_then(Value::as_str) {
                                let t = t.to_string();
                                part["text"] =
                                    Value::String(compress_text(&t, &mut stats, "openai-tool-array"));
                            }
                        }
                        }
                    }
                    _ => {}
                }
            }
            continue;
        }
        // claude tool_result blocks
        let Some(blocks) = msg.get_mut("content").and_then(Value::as_array_mut) else {
            continue;
        };
        for block in blocks.iter_mut() {
            if block.get("type").and_then(Value::as_str) != Some("tool_result") {
                continue;
            }
            if block.get("is_error").and_then(Value::as_bool) == Some(true) {
                continue;
            }
            match block.get_mut("content") {
                Some(Value::String(s)) => {
                    let s = s.clone();
                    block["content"] = Value::String(compress_text(&s, &mut stats, "claude-string"));
                }
                Some(Value::Array(parts)) => {
                    for part in parts.iter_mut() {
                        if part.get("type").and_then(Value::as_str) == Some("text") {
                            if let Some(t) = part.get("text").and_then(Value::as_str) {
                                let t = t.to_string();
                                part["text"] =
                                    Value::String(compress_text(&t, &mut stats, "claude-array"));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    Some(stats)
}
