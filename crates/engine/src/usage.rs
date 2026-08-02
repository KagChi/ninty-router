//! Token usage estimation (fallback when upstream omits usage): chars / 4.

/// Estimate prompt tokens from an OpenAI-style request body.
pub fn estimate_prompt_tokens(body: &serde_json::Value) -> i64 {
    let mut chars = 0usize;
    if let Some(messages) = body.get("messages").and_then(|m| m.as_array()) {
        for msg in messages {
            chars += text_len(msg.get("content"));
            if let Some(t) = msg.get("tool_calls") {
                chars += t.to_string().len();
            }
        }
    }
    if let Some(t) = body.get("system") {
        chars += text_len(Some(t));
    }
    (chars / 4).max(1) as i64
}

fn text_len(content: Option<&serde_json::Value>) -> usize {
    match content {
        Some(serde_json::Value::String(s)) => s.len(),
        Some(serde_json::Value::Array(parts)) => parts
            .iter()
            .map(|p| {
                p.get("text")
                    .and_then(|t| t.as_str())
                    .map(|s| s.len())
                    .unwrap_or(0)
            })
            .sum(),
        Some(other) => other.to_string().len(),
        None => 0,
    }
}

pub fn estimate_completion_tokens(text: &str) -> i64 {
    (text.len() / 4) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimates_from_messages() {
        let body = serde_json::json!({
            "messages": [
                {"role": "system", "content": "you are helpful"},
                {"role": "user", "content": [{"type": "text", "text": "hello world"}]}
            ]
        });
        let n = estimate_prompt_tokens(&body);
        assert!((5..=12).contains(&n), "got {n}");
    }
}
