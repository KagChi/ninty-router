//! SSE line parsing utilities.

/// Incremental SSE buffer: feed bytes, get complete `data:` payloads back.
pub struct SseParser {
    buf: Vec<u8>,
    /// accumulated text of all streamed `content` deltas (for usage estimation)
    pub collected_text: String,
    pub done: bool,
}

impl Default for SseParser {
    fn default() -> Self {
        Self::new()
    }
}

impl SseParser {
    pub fn new() -> Self {
        Self {
            buf: Vec::with_capacity(8192),
            collected_text: String::new(),
            done: false,
        }
    }

    /// Feed a chunk; returns complete SSE events as raw `data:` payload strings.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<String> {
        self.buf.extend_from_slice(bytes);
        let mut events = Vec::new();
        loop {
            let Some(pos) = self.buf.windows(2).position(|w| w == b"\n\n") else {
                // also handle \r\n\r\n
                let Some(pos4) = self.buf.windows(4).position(|w| w == b"\r\n\r\n") else {
                    break;
                };
                let frame: Vec<u8> = self.buf.drain(..pos4 + 4).collect();
                if let Some(data) = extract_data(&frame) {
                    events.push(data);
                }
                continue;
            };
            let frame: Vec<u8> = self.buf.drain(..pos + 2).collect();
            if let Some(data) = extract_data(&frame) {
                events.push(data);
            }
        }
        for ev in &events {
            if ev.trim() == "[DONE]" {
                self.done = true;
            }
        }
        events
    }

    /// Remaining unterminated bytes (flush at end).
    pub fn finish(&mut self) -> Option<String> {
        if self.buf.is_empty() {
            return None;
        }
        let rest = std::mem::take(&mut self.buf);
        extract_data(&rest)
    }
}

fn extract_data(frame: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(frame);
    let mut payload = String::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(rest) = line.strip_prefix("data:") {
            if !payload.is_empty() {
                payload.push('\n');
            }
            payload.push_str(rest.strip_prefix(' ').unwrap_or(rest));
        }
    }
    if payload.is_empty() {
        None
    } else {
        Some(payload)
    }
}

/// Parse an OpenAI chat chunk JSON; extract usage + text delta. Tolerates
/// non-usage chunks and malformed JSON (returns defaults).
pub fn inspect_openai_chunk(payload: &str) -> (Option<(i64, i64)>, String) {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(payload) else {
        return (None, String::new());
    };
    let usage = v.get("usage").and_then(|u| {
        let p = u.get("prompt_tokens")?.as_i64()?;
        let c = u.get("completion_tokens")?.as_i64()?;
        Some((p, c))
    });
    let mut text = String::new();
    if let Some(choices) = v.get("choices").and_then(|c| c.as_array()) {
        for ch in choices {
            if let Some(t) = ch
                .get("delta")
                .and_then(|d| d.get("content"))
                .and_then(|c| c.as_str())
            {
                text.push_str(t);
            } else if let Some(t) = ch
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
            {
                text.push_str(t);
            }
        }
    }
    (usage, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_frames_across_chunk_boundaries() {
        let mut p = SseParser::new();
        assert_eq!(
            p.feed(b"data: {\"a\":1}\n\nda"),
            vec!["{\"a\":1}".to_string()]
        );
        let ev = p.feed(b"ta: [DONE]\n\n");
        assert_eq!(ev, vec!["[DONE]".to_string()]);
        assert!(p.done);
    }

    #[test]
    fn handles_crlf() {
        let mut p = SseParser::new();
        let ev = p.feed(b"data: hello\r\n\r\n");
        assert_eq!(ev, vec!["hello".to_string()]);
    }

    #[test]
    fn usage_and_text() {
        let payload = r#"{"choices":[{"delta":{"content":"hi"}}],"usage":{"prompt_tokens":3,"completion_tokens":1}}"#;
        let (u, t) = inspect_openai_chunk(payload);
        assert_eq!(u, Some((3, 1)));
        assert_eq!(t, "hi");
    }
}
