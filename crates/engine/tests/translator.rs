use engine::translator::stream::{ClaudeToOpenAI, OpenAIToClaude};
use engine::translator::{request, response};
use serde_json::json;

#[test]
fn openai_to_claude_basic() {
    let body = json!({
        "model": "claude-x",
        "max_tokens": 100,
        "messages": [
            {"role": "system", "content": "be terse"},
            {"role": "user", "content": "hi"},
            {"role": "assistant", "content": "hello"},
            {"role": "user", "content": [{"type": "text", "text": "again"}]},
        ],
    });
    let out = request::openai_to_claude(&body).unwrap();
    assert_eq!(out["max_tokens"], 100);
    let system = out["system"].as_array().unwrap();
    assert_eq!(system.len(), 2);
    assert!(system[1]["text"].as_str().unwrap().contains("be terse"));
    let msgs = out["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 3);
    assert_eq!(msgs[0]["role"], "user");
    assert_eq!(msgs[1]["role"], "assistant");
    assert_eq!(msgs[2]["role"], "user");
}

#[test]
fn openai_to_claude_tool_calls_and_results() {
    let body = json!({
        "model": "m",
        "max_tokens": 100,
        "messages": [
            {"role": "assistant", "content": "checking", "tool_calls": [
                {"id": "call_1", "type": "function", "function": {"name": "Bash", "arguments": "{\"cmd\":\"ls\"}"}}
            ]},
            {"role": "tool", "tool_call_id": "call_1", "content": "file.txt"},
            {"role": "user", "content": "thanks"},
        ],
        "tools": [{"type": "function", "function": {"name": "Bash", "description": "run", "parameters": {"type": "object"}}}],
    });
    let out = request::openai_to_claude(&body).unwrap();
    // tools present → max_tokens raised to 32000
    assert_eq!(out["max_tokens"], 32000);
    let msgs = out["messages"].as_array().unwrap();
    // assistant(tool_use) flushed, then tool_result alone, then user text
    assert_eq!(msgs[0]["role"], "assistant");
    let blocks = msgs[0]["content"].as_array().unwrap();
    assert!(blocks.iter().any(|b| b["type"] == "tool_use"));
    assert_eq!(msgs[1]["content"][0]["type"], "tool_result");
    assert_eq!(msgs[1]["content"][0]["tool_use_id"], "call_1");
    assert_eq!(msgs[2]["role"], "user");
    // tools translated + last has cache_control
    let tools = out["tools"].as_array().unwrap();
    assert_eq!(tools[0]["name"], "Bash");
    assert!(tools[0]["input_schema"].is_object());
    assert!(tools.last().unwrap()["cache_control"].is_object());
}

#[test]
fn openai_to_claude_images() {
    let body = json!({
        "model": "m",
        "messages": [{"role": "user", "content": [
            {"type": "image_url", "image_url": {"url": "data:image/png;base64,AAA"}},
            {"type": "image_url", "image_url": {"url": "https://x.com/a.png"}},
            {"type": "text", "text": "what is this"},
        ]}],
    });
    let out = request::openai_to_claude(&body).unwrap();
    let content = out["messages"][0]["content"].as_array().unwrap();
    assert_eq!(content[0]["source"]["type"], "base64");
    assert_eq!(content[1]["source"]["type"], "url");
    assert_eq!(content[2]["type"], "text");
}

#[test]
fn claude_to_openai_basic() {
    let body = json!({
        "model": "m",
        "max_tokens": 500,
        "system": [{"type": "text", "text": "x-anthropic-billing-header: abc\nreal system"}],
        "messages": [
            {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "t1", "content": "result"}]},
            {"role": "assistant", "content": [
                {"type": "text", "text": "calling"},
                {"type": "tool_use", "id": "t2", "name": "Bash", "input": {"cmd": "ls"}},
            ]},
        ],
        "tools": [{"name": "Bash", "description": "run", "input_schema": {"type": "object"}}],
        "tool_choice": {"type": "any"},
    });
    let out = request::claude_to_openai(&body).unwrap();
    // tools present and 500 < 32000 → raised to 32000
    assert_eq!(out["max_tokens"], 32000);
    assert_eq!(out["messages"][0]["role"], "system");
    assert_eq!(out["messages"][0]["content"], "real system");
    // tool_result → tool message
    assert_eq!(out["messages"][1]["role"], "tool");
    assert_eq!(out["messages"][1]["tool_call_id"], "t1");
    // tool_use → assistant tool_calls
    let asst = &out["messages"][2];
    assert_eq!(asst["role"], "assistant");
    assert_eq!(asst["tool_calls"][0]["function"]["name"], "Bash");
    assert!(asst["tool_calls"][0]["function"]["arguments"]
        .as_str()
        .unwrap()
        .contains("ls"));
    // tools → function shape; any → required
    assert_eq!(out["tools"][0]["function"]["name"], "Bash");
    assert_eq!(out["tool_choice"], "required");
}

#[test]
fn claude_to_openai_missing_tool_response_repair() {
    let body = json!({
        "model": "m",
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t9", "name": "X", "input": {}}
            ]},
            {"role": "user", "content": "next"},
        ],
    });
    let out = request::claude_to_openai(&body).unwrap();
    let msgs = out["messages"].as_array().unwrap();
    assert_eq!(msgs[1]["role"], "tool");
    assert_eq!(msgs[1]["content"], "[No response received]");
    assert_eq!(msgs[2]["role"], "user");
}

#[test]
fn claude_stream_to_openai() {
    let mut t = ClaudeToOpenAI::new();
    let events = [
        json!({"type":"message_start","message":{"id":"msg_1","model":"claude-x","usage":{"input_tokens":12,"cache_read_input_tokens":3}}}),
        json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hi"}}),
        json!({"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"tu_1","name":"Bash"}}),
        json!({"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"a\":"}}),
        json!({"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"1}"}}),
        json!({"type":"content_block_stop","index":1}),
        json!({"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":7}}),
        json!({"type":"message_stop"}),
    ];
    let mut out: Vec<serde_json::Value> = Vec::new();
    for e in &events {
        out.extend(t.handle(e));
    }
    // role chunk
    assert_eq!(out[0]["choices"][0]["delta"]["role"], "assistant");
    // text delta
    assert!(out
        .iter()
        .any(|c| c["choices"][0]["delta"]["content"] == "hi"));
    // tool call open
    let open = out
        .iter()
        .find(|c| c["choices"][0]["delta"]["tool_calls"][0]["function"]["name"] == "Bash")
        .unwrap();
    assert_eq!(open["choices"][0]["delta"]["tool_calls"][0]["id"], "tu_1");
    // arg deltas stream with repeated id
    let arg_chunks: Vec<_> = out
        .iter()
        .filter(|c| {
            c["choices"][0]["delta"]["tool_calls"][0]["function"]["arguments"]
                .as_str()
                .map(|s| !s.is_empty())
                .unwrap_or(false)
        })
        .collect();
    assert_eq!(arg_chunks.len(), 2);
    // finish chunk: tool_use → tool_calls, usage folded cache into prompt
    let finish = out
        .iter()
        .find(|c| c["choices"][0]["finish_reason"].is_string())
        .unwrap();
    assert_eq!(finish["choices"][0]["finish_reason"], "tool_calls");
    assert_eq!(finish["usage"]["prompt_tokens"], 15);
    assert_eq!(finish["usage"]["completion_tokens"], 7);
}

#[test]
fn openai_stream_to_claude() {
    let mut t = OpenAIToClaude::new();
    let chunks = [
        json!({"id":"chatcmpl-abc12345","model":"gpt-x","choices":[{"index":0,"delta":{"role":"assistant","content":"he"}}]}),
        json!({"id":"chatcmpl-abc12345","model":"gpt-x","choices":[{"index":0,"delta":{"content":"llo"}}]}),
        json!({"id":"chatcmpl-abc12345","model":"gpt-x","choices":[{"index":0,"delta":{"reasoning_content":"thinking"}}]}),
        json!({"id":"chatcmpl-abc12345","model":"gpt-x","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"Read","arguments":""}}]}}]}),
        json!({"id":"chatcmpl-abc12345","model":"gpt-x","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"limit\":\"50\"}"}}]}}]}),
        json!({"id":"chatcmpl-abc12345","model":"gpt-x","choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":20,"completion_tokens":5}}),
    ];
    let mut out: Vec<serde_json::Value> = Vec::new();
    for c in &chunks {
        out.extend(t.handle(c));
    }
    assert_eq!(out[0]["type"], "message_start");
    assert_eq!(out[0]["message"]["id"], "abc12345");
    // text block start + deltas
    assert!(out
        .iter()
        .any(|e| e["type"] == "content_block_start" && e["content_block"]["type"] == "text"));
    assert!(out.iter().any(|e| e["delta"]["text"] == "llo"));
    // thinking block
    assert!(out.iter().any(|e| e["delta"]["thinking"] == "thinking"));
    // tool_use block opened with id
    assert!(out
        .iter()
        .any(|e| e["content_block"]["type"] == "tool_use" && e["content_block"]["id"] == "call_1"));
    // args emitted as ONE input_json_delta at finish, sanitized (limit string→number)
    let arg_delta = out
        .iter()
        .find(|e| e["delta"]["type"] == "input_json_delta")
        .unwrap();
    assert_eq!(arg_delta["delta"]["partial_json"], "{\"limit\":50}");
    // message_delta stop_reason tool_use + usage
    let md = out.iter().find(|e| e["type"] == "message_delta").unwrap();
    assert_eq!(md["delta"]["stop_reason"], "tool_use");
    assert_eq!(md["usage"]["output_tokens"], 5);
    assert_eq!(out.last().unwrap()["type"], "message_stop");
}

#[test]
fn json_response_roundtrip() {
    let claude = json!({
        "id": "msg_1", "type": "message", "model": "claude-x",
        "content": [{"type":"text","text":"answer"}],
        "stop_reason": "end_turn",
        "usage": {"input_tokens": 10, "output_tokens": 2, "cache_read_input_tokens": 4}
    });
    let openai = response::claude_json_to_openai(&claude, "claude-x").unwrap();
    assert_eq!(openai["choices"][0]["message"]["content"], "answer");
    assert_eq!(openai["choices"][0]["finish_reason"], "stop");
    assert_eq!(openai["usage"]["prompt_tokens"], 14);
    assert_eq!(openai["usage"]["prompt_tokens_details"]["cached_tokens"], 4);

    let back = response::openai_json_to_claude(&openai, "claude-x").unwrap();
    assert_eq!(back["content"][0]["text"], "answer");
    assert_eq!(back["stop_reason"], "end_turn");
    assert_eq!(back["usage"]["input_tokens"], 14);
}

#[test]
fn openai_to_claude_flush_without_finish_reason() {
    let mut t = OpenAIToClaude::new();
    let chunks = [
        json!({"id":"chatcmpl-xyz12345","model":"m","choices":[{"index":0,"delta":{"content":"hi"}}]}),
        json!({"id":"chatcmpl-xyz12345","model":"m","choices":[{"index":0,"delta":{}}],"usage":{"prompt_tokens":8,"completion_tokens":2}}),
    ];
    let mut out: Vec<serde_json::Value> = Vec::new();
    for c in &chunks {
        out.extend(t.handle(c));
    }
    out.extend(t.flush());
    let md = out.iter().find(|e| e["type"] == "message_delta").unwrap();
    assert_eq!(md["delta"]["stop_reason"], "end_turn");
    assert_eq!(md["usage"]["input_tokens"], 8);
    assert_eq!(out.last().unwrap()["type"], "message_stop");
    // flush is idempotent
    assert!(t.flush().is_empty());
}

#[test]
fn claude_to_openai_flush_defaults() {
    let mut t = ClaudeToOpenAI::new();
    t.handle(&json!({"type":"message_start","message":{"id":"msg_f","model":"m","usage":{"input_tokens":5}}}));
    let out = t.flush();
    assert_eq!(out[0]["choices"][0]["finish_reason"], "stop");
    assert_eq!(out[0]["usage"]["prompt_tokens"], 5);
    assert!(t.flush().is_empty());
}
