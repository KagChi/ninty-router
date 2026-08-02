//! Qoder (COSY) provider support: header signing, request body, SSE envelope.
//! Port of shared/qoder/{cosy,constants}.js + executors/qoder.js essentials.

use aes::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
use aes::Aes128;
use base64::Engine;
use rsa::{pkcs8::DecodePublicKey, Pkcs1v15Encrypt, RsaPublicKey};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use ninty_core::error::{Error, Result};

const RSA_PUBLIC_KEY_PEM: &str = "-----BEGIN PUBLIC KEY-----
MIGfMA0GCSqGSIb3DQEBAQUAA4GNADCBiQKBgQDA8iMH5c02LilrsERw9t6Pv5Nc
4k6Pz1EaDicBMpdpxKduSZu5OANqUq8er4GM95omAGIOPOh+Nx0spthYA2BqGz+l
6HRkPJ7S236FZz73In/KVuLnwI8JJ2CbuJap8kvheCCZpmAWpb/cPx/3Vr/J6I17
XcW+ML9FoCI6AOvOzwIDAQAB
-----END PUBLIC KEY-----";

pub const CHAT_URL: &str =
    "https://api3.qoder.sh/algo/api/v2/service/pro/sse/agent_chat_generation?FetchKeys=llm_model_result&AgentId=agent_common&Encode=1";
pub const MODEL_LIST_URL: &str = "https://api3.qoder.sh/algo/api/v2/model/list";
pub const DEVICE_TOKEN_URL: &str = "https://openapi.qoder.sh/api/v1/deviceToken/poll";
pub const USERINFO_URL: &str = "https://openapi.qoder.sh/api/v1/userinfo";
pub const QUOTA_USAGE_URL: &str = "https://openapi.qoder.sh/api/v2/quota/usage";
pub const REFRESH_TOKEN_URL: &str = "https://center.qoder.sh/algo/api/v3/user/refresh_token";
pub const LOGIN_URL: &str = "https://qoder.com/device/selectAccounts";

const IDE_VERSION: &str = "1.0.0";
const CLIENT_TYPE: &str = "5";
const DATA_POLICY: &str = "disagree";
const LOGIN_VERSION: &str = "v2";
const MACHINE_OS: &str = "x86_64_windows";
const MACHINE_TYPE: &str = "5";

#[derive(Debug, Clone)]
pub struct CosyCreds {
    pub user_id: String,
    pub auth_token: String,
    pub name: String,
    pub email: String,
    pub machine_id: String,
}

fn b64(data: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(data)
}

fn md5_hex(data: &[u8]) -> String {
    hex::encode(md5::compute(data).0)
}

type Aes128CbcEnc = cbc::Encryptor<Aes128>;

fn aes_encrypt_cbc_b64(plaintext: &str, key: &str) -> Result<String> {
    let key_bytes = key.as_bytes();
    if key_bytes.len() != 16 {
        return Err(Error::Internal("qoder aes key must be 16 bytes".into()));
    }
    let enc = Aes128CbcEnc::new(key_bytes.into(), key_bytes.into());
    let out = enc.encrypt_padded_vec_mut::<Pkcs7>(plaintext.as_bytes());
    Ok(b64(&out))
}

fn rsa_encrypt_b64(data: &str) -> Result<String> {
    let key = RsaPublicKey::from_public_key_pem(RSA_PUBLIC_KEY_PEM)
        .map_err(|e| Error::Internal(format!("qoder rsa key: {e}")))?;
    let mut rng = rand::thread_rng();
    let out = key
        .encrypt(&mut rng, Pkcs1v15Encrypt, data.as_bytes())
        .map_err(|e| Error::Internal(format!("qoder rsa encrypt: {e}")))?;
    Ok(b64(&out))
}

fn compute_sig_path(url: &str) -> String {
    let path = url::Url::parse(url)
        .map(|u| u.path().to_string())
        .unwrap_or_default();
    path.strip_prefix("/algo").map(String::from).unwrap_or(path)
}

mod url {
    pub use reqwest::Url;
}

/// Build the 19-header COSY set for one request. Body = exact bytes sent.
pub fn build_cosy_headers(
    body: &[u8],
    request_url: &str,
    creds: &CosyCreds,
) -> Result<Vec<(String, String)>> {
    if creds.user_id.is_empty() || creds.auth_token.is_empty() {
        return Err(Error::BadRequest("qoder cosy creds empty".into()));
    }
    let aes_key = Uuid::new_v4().to_string()[..16].to_string();
    let user_info = json!({
        "uid": creds.user_id,
        "security_oauth_token": creds.auth_token,
        "name": creds.name,
        "aid": "",
        "email": creds.email,
    });
    let info = aes_encrypt_cbc_b64(&user_info.to_string(), &aes_key)?;
    let cosy_key = rsa_encrypt_b64(&aes_key)?;

    let timestamp = chrono::Utc::now().timestamp().to_string();
    let payload_json = json!({
        "version": "v1",
        "requestId": Uuid::new_v4().to_string(),
        "info": info,
        "cosyVersion": IDE_VERSION,
        "ideVersion": "",
    });
    let payload_b64 = b64(payload_json.to_string().as_bytes());

    let sig_path = compute_sig_path(request_url);
    let sig_input = format!(
        "{payload_b64}\n{cosy_key}\n{timestamp}\n{}\n{sig_path}",
        String::from_utf8_lossy(body)
    );
    let sig = md5_hex(sig_input.as_bytes());
    let body_hash = md5_hex(body);

    Ok(vec![
        (
            "Authorization".into(),
            format!("Bearer COSY.{payload_b64}.{sig}"),
        ),
        ("Cosy-Key".into(), cosy_key),
        ("Cosy-User".into(), creds.user_id.clone()),
        ("Cosy-Date".into(), timestamp),
        ("Cosy-Version".into(), IDE_VERSION.into()),
        ("Cosy-Machineid".into(), creds.machine_id.clone()),
        ("Cosy-Machinetoken".into(), creds.machine_id.clone()),
        ("Cosy-Machinetype".into(), MACHINE_TYPE.into()),
        ("Cosy-Machineos".into(), MACHINE_OS.into()),
        ("Cosy-Clienttype".into(), CLIENT_TYPE.into()),
        ("Cosy-Clientip".into(), "127.0.0.1".into()),
        ("Cosy-Bodyhash".into(), body_hash),
        ("Cosy-Bodylength".into(), body.len().to_string()),
        ("Cosy-Sigpath".into(), sig_path),
        ("Cosy-Data-Policy".into(), DATA_POLICY.into()),
        ("Cosy-Organization-Id".into(), String::new()),
        ("Cosy-Organization-Tags".into(), String::new()),
        ("Login-Version".into(), LOGIN_VERSION.into()),
        ("X-Request-Id".into(), Uuid::new_v4().to_string()),
    ])
}

pub fn stable_hash(prefix: &str, parts: &[&str]) -> String {
    let mut h = Sha256::new();
    h.update(prefix.as_bytes());
    for p in parts {
        h.update(b"\0");
        h.update(p.as_bytes());
    }
    hex::encode(h.finalize())[..16].to_string()
}

fn stable_chat_record_id(
    model: &str,
    messages: &[Value],
    tools: Option<&Value>,
    max_tokens: i64,
) -> String {
    let mut h = Sha256::new();
    h.update(b"qoder-record\0");
    h.update(model.as_bytes());
    for m in messages {
        if let Some(r) = m.get("role").and_then(Value::as_str) {
            h.update(b"\0");
            h.update(r.as_bytes());
        }
        if let Some(c) = m.get("content").and_then(Value::as_str) {
            if !c.is_empty() {
                h.update(b"\0");
                h.update(c.as_bytes());
            }
        }
    }
    if let Some(t) = tools {
        h.update(b"\0");
        h.update(t.to_string().as_bytes());
    }
    h.update(format!("\0mt={max_tokens}").as_bytes());
    hex::encode(h.finalize())[..16].to_string()
}

fn extract_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|p| p.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Map an openai chat body → the exact Qoder shape. model_config from live catalog.
pub fn build_chat_body(
    model_key: &str,
    body: &Value,
    model_config: &Value,
    user_id: &str,
) -> Value {
    let mut messages: Vec<Value> = vec![];
    let mut system_parts: Vec<String> = vec![];
    for m in body
        .get("messages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        let role = m.get("role").and_then(Value::as_str).unwrap_or("user");
        let text = extract_text(m.get("content").unwrap_or(&Value::Null));
        if role == "system" {
            if !text.is_empty() {
                system_parts.push(text);
            }
            continue;
        }
        let mut cloned = m.clone();
        cloned["content"] = Value::String(text);
        messages.push(cloned);
    }
    let system_text = system_parts.join("\n\n");
    let tools = body.get("tools").cloned().unwrap_or(Value::Null);

    let max_output = model_config
        .get("max_output_tokens")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let mut max_tokens = if max_output > 0 { max_output } else { 32_768 };
    for k in ["max_tokens", "max_completion_tokens"] {
        if let Some(v) = body.get(k).and_then(Value::as_i64) {
            if v > 0 && v < max_tokens {
                max_tokens = v;
            }
        }
    }

    let last_user = messages
        .iter()
        .rev()
        .find(|m| m.get("role").and_then(Value::as_str) == Some("user"))
        .and_then(|m| m.get("content").and_then(Value::as_str))
        .unwrap_or("")
        .to_string();

    let session_id = stable_hash("qoder-session", &[user_id, model_key]);
    let record_id = stable_chat_record_id(model_key, &messages, body.get("tools"), max_tokens);
    let name = if last_user.chars().count() > 30 {
        format!("{}...", last_user.chars().take(30).collect::<String>())
    } else {
        last_user.clone()
    };

    json!({
        "request_id": Uuid::new_v4().to_string(),
        "request_set_id": record_id,
        "chat_record_id": record_id,
        "session_id": session_id,
        "stream": true,
        "chat_task": "FREE_INPUT",
        "is_reply": true,
        "is_retry": false,
        "source": 1,
        "version": "3",
        "session_type": "qodercli",
        "agent_id": "agent_common",
        "task_id": "common",
        "code_language": "",
        "chat_prompt": "",
        "image_urls": Value::Null,
        "aliyun_user_type": "",
        "system": system_text,
        "messages": messages,
        "tools": if tools.is_array() { tools.clone() } else { json!([]) },
        "parameters": {"max_tokens": max_tokens},
        "chat_context": {
            "chatPrompt": "",
            "imageUrls": Value::Null,
            "extra": {
                "context": [],
                "modelConfig": {"key": model_key, "is_reasoning": model_config.get("is_reasoning").and_then(Value::as_bool).unwrap_or(false)},
                "originalContent": last_user,
            },
            "features": [],
            "text": last_user,
        },
        "model_config": model_config,
        "business": {
            "product": "cli",
            "version": "1.0.0",
            "type": "agent",
            "stage": "start",
            "id": Uuid::new_v4().to_string(),
            "name": name,
            "begin_at": chrono::Utc::now().timestamp_millis(),
        },
    })
}

/// Fetch live model catalog (COSY-signed GET) → raw config for `model_key`.
pub async fn fetch_model_config(
    client: &reqwest::Client,
    creds: &CosyCreds,
    model_key: &str,
) -> Result<Value> {
    let headers = build_cosy_headers(&[], MODEL_LIST_URL, creds)?;
    let mut req = client
        .get(MODEL_LIST_URL)
        .header("accept", "application/json")
        .header("accept-encoding", "identity")
        .timeout(std::time::Duration::from_secs(30));
    for (k, v) in headers {
        req = req.header(k, v);
    }
    let resp = req.send().await.map_err(|e| Error::Upstream {
        status: 502,
        message: e.to_string(),
    })?;
    if !resp.status().is_success() {
        return Err(Error::Upstream {
            status: resp.status().as_u16(),
            message: "qoder model list failed".into(),
        });
    }
    let v: Value = resp
        .json()
        .await
        .map_err(|e| Error::Internal(e.to_string()))?;
    let key = model_key.to_string();
    v.get("chat")
        .and_then(Value::as_array)
        .and_then(|arr| {
            arr.iter()
                .find(|e| e.get("key").and_then(Value::as_str) == Some(&key))
        })
        .cloned()
        .ok_or_else(|| {
            Error::BadRequest(format!(
                "qoder: model_config for '{model_key}' not in catalog"
            ))
        })
}

/// Unwrap one upstream SSE `data:` payload → inner openai chunk JSON string,
/// or "[DONE]". Envelope: {"statusCodeValue":200,"body":"<inner json>"}.
pub fn unwrap_envelope(data: &str, model: &str) -> Option<String> {
    if data == "[DONE]" {
        return Some("[DONE]".into());
    }
    let env: Value = serde_json::from_str(data).ok()?;
    let status = env
        .get("statusCodeValue")
        .and_then(Value::as_i64)
        .unwrap_or(200);
    let inner = env.get("body").and_then(Value::as_str).unwrap_or("");
    if status != 200 {
        let msg: String = inner.chars().take(200).collect();
        return Some(
            json!({
                "id": format!("qoder-error-{}", chrono::Utc::now().timestamp()),
                "object": "chat.completion.chunk",
                "created": chrono::Utc::now().timestamp(),
                "model": model,
                "choices": [{"index": 0, "delta": {"content": format!("\n[qoder error {status}: {msg}]")}, "finish_reason": "stop"}],
            })
            .to_string(),
        );
    }
    if inner.is_empty() {
        return None;
    }
    if inner == "[DONE]" {
        return Some("[DONE]".into());
    }
    Some(inner.replace(['\r', '\n'], ""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_hash_deterministic() {
        let a = stable_hash("qoder-session", &["u1", "qmodel"]);
        assert_eq!(a, stable_hash("qoder-session", &["u1", "qmodel"]));
        assert_ne!(a, stable_hash("qoder-session", &["u2", "qmodel"]));
        assert_eq!(a.len(), 16);
    }

    #[test]
    fn envelope_unwrap() {
        let inner = json!({"choices":[{"delta":{"content":"hi"}}]}).to_string();
        let env = json!({"statusCodeValue":200,"body":inner}).to_string();
        let out = unwrap_envelope(&env, "qmodel").unwrap();
        assert!(out.contains("hi"));
        assert_eq!(unwrap_envelope("[DONE]", "m").unwrap(), "[DONE]");
        let err = json!({"statusCodeValue":429,"body":"slow down"}).to_string();
        assert!(unwrap_envelope(&err, "m")
            .unwrap()
            .contains("qoder error 429"));
    }

    #[test]
    fn body_builder() {
        let body = json!({"messages":[
            {"role":"system","content":"sys"},
            {"role":"user","content":"hello"}
        ]});
        let cfg = json!({"key":"qmodel","max_output_tokens":4096,"is_reasoning":false});
        let out = build_chat_body("qmodel", &body, &cfg, "user-1");
        assert_eq!(out["system"], "sys");
        assert_eq!(out["parameters"]["max_tokens"], 4096);
        assert_eq!(out["messages"].as_array().unwrap().len(), 1);
        assert_eq!(out["chat_context"]["text"], "hello");
        assert_eq!(
            out["session_id"],
            stable_hash("qoder-session", &["user-1", "qmodel"])
        );
    }

    #[test]
    fn cosy_headers_golden_shape() {
        let creds = CosyCreds {
            user_id: "u".into(),
            auth_token: "dt-x".into(),
            name: String::new(),
            email: String::new(),
            machine_id: "machine-1".into(),
        };
        let headers = build_cosy_headers(b"{}", CHAT_URL, &creds).unwrap();
        let get = |k: &str| {
            headers
                .iter()
                .find(|(h, _)| h == k)
                .map(|(_, v)| v.clone())
                .unwrap()
        };
        assert!(get("Authorization").starts_with("Bearer COSY."));
        assert_eq!(get("Cosy-User"), "u");
        assert_eq!(get("Cosy-Machineid"), "machine-1");
        assert_eq!(
            get("Cosy-Sigpath"),
            "/api/v2/service/pro/sse/agent_chat_generation"
        );
        assert_eq!(get("Cosy-Bodylength"), "2");
        assert_eq!(headers.len(), 19);
    }
}
