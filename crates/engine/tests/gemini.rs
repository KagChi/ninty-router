use engine::translator::gemini::{
    clean_schema, gemini_json_to_openai, gemini_to_openai, openai_to_gemini, GeminiToOpenAI,
    OpenAIToGemini,
};
use serde_json::json;

#[test]
fn openai_to_gemini_request() {
    let body = json!({
        "model": "gemini-x",
        "temperature": 0.5,
        "messages": [
            {"role": "system", "content": "be terse"},
            {"role": "user", "content": "hi"},
            {"role": "assistant", "content": "hello", "tool_calls": [
                {"id": "c1", "type": "function", "function": {"name": "Bash Tool", "arguments": "{\"cmd\":\"ls\"}"}}
            ]},
            {"role": "tool", "tool_call_id": "c1", "content": "{\"files\":[\"a\"]}"},
        ],
        "tools": [{"type": "function", "function": {"name": "Bash Tool", "description": "run", "parameters": {"type": "object", "properties": {"cmd": {"type": "string", "minLength": 1, "format": "x"}}, "required": ["cmd", "nope"]}}}],
    });
    let out = openai_to_gemini(&body).unwrap();
    assert_eq!(out["systemInstruction"]["parts"][0]["text"], "be terse");
    assert_eq!(out["generationConfig"]["temperature"], 0.5);
    assert_eq!(out["generationConfig"]["maxOutputTokens"], 64000); // no max set → default ceiling
    let contents = out["contents"].as_array().unwrap();
    assert_eq!(contents[0]["role"], "user");
    assert_eq!(contents[1]["role"], "model");
    // functionCall name sanitized (space → _)
    let fc = &contents[1]["parts"][1]["functionCall"];
    assert_eq!(fc["name"], "Bash_Tool");
    // functionResponse inlined as user turn after model turn
    assert_eq!(contents[2]["role"], "user");
    assert_eq!(
        contents[2]["parts"][0]["functionResponse"]["name"],
        "Bash_Tool"
    );
    assert_eq!(
        contents[2]["parts"][0]["functionResponse"]["response"]["files"][0],
        "a"
    );
    // tools → functionDeclarations, schema cleaned
    let decl = &out["tools"][0]["functionDeclarations"][0];
    assert_eq!(decl["name"], "Bash_Tool");
    assert!(decl["parameters"]["properties"]["cmd"]
        .get("minLength")
        .is_none());
    assert!(decl["parameters"]["properties"]["cmd"]
        .get("format")
        .is_none());
    // required filtered to existing properties
    assert_eq!(decl["parameters"]["required"], json!(["cmd"]));
}

#[test]
fn gemini_to_openai_request() {
    let body = json!({
        "model": "m",
        "systemInstruction": {"parts": [{"text": "sys"}]},
        "contents": [
            {"role": "user", "parts": [{"text": "hi"}]},
            {"role": "model", "parts": [
                {"text": "calling"},
                {"functionCall": {"name": "Bash", "args": {"cmd": "ls"}}}
            ]},
            {"role": "user", "parts": [{"functionResponse": {"id": "call_Bash", "name": "Bash", "response": {"result": {"ok": true}}}}]},
        ],
        "generationConfig": {"maxOutputTokens": 1000, "temperature": 0.2},
        "tools": [{"functionDeclarations": [{"name": "Bash", "description": "run", "parameters": {"type": "object"}}]}],
    });
    let out = gemini_to_openai(&body).unwrap();
    assert_eq!(out["messages"][0]["role"], "system");
    assert_eq!(out["messages"][2]["role"], "assistant");
    assert_eq!(
        out["messages"][2]["tool_calls"][0]["function"]["name"],
        "Bash"
    );
    assert_eq!(out["messages"][3]["role"], "tool");
    assert_eq!(out["messages"][3]["tool_call_id"], "call_Bash");
    assert_eq!(out["temperature"], 0.2);
    assert_eq!(out["max_tokens"], 32000); // tools floor raises 1000
    assert_eq!(out["tools"][0]["function"]["name"], "Bash");
}

#[test]
fn gemini_stream_to_openai() {
    let mut t = GeminiToOpenAI::new();
    let chunks = [
        json!({"candidates":[{"content":{"role":"model","parts":[{"text":"he"}]},"index":0}],"responseId":"r1","modelVersion":"gemini-3"}),
        json!({"candidates":[{"content":{"role":"model","parts":[{"text":"y"},{"thought":true,"text":"hmm"}]},"index":0}]}),
        json!({"candidates":[{"content":{"role":"model","parts":[{"functionCall":{"name":"Bash","args":{"a":1}}}]},"index":0,"finishReason":"STOP"}],
               "usageMetadata":{"promptTokenCount":10,"candidatesTokenCount":4,"thoughtsTokenCount":2,"totalTokenCount":16,"cachedContentTokenCount":3}}),
    ];
    let mut out: Vec<serde_json::Value> = Vec::new();
    for c in &chunks {
        out.extend(t.handle(c));
    }
    assert_eq!(out[0]["choices"][0]["delta"]["role"], "assistant");
    assert!(out
        .iter()
        .any(|c| c["choices"][0]["delta"]["content"] == "y"));
    assert!(out
        .iter()
        .any(|c| c["choices"][0]["delta"]["reasoning_content"] == "hmm"));
    let tc = out
        .iter()
        .find(|c| c["choices"][0]["delta"]["tool_calls"][0]["function"]["name"] == "Bash")
        .unwrap();
    assert!(
        tc["choices"][0]["delta"]["tool_calls"][0]["function"]["arguments"]
            .as_str()
            .unwrap()
            .contains("\"a\":1")
    );
    let finish = out.last().unwrap();
    assert_eq!(finish["choices"][0]["finish_reason"], "tool_calls"); // stop + tool calls → tool_calls
    assert_eq!(finish["usage"]["prompt_tokens"], 10);
    assert_eq!(finish["usage"]["completion_tokens"], 6); // candidates + thoughts
    assert_eq!(finish["usage"]["prompt_tokens_details"]["cached_tokens"], 3);
    assert_eq!(
        finish["usage"]["completion_tokens_details"]["reasoning_tokens"],
        2
    );
}

#[test]
fn openai_stream_to_gemini() {
    let mut t = OpenAIToGemini::new();
    let chunks = [
        json!({"id":"chatcmpl-g1234567","model":"gpt-x","choices":[{"index":0,"delta":{"content":"he"}}]}),
        json!({"id":"chatcmpl-g1234567","model":"gpt-x","choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":5,"completion_tokens":1}}),
    ];
    let mut out: Vec<serde_json::Value> = Vec::new();
    for c in &chunks {
        out.extend(t.handle(c));
    }
    assert_eq!(out[0]["candidates"][0]["content"]["parts"][0]["text"], "he");
    assert_eq!(out[1]["candidates"][0]["finishReason"], "STOP");
    assert_eq!(out[1]["usageMetadata"]["promptTokenCount"], 5);
}

#[test]
fn gemini_json_response() {
    let g = json!({
        "candidates": [{"content": {"role": "model", "parts": [{"text": "answer"}, {"thought": true, "text": "why"}]}, "finishReason": "STOP", "index": 0}],
        "usageMetadata": {"promptTokenCount": 10, "candidatesTokenCount": 4, "thoughtsTokenCount": 2, "totalTokenCount": 16},
        "modelVersion": "gemini-3", "responseId": "r9"
    });
    let o = gemini_json_to_openai(&g, "gemini-3").unwrap();
    assert_eq!(o["choices"][0]["message"]["content"], "answer");
    assert_eq!(o["choices"][0]["message"]["reasoning_content"], "why");
    assert_eq!(o["usage"]["prompt_tokens"], 12); // thoughts folded into prompt (non-stream)
    assert_eq!(o["usage"]["completion_tokens"], 4);

    let back = engine::translator::response::openai_json_to_gemini(&o, "gemini-3").unwrap();
    assert_eq!(
        back["candidates"][0]["content"]["parts"][0]["text"],
        "answer"
    );
    assert_eq!(back["candidates"][0]["finishReason"], "STOP");
    assert_eq!(back["usageMetadata"]["promptTokenCount"], 12);
}

#[test]
fn schema_cleaning() {
    let s = json!({
        "type": "object",
        "title": "X",
        "properties": {
            "a": {"type": ["string", "null"], "format": "date-time", "default": "x"},
            "b": {"properties": {"c": {"type": "integer"}}},
        },
        "required": ["a", "missing"],
        "x-custom": 1,
    });
    let c = clean_schema(&s);
    assert!(c.get("title").is_none());
    assert!(c.get("x-custom").is_none());
    assert_eq!(c["properties"]["a"]["type"], "string");
    assert!(c["properties"]["a"].get("format").is_none());
    assert_eq!(c["properties"]["b"]["type"], "object"); // ensure-object-type
    assert_eq!(c["required"], json!(["a"]));
}
