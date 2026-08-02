//! Chat core shared by /v1/chat/completions, /v1/messages, /v1beta/models.
//! Account fallback loop + combo model loop + two-stage stream translation.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::Response;
use axum::Json;
use bytes::Bytes;
use engine::executor::retry_config_for;
use engine::fallback::{self, Verdict};
use engine::sse::SseParser;
use engine::translator::{self, Format};
use ninty_core::config;
use ninty_core::error::Error;
use ninty_core::registry::{self, AuthStyle, UrlStyle, WireFormat};
use serde_json::{json, Value};

use crate::api::ApiError;
use crate::repos::{api_keys, combos, connections, nodes, settings, usage};
use crate::state::AppState;

const STALL_MS: u64 = 360_000;
const FIRST_BYTE_MS: u64 = 200_000;

// ---------------------------------------------------------------------------
// handlers
// ---------------------------------------------------------------------------

pub async fn chat_completions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Response, ApiError> {
    run(
        State(state),
        headers,
        Json(body),
        WireFormat::Openai,
        "/v1/chat/completions",
    )
    .await
}

pub async fn messages(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Response, ApiError> {
    run(
        State(state),
        headers,
        Json(body),
        WireFormat::Claude,
        "/v1/messages",
    )
    .await
}

fn to_format(f: WireFormat) -> Format {
    match f {
        WireFormat::Openai => Format::Openai,
        WireFormat::Claude => Format::Claude,
        WireFormat::Gemini => Format::Gemini,
        WireFormat::Responses => Format::Responses,
    }
}

// ---------------------------------------------------------------------------
// top-level: auth guard → combo dispatch → model loop
// ---------------------------------------------------------------------------

pub(crate) async fn run(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
    client_format: WireFormat,
    endpoint: &'static str,
) -> Result<Response, ApiError> {
    let stream = body.get("stream").and_then(Value::as_bool).unwrap_or(false);
    let model = body
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if model.is_empty() {
        return Err(Error::BadRequest("missing model".into()).into());
    }

    // API key enforcement
    let app_settings = settings::get(&state.db).await?;
    let mut key_str: Option<String> = None;
    if app_settings.require_api_key {
        let bearer = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(str::to_string)
            .ok_or(Error::Unauthorized)?;
        let key = api_keys::get_by_key(&state.db, &bearer)
            .await?
            .ok_or(Error::Unauthorized)?;
        if !key.is_active {
            return Err(Error::Unauthorized.into());
        }
        if !key.allowed_models.is_empty() && !key.allowed_models.iter().any(|m| m == &model) {
            return Err(
                Error::BadRequest(format!("model '{model}' not allowed for this key")).into(),
            );
        }
        if let Some(limit) = key.token_limit {
            let used =
                usage::key_usage_since(&state.db, &key.key, key.limit_reset_at.clone()).await?;
            if used >= limit {
                return Err(ApiError(Error::Upstream {
                    status: 429,
                    message: "token limit reached".into(),
                }));
            }
        }
        if let Some(rpm) = key.rpm_limit {
            if usage::rpm_count(&state.db, &key.key).await? >= rpm {
                record(
                    &state,
                    "",
                    &model,
                    None,
                    Some(&key.key),
                    endpoint,
                    0,
                    0,
                    "error",
                    Some("rate_limit"),
                    None,
                )
                .await;
                return Err(ApiError(Error::Upstream {
                    status: 429,
                    message: "rate limit exceeded".into(),
                }));
            }
        }
        key_str = Some(key.key);
    }

    // combo dispatch
    let combo = combos::get_by_name(&state.db, &model).await?;
    let combo_mode = combo.is_some();
    let mut specs: Vec<String> = match combo {
        Some(c) if !c.models.is_empty() => reorder_by_capability(c.models, &body),
        Some(_) => return Err(Error::BadRequest("combo has no models".into()).into()),
        None => vec![model.clone()],
    };

    let mut last_error: Option<(u16, String)> = None;
    while !specs.is_empty() {
        let spec = specs.remove(0);
        match run_single(
            &state,
            &body,
            &spec,
            client_format,
            endpoint,
            stream,
            &key_str,
            &headers,
        )
        .await
        {
            Ok(resp) => return Ok(resp),
            Err((status, text, verdict)) => {
                if verdict == Verdict::NoFallback || !combo_mode {
                    return Err(ApiError(Error::Upstream {
                        status,
                        message: text,
                    }));
                }
                last_error = Some((status, text));
            }
        }
    }
    let (status, message) = last_error.unwrap_or((503, "no accounts available".into()));
    Err(ApiError(Error::Upstream { status, message }))
}

// ---------------------------------------------------------------------------
// single model: account fallback loop
// ---------------------------------------------------------------------------

type AttemptErr = (u16, String, Verdict);

#[allow(clippy::too_many_arguments)]
async fn run_single(
    state: &Arc<AppState>,
    body: &Value,
    spec: &str,
    client_format: WireFormat,
    endpoint: &'static str,
    stream: bool,
    key_str: &Option<String>,
    client_headers: &HeaderMap,
) -> Result<Response, AttemptErr> {
    let targets = resolve_targets(state, spec, client_format)
        .await
        .map_err(|e| (400u16, e.to_string(), Verdict::NoFallback))?;

    let mut last: Option<(u16, String)> = None;
    for target in targets {
        // qoder: fully custom executor (COSY sign + model_config + envelope SSE)
        if target.provider_id == "qoder" {
            match run_qoder(
                state,
                &target,
                body,
                client_format,
                endpoint,
                stream,
                key_str,
            )
            .await
            {
                Ok(resp) => return Ok(resp),
                Err(e) => return Err(e),
            }
        }

        // request body translation
        let mut body_out = body.clone();
        body_out["model"] = Value::String(target.model.clone());
        let up_fmt = to_format(target.format);
        if to_format(client_format) != up_fmt {
            body_out = translator::translate_request(to_format(client_format), up_fmt, &body_out)
                .map_err(|e| (500, e.to_string(), Verdict::NoFallback))?;
        }
        if target.format == WireFormat::Claude {
            inject_claude_identity(&mut body_out);
        }

        // force_stream upstreams (codex): always stream upstream
        if target.force_stream {
            body_out["stream"] = Value::Bool(true);
        }

        // token savers: post-translation, pre-executor
        let saver_off = client_headers
            .get("x-9router-token-saver")
            .and_then(|v| v.to_str().ok())
            .map(|v| v.eq_ignore_ascii_case("off"))
            .unwrap_or(false);
        let mut rtk_saved: i64 = 0;
        let mut pxpipe_summary: Option<engine::pxpipe::PxpipeSummary> = None;
        if !saver_off {
            let s = settings::get(&state.db).await.unwrap_or_default();
            if s.rtk_enabled {
                if let Some(stats) = engine::rtk::compress_messages(&mut body_out) {
                    rtk_saved = stats.saved();
                }
            }
            let up_fmt = to_format(target.format);
            if s.caveman_enabled {
                if let Some(p) = engine::savers::caveman_prompt("full") {
                    engine::savers::inject_system_prompt(&mut body_out, up_fmt, &p);
                }
            }
            if s.ponytail_enabled {
                if let Some(p) = engine::savers::ponytail_prompt(&s.ponytail_level) {
                    engine::savers::inject_system_prompt(&mut body_out, up_fmt, &p);
                }
            }
            // PXPIPE: image bulky context (Claude-format bodies only), last saver
            // before dispatch. Fail-open: errors/timeouts leave body untouched.
            if s.pxpipe_enabled && up_fmt == Format::Claude {
                let opts = engine::pxpipe::PxpipeOpts {
                    enabled: true,
                    min_chars: s.pxpipe_min_chars,
                    timeout_ms: s.pxpipe_timeout_ms,
                    model: target.model.clone(),
                };
                let (new_body, summary) =
                    engine::pxpipe::compress_with_pxpipe(&body_out, &opts, &config::data_dir())
                        .await;
                if let Some(b) = new_body {
                    body_out = b;
                }
                if let Some(line) = engine::pxpipe::format_pxpipe_log(&summary) {
                    tracing::info!("PXPIPE {line}");
                }
                if summary.applied {
                    pxpipe_summary = Some(summary);
                }
            }
        }

        let detail_ctx = state.enable_request_logs().then(|| DetailCtx {
            request: truncate_body(body),
            provider_request: truncate_body(&body_out),
            started: std::time::Instant::now(),
            pxpipe: pxpipe_summary
                .as_ref()
                .and_then(|p| serde_json::to_value(p).ok()),
        });

        let (url, headers) = match build_url_and_auth(state, &target, stream).await {
            Ok(v) => v,
            Err(e) => return Err((500, e.to_string(), Verdict::NoFallback)),
        };

        let send = send_request(&state.http, &url, &headers, &body_out, target.timeout_ms).await;
        let resp = match send {
            Ok(r) => r,
            Err((status, text)) => {
                let verdict = judge(&text, status, target.backoff_level);
                match verdict {
                    Verdict::NoFallback => return Err((status, text, verdict)),
                    _ => {
                        mark(state, &target, &text, &verdict).await;
                        record(
                            state,
                            &target.provider_id,
                            &target.model,
                            target.conn_id.as_deref(),
                            key_str.as_deref(),
                            endpoint,
                            0,
                            0,
                            "error",
                            None,
                            detail_ctx.as_ref(),
                        )
                        .await;
                        last = Some((status, text));
                        continue;
                    }
                }
            }
        };

        let mut resp = resp;
        if matches!(resp.status().as_u16(), 401 | 403) && target.oauth_refresh {
            // reactive refresh: force once, retry once
            if let Some(conn_id) = &target.conn_id {
                if let Ok(Some(conn)) = connections::get(&state.db, conn_id).await {
                    if let Ok(fresh) = crate::oauth_state::refresh_now(state, &conn).await {
                        let cred = fresh
                            .data
                            .get("accessToken")
                            .or_else(|| fresh.data.get("copilotToken"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        if !cred.is_empty() {
                            let mut t2 = target.clone();
                            t2.credential = cred;
                            if let Ok((url2, headers2)) =
                                build_url_and_auth(state, &t2, stream).await
                            {
                                if let Ok(r2) = send_request(
                                    &state.http,
                                    &url2,
                                    &headers2,
                                    &body_out,
                                    t2.timeout_ms,
                                )
                                .await
                                {
                                    resp = r2;
                                }
                            }
                        }
                    }
                }
            }
        }

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let text = resp.text().await.unwrap_or_default();
            let verdict = judge(&text, status, target.backoff_level);
            match verdict {
                Verdict::NoFallback => return Err((status, text, verdict)),
                _ => {
                    mark(state, &target, &text, &verdict).await;
                    record(
                        state,
                        &target.provider_id,
                        &target.model,
                        target.conn_id.as_deref(),
                        key_str.as_deref(),
                        endpoint,
                        0,
                        0,
                        "error",
                        None,
                        detail_ctx.as_ref(),
                    )
                    .await;
                    last = Some((status, text));
                    continue;
                }
            }
        }

        // success
        if let Some(conn_id) = &target.conn_id {
            let sticky = sticky_limit(state, &target.provider_id).await;
            let _ = connections::clear_error(&state.db, conn_id, &target.model, sticky).await;
        }
        return Ok(finish(
            state,
            target,
            body,
            client_format,
            endpoint,
            stream,
            key_str,
            rtk_saved,
            resp,
            detail_ctx,
        )
        .await);
    }

    let (status, text) = last.unwrap_or((503, format!("no accounts available for '{spec}'")));
    Err((status, text, Verdict::Fallback { cooldown_ms: 0 }))
}

/// Request-log context: bodies captured post-savers (final upstream body),
/// truncated to 64KB each, secrets never included (bodies only, no auth headers).
struct DetailCtx {
    request: Value,
    provider_request: Value,
    started: std::time::Instant,
    pxpipe: Option<Value>,
}

const DETAIL_BODY_CAP: usize = 64 * 1024;

/// Truncate a JSON value's serialized form to cap bytes; returns {truncated, head}
/// marker when over. Bodies only ever carry user content — auth headers are never
/// part of this path (secrets redacted by construction).
fn truncate_body(v: &Value) -> Value {
    let s = v.to_string();
    if s.len() <= DETAIL_BODY_CAP {
        return v.clone();
    }
    json!({"truncated": true, "chars": s.len(), "head": &s[..DETAIL_BODY_CAP]})
}

fn judge(text: &str, status: u16, backoff_level: u32) -> Verdict {
    let parsed: Value = serde_json::from_str(text).unwrap_or(Value::Null);
    let v = fallback::classify(status, text, backoff_level);
    fallback::with_resets_at(v, fallback::extract_resets_at(&parsed))
}

async fn mark(state: &Arc<AppState>, target: &Target, text: &str, verdict: &Verdict) {
    let Some(conn_id) = &target.conn_id else {
        return;
    };
    let (ms, deactivate) = match verdict {
        Verdict::Fallback { cooldown_ms } => (*cooldown_ms, false),
        Verdict::Deactivate => (0, true),
        Verdict::NoFallback => return,
    };
    let _ = connections::mark_unavailable(&state.db, conn_id, &target.model, ms, text, deactivate)
        .await;
}

// ---------------------------------------------------------------------------
// response handling
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn finish(
    state: &Arc<AppState>,
    target: Target,
    body: &Value,
    client_format: WireFormat,
    endpoint: &'static str,
    stream: bool,
    key_str: &Option<String>,
    rtk_saved: i64,
    resp: reqwest::Response,
    detail_ctx: Option<DetailCtx>,
) -> Response {
    let request_model = body
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or(&target.model)
        .to_string();

    if !stream && target.force_stream {
        // upstream streams, client wants a single JSON: collect + accumulate
        return collect_forced_stream(
            state,
            target,
            client_format,
            endpoint,
            key_str,
            rtk_saved,
            &request_model,
            resp,
            detail_ctx,
        )
        .await;
    }

    if !stream {
        let text = match resp.text().await {
            Ok(t) => t,
            Err(e) => return ApiError(Error::Internal(e.to_string())).into_response(),
        };
        let parsed: Value = serde_json::from_str(&text)
            .unwrap_or_else(|_| json!({"error": {"message": "invalid json from upstream"}}));
        let up_fmt = to_format(target.format);
        let out = if target.format == client_format {
            parsed
        } else {
            translator::translate_response_json(
                up_fmt,
                to_format(client_format),
                &parsed,
                &request_model,
            )
            .unwrap_or(parsed)
        };
        let (prompt, completion) = usage_of(client_format, &out);
        record_meta(
            state,
            &target.provider_id,
            &request_model,
            target.conn_id.as_deref(),
            key_str.as_deref(),
            endpoint,
            prompt,
            completion,
            "success",
            None,
            rtk_saved,
            detail_ctx.as_ref(),
        )
        .await;
        return Json(out).into_response();
    }

    // streaming: stage1 upstream→openai pivot, stage2 pivot→client (skipped when equal)
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(64);
    let bytes_stream = resp.bytes_stream();
    let up_fmt = to_format(target.format);
    let cli_fmt = to_format(client_format);
    let model_for_stream = request_model.clone();
    tokio::spawn(async move {
        stream_pipeline(bytes_stream, up_fmt, cli_fmt, &model_for_stream, tx).await;
    });

    // usage recorded when stream ends: byte estimate; translator usage handled inside pipeline events
    let db = state.db.clone();
    let (provider, conn_id, model, key, ep) = (
        target.provider_id.clone(),
        target.conn_id.clone(),
        request_model.clone(),
        key_str.clone(),
        endpoint,
    );
    let request_logs = state.enable_request_logs();
    let mut detail_ctx = detail_ctx;
    let counted = CountingStream::new(rx, move |total_bytes| {
        let estimate = total_bytes / 4;
        let db = db.clone();
        let (provider, conn_id, model, key) = (
            provider.clone(),
            conn_id.clone(),
            model.clone(),
            key.clone(),
        );
        let ctx = detail_ctx.take();
        tokio::spawn(async move {
            let meta = (rtk_saved > 0).then(|| json!({"rtk_saved_bytes": rtk_saved}));
            let _ = usage::record(
                &db,
                usage::UsageRecord {
                    provider: provider.clone(),
                    model: model.clone(),
                    connection_id: conn_id.clone(),
                    api_key: key,
                    endpoint: ep.into(),
                    prompt_tokens: 0,
                    completion_tokens: estimate as i64,
                    cost: 0.0,
                    status: "success".into(),
                    meta,
                },
            )
            .await;
            if request_logs {
                let extra = ctx.map(|c| {
                    let mut m = json!({
                        "request": c.request,
                        "providerRequest": c.provider_request,
                        "latencyMs": c.started.elapsed().as_millis() as u64,
                    });
                    if let Some(p) = &c.pxpipe {
                        m["pxpipe"] = p.clone();
                    }
                    m
                });
                let _ = usage::insert_request_detail(
                    &db,
                    usage::RequestDetail {
                        provider,
                        model,
                        status: "success".into(),
                        input_tokens: 0,
                        output_tokens: estimate as i64,
                        endpoint: ep.into(),
                        extra,
                    },
                )
                .await;
            }
        });
    });

    Response::builder()
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-cache")
        .header("connection", "keep-alive")
        .body(Body::from_stream(counted))
        .unwrap()
}

/// SSE pipeline: upstream bytes → stage1 (→openai) → stage2 (→client) → client events.
async fn stream_pipeline<S, E>(
    mut bytes_stream: S,
    up: Format,
    cli: Format,
    model: &str,
    tx: tokio::sync::mpsc::Sender<Result<Bytes, std::io::Error>>,
) where
    S: futures::Stream<Item = Result<Bytes, E>> + Unpin,
{
    use futures::StreamExt;

    let mut parser = SseParser::new();
    let mut stage1 = make_stage1(up);
    let mut stage2 = make_stage2(cli);
    let mut flushed = false;

    loop {
        let next = tokio::time::timeout(
            std::time::Duration::from_millis(STALL_MS),
            bytes_stream.next(),
        )
        .await;
        let chunk = match next {
            Ok(Some(Ok(c))) => c,
            Ok(Some(Err(_))) | Ok(None) => break,
            Err(_) => {
                // stall: keep-alive comment
                if tx
                    .send(Ok(Bytes::from(": ninty-router idle\n\n")))
                    .await
                    .is_err()
                {
                    return;
                }
                continue;
            }
        };
        for payload in parser.feed(&chunk) {
            if payload == "[DONE]" {
                flush_all(
                    &mut *stage1,
                    &mut *stage2,
                    up,
                    cli,
                    model,
                    &tx,
                    &mut flushed,
                )
                .await;
                if cli == Format::Openai {
                    let _ = tx.send(Ok(Bytes::from("data: [DONE]\n\n"))).await;
                }
                return;
            }
            let parsed: Value = match serde_json::from_str(&payload) {
                Ok(v) => v,
                Err(_) => continue,
            };
            for mid in stage1.handle(&parsed) {
                emit_stage2(&mid, &mut *stage2, cli, model, &tx).await;
            }
        }
    }
    flush_all(
        &mut *stage1,
        &mut *stage2,
        up,
        cli,
        model,
        &tx,
        &mut flushed,
    )
    .await;
}

trait Stage: Send {
    fn handle(&mut self, chunk: &Value) -> Vec<Value>;
    fn flush(&mut self) -> Vec<Value>;
}

struct Identity;
impl Stage for Identity {
    fn handle(&mut self, chunk: &Value) -> Vec<Value> {
        vec![chunk.clone()]
    }
    fn flush(&mut self) -> Vec<Value> {
        vec![]
    }
}

macro_rules! impl_stage {
    ($t:ty) => {
        impl Stage for $t {
            fn handle(&mut self, chunk: &Value) -> Vec<Value> {
                self.handle(chunk)
            }
            fn flush(&mut self) -> Vec<Value> {
                self.flush()
            }
        }
    };
}
impl_stage!(translator::stream::ClaudeToOpenAI);
impl_stage!(translator::stream::OpenAIToClaude);
impl_stage!(translator::gemini::GeminiToOpenAI);
impl_stage!(translator::responses::ResponsesToOpenAI);
impl_stage!(translator::gemini::OpenAIToGemini);

fn make_stage1(up: Format) -> Box<dyn Stage> {
    match up {
        Format::Openai => Box::new(Identity),
        Format::Claude => Box::new(translator::stream::ClaudeToOpenAI::new()),
        Format::Gemini => Box::new(translator::gemini::GeminiToOpenAI::new()),
        Format::Responses => Box::new(translator::responses::ResponsesToOpenAI::new()),
    }
}

fn make_stage2(cli: Format) -> Box<dyn Stage> {
    match cli {
        Format::Openai | Format::Responses => Box::new(Identity),
        Format::Claude => Box::new(translator::stream::OpenAIToClaude::new()),
        Format::Gemini => Box::new(translator::gemini::OpenAIToGemini::new()),
    }
}

async fn emit_stage2(
    mid: &Value,
    stage2: &mut dyn Stage,
    cli: Format,
    model: &str,
    tx: &tokio::sync::mpsc::Sender<Result<Bytes, std::io::Error>>,
) {
    for ev in stage2.handle(mid) {
        let _ = tx
            .send(Ok(Bytes::from(serialize_event(cli, &ev, model))))
            .await;
    }
}

async fn flush_all(
    stage1: &mut dyn Stage,
    stage2: &mut dyn Stage,
    _up: Format,
    cli: Format,
    model: &str,
    tx: &tokio::sync::mpsc::Sender<Result<Bytes, std::io::Error>>,
    flushed: &mut bool,
) {
    if *flushed {
        return;
    }
    *flushed = true;
    for mid in stage1.flush() {
        emit_stage2(&mid, stage2, cli, model, tx).await;
    }
    for ev in stage2.flush() {
        let _ = tx
            .send(Ok(Bytes::from(serialize_event(cli, &ev, model))))
            .await;
    }
}

fn serialize_event(format: Format, ev: &Value, model: &str) -> String {
    let mut ev = ev.clone();
    if ev.get("model").is_none() {
        ev["model"] = Value::String(model.to_string());
    }
    match format {
        // claude SSE carries an event: line with the block type
        Format::Claude => {
            let ty = ev.get("type").and_then(Value::as_str).unwrap_or("message");
            format!("event: {ty}\ndata: {}\n\n", ev)
        }
        _ => format!("data: {ev}\n\n"),
    }
}

// ---------------------------------------------------------------------------
// targets
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub(crate) struct Target {
    pub(crate) conn_id: Option<String>,
    pub(crate) provider_id: String,
    pub(crate) base_url: String,
    pub(crate) url_override: Option<String>,
    pub(crate) format: WireFormat,
    pub(crate) auth: AuthStyle,
    pub(crate) credential: String,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) model: String,
    pub(crate) timeout_ms: u64,
    pub(crate) url_style: UrlStyle,
    pub(crate) url_suffix: String,
    pub(crate) backoff_level: u32,
    pub(crate) force_stream: bool,
    pub(crate) oauth_refresh: bool,
    pub(crate) qoder_creds: Option<engine::qoder::CosyCreds>,
    pub(crate) vertex_sa: Option<String>,
    pub(crate) vertex_project: Option<String>,
    pub(crate) vertex_location: Option<String>,
}

pub(crate) async fn resolve_targets(
    state: &Arc<AppState>,
    spec: &str,
    client_format: WireFormat,
) -> ninty_core::error::Result<Vec<Target>> {
    let (prefix, model) = registry::resolve(spec)
        .map(|(p, m)| (Some(p), m))
        .unwrap_or((
            None,
            spec.split_once('/')
                .map(|(_, m)| m.to_string())
                .unwrap_or_default(),
        ));
    let prefix_str = spec.split('/').next().unwrap_or("");

    // custom node?
    if prefix.is_none() {
        for node in nodes::list(&state.db).await? {
            if node.prefix() != Some(prefix_str) {
                continue;
            }
            if let Some(models) = node.data.get("models").and_then(|m| m.as_array()) {
                if !models.is_empty() && !models.iter().any(|m| m.as_str() == Some(&model)) {
                    continue;
                }
            }
            let format = if node.api_type() == "anthropic" {
                WireFormat::Claude
            } else {
                WireFormat::Openai
            };
            return Ok(vec![Target {
                conn_id: None,
                provider_id: format!("node:{prefix_str}"),
                base_url: node.base_url().unwrap_or("").to_string(),
                url_override: node.chat_url(),
                format,
                auth: AuthStyle::Bearer,
                credential: node.api_key().unwrap_or("").to_string(),
                headers: vec![],
                model,
                timeout_ms: 120_000,
                url_style: UrlStyle::Plain,
                url_suffix: String::new(),
                backoff_level: 0,
                force_stream: false,
                oauth_refresh: false,
                qoder_creds: None,
                vertex_sa: None,
                vertex_project: None,
                vertex_location: None,
            }]);
        }
        return Err(Error::BadRequest(format!("unknown model '{spec}'")));
    }

    let provider = prefix.unwrap();
    let upstream_model = registry::find_model(provider, &model)
        .map(|m| m.upstream_model_id.unwrap_or(m.id).to_string())
        .unwrap_or_else(|| model.clone());
    if !provider.models.is_empty() && registry::find_model(provider, &model).is_none() {
        return Err(Error::BadRequest(format!("unknown model '{spec}'")));
    }

    let conns = connections::list(&state.db, Some(provider.id)).await?;
    let now = chrono::Utc::now();
    let mut conns: Vec<_> = conns
        .into_iter()
        .filter(|c| {
            if !c.is_active {
                return false;
            }
            match c
                .data
                .get(format!("modelLock_{model}"))
                .and_then(|v| v.as_str())
            {
                Some(lock) => chrono::DateTime::parse_from_rfc3339(lock)
                    .map(|t| t.with_timezone(&chrono::Utc) <= now)
                    .unwrap_or(true),
                None => true,
            }
        })
        .collect();

    // strategy ordering
    let app_settings = settings::get(&state.db).await.unwrap_or_default();
    let strategy = app_settings
        .provider_strategies
        .get(provider.id)
        .and_then(|s| s.fallback_strategy.as_deref())
        .unwrap_or("priority");
    if strategy == "round-robin" {
        let sticky = app_settings
            .provider_strategies
            .get(provider.id)
            .and_then(|s| s.sticky_round_robin_limit)
            .unwrap_or(app_settings.sticky_round_robin_limit);
        conns.sort_by_key(|c| {
            (
                c.data
                    .get("lastUsedAt")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .unwrap_or_default(),
                c.priority,
            )
        });
        if let Some(last) = conns.last() {
            let count = last
                .data
                .get("consecutiveUseCount")
                .and_then(|c| c.as_i64())
                .unwrap_or(0);
            if count > 0 && (count as u32) < sticky {
                let last = conns.pop().unwrap();
                conns.insert(0, last);
            }
        }
    } else {
        conns.sort_by_key(|c| c.priority);
    }

    let mut targets = Vec::new();
    for c in conns {
        // proactive OAuth refresh (lead window)
        let c = match crate::oauth_state::ensure_fresh(state, &c).await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("oauth refresh failed for {}: {e}", c.id);
                continue;
            }
        };
        // zero-translation fast path: alt transport matching client format
        let mut transport = provider.transport;
        if let Some(api_type) = c.data.get("apiType").and_then(|v| v.as_str()) {
            let want = match api_type {
                "anthropic" | "claude" => Some(WireFormat::Claude),
                "openai" => Some(WireFormat::Openai),
                "gemini" => Some(WireFormat::Gemini),
                _ => None,
            };
            if let Some(w) = want {
                if let Some(alt) = provider.alt_transports.iter().find(|t| t.format == w) {
                    transport = *alt;
                }
            }
        } else if let Some(alt) = provider
            .alt_transports
            .iter()
            .find(|t| t.format == client_format)
        {
            transport = *alt;
        }

        let credential = if provider.no_auth {
            String::new()
        } else if provider.id == "github" {
            c.data
                .get("copilotToken")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        } else {
            c.api_key()
                .or_else(|| c.data.get("accessToken").and_then(|v| v.as_str()))
                .unwrap_or("")
                .to_string()
        };
        if credential.is_empty() && !provider.no_auth && provider.id != "vertex" {
            continue;
        }
        let base_url = c
            .data
            .get("baseUrl")
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_else(|| transport.base_url.to_string());
        targets.push(Target {
            conn_id: Some(c.id.clone()),
            provider_id: provider.id.to_string(),
            base_url,
            url_override: None,
            format: transport.format,
            auth: transport.auth,
            credential,
            headers: transport
                .headers
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            model: upstream_model.clone(),
            timeout_ms: transport.timeout_ms,
            url_style: transport.url_style,
            url_suffix: transport.url_suffix.to_string(),
            backoff_level: c
                .data
                .get("backoffLevel")
                .and_then(|l| l.as_u64())
                .unwrap_or(0) as u32,
            force_stream: transport.force_stream,
            oauth_refresh: matches!(
                provider.id,
                "claude"
                    | "codex"
                    | "github"
                    | "kiro"
                    | "cline"
                    | "codebuddy-cn"
                    | "codebuddy-intl"
            ),
            qoder_creds: if provider.id == "qoder" {
                Some(engine::qoder::CosyCreds {
                    user_id: c
                        .data
                        .get("userId")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .into(),
                    auth_token: c
                        .data
                        .get("accessToken")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .into(),
                    name: c
                        .data
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .into(),
                    email: c
                        .data
                        .get("email")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .into(),
                    machine_id: c
                        .data
                        .get("machineId")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .into(),
                })
            } else {
                None
            },
            vertex_sa: c
                .data
                .get("serviceAccountJson")
                .or_else(|| c.data.get("service_account_json"))
                .and_then(|v| v.as_str())
                .map(String::from),
            vertex_project: c
                .data
                .get("project")
                .and_then(|v| v.as_str())
                .map(String::from),
            vertex_location: c
                .data
                .get("location")
                .and_then(|v| v.as_str())
                .map(String::from),
        });
    }
    if targets.is_empty() {
        return Err(Error::BadRequest(format!(
            "no active accounts for '{spec}'"
        )));
    }
    Ok(targets)
}

pub(crate) async fn build_url_and_auth(
    state: &Arc<AppState>,
    target: &Target,
    stream: bool,
) -> ninty_core::error::Result<(String, Vec<(String, String)>)> {
    let mut headers = target.headers.clone();
    if let Some(url) = &target.url_override {
        if !target.credential.is_empty() {
            headers.push((
                "authorization".into(),
                format!("Bearer {}", target.credential),
            ));
        }
        return Ok((url.clone(), headers));
    }

    let base = target.base_url.trim_end_matches('/');
    let action = if stream {
        "streamGenerateContent?alt=sse"
    } else {
        "generateContent"
    };
    #[allow(unused_mut)]
    let mut credential = target.credential.clone();
    let url = match target.url_style {
        UrlStyle::VertexModelAction => {
            let sa = target.vertex_sa.as_deref().ok_or_else(|| {
                Error::BadRequest("vertex connection missing serviceAccountJson".into())
            })?;
            credential = engine::oauth::vertex::mint_access_token(&state.http, sa).await?;
            let project = target.vertex_project.as_deref().unwrap_or("");
            let location = target.vertex_location.as_deref().unwrap_or("global");
            format!(
                "{base}/v1/projects/{project}/locations/{location}/publishers/google/models/{}:{action}",
                target.model
            )
        }
        UrlStyle::ModelAction => format!("{base}/{}:{action}", target.model),
        UrlStyle::Plain => {
            let path = match target.format {
                WireFormat::Openai => "/chat/completions",
                WireFormat::Claude => "/messages",
                WireFormat::Gemini => "",
                WireFormat::Responses => "/responses",
            };
            // base may already be a full endpoint (e.g. opencode, glm)
            if base.ends_with(path) && !path.is_empty() {
                format!("{base}{}", target.url_suffix)
            } else {
                format!("{base}{path}{}", target.url_suffix)
            }
        }
    };

    // cline: extra client headers + workos token prefix
    if target.provider_id == "cline" {
        headers.extend([
            ("x-platform".into(), std::env::consts::OS.into()),
            (
                "x-platform-version".into(),
                env!("CARGO_PKG_VERSION").into(),
            ),
            ("x-client-type".into(), "9router".into()),
            ("x-client-version".into(), env!("CARGO_PKG_VERSION").into()),
            ("x-core-version".into(), env!("CARGO_PKG_VERSION").into()),
            ("x-is-multiroot".into(), "false".into()),
        ]);
        credential = engine::oauth::refresh::cline_workos(&credential);
    }

    let url = match target.auth {
        AuthStyle::Bearer => {
            if !credential.is_empty() {
                headers.push(("authorization".into(), format!("Bearer {credential}")));
            }
            url
        }
        AuthStyle::XApiKey => {
            headers.push(("x-api-key".into(), credential));
            url
        }
        AuthStyle::QueryKey => {
            let sep = if url.contains('?') { "&" } else { "?" };
            format!("{url}{sep}key={credential}")
        }
        AuthStyle::PublicToken => {
            headers.push(("authorization".into(), "Bearer public".into()));
            url
        }
    };
    Ok((url, headers))
}

async fn send_request(
    client: &reqwest::Client,
    url: &str,
    headers: &[(String, String)],
    body: &Value,
    timeout_ms: u64,
) -> Result<reqwest::Response, (u16, String)> {
    let mut attempt = 0u32;
    loop {
        let mut req = client
            .post(url)
            .timeout(std::time::Duration::from_millis(
                FIRST_BYTE_MS.max(timeout_ms),
            ))
            .header("content-type", "application/json");
        for (k, v) in headers {
            req = req.header(k, v);
        }
        match req.json(body).send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                if let Some((max_attempts, delay_ms)) = retry_config_for(status) {
                    attempt += 1;
                    if attempt < max_attempts {
                        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                        continue;
                    }
                }
                return Ok(resp);
            }
            Err(e) => {
                attempt += 1;
                if attempt < 2 && (e.is_connect() || e.is_timeout()) {
                    tokio::time::sleep(std::time::Duration::from_millis(1_000)).await;
                    continue;
                }
                return Err((502, format!("upstream request failed: {e}")));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// misc helpers
// ---------------------------------------------------------------------------

fn inject_claude_identity(body: &mut Value) {
    let mut system = body
        .get("system")
        .and_then(|s| s.as_array())
        .cloned()
        .unwrap_or_default();
    let already = system.iter().any(|b| {
        b.get("type").and_then(|t| t.as_str()) == Some("text")
            && b.get("text")
                .and_then(|t| t.as_str())
                .map(|t| t.starts_with("You are Claude Code"))
                .unwrap_or(false)
    });
    if !already {
        system.insert(
            0,
            json!({"type": "text", "text": "You are Claude Code, Anthropic's official CLI."}),
        );
    }
    if !system.is_empty() {
        body["system"] = Value::Array(system);
    }
}

/// Combo capability reorder: image/pdf in last user message → vision-capable models first.
fn reorder_by_capability(models: Vec<String>, body: &Value) -> Vec<String> {
    const VISION_PREFIXES: &[&str] = &[
        "anthropic/",
        "claude/",
        "openai/",
        "gemini/",
        "vertex/",
        "vx/",
    ];
    let needs_vision = body
        .get("messages")
        .and_then(|m| m.as_array())
        .map(|msgs| {
            msgs.iter().rev().any(|m| {
                m.get("role").and_then(|r| r.as_str()) == Some("user")
                    && m.get("content")
                        .and_then(|c| c.as_array())
                        .map(|parts| {
                            parts.iter().any(|p| {
                                let t = p.get("type").and_then(|t| t.as_str()).unwrap_or("");
                                t.contains("image") || t.contains("pdf") || t.contains("document")
                            })
                        })
                        .unwrap_or(false)
            })
        })
        .unwrap_or(false);
    if !needs_vision {
        return models;
    }
    let (mut vision, mut rest): (Vec<String>, Vec<String>) = (vec![], vec![]);
    for m in models {
        if VISION_PREFIXES.iter().any(|p| m.starts_with(p)) {
            vision.push(m);
        } else {
            rest.push(m);
        }
    }
    vision.extend(rest);
    vision
}

fn usage_of(format: WireFormat, body: &Value) -> (i64, i64) {
    let u = body.get("usage");
    match format {
        WireFormat::Claude => (
            u.and_then(|u| u.get("input_tokens"))
                .and_then(Value::as_i64)
                .unwrap_or(0),
            u.and_then(|u| u.get("output_tokens"))
                .and_then(Value::as_i64)
                .unwrap_or(0),
        ),
        WireFormat::Gemini => (
            u.and_then(|u| u.get("promptTokenCount"))
                .and_then(Value::as_i64)
                .unwrap_or(0),
            u.and_then(|u| u.get("candidatesTokenCount"))
                .and_then(Value::as_i64)
                .unwrap_or(0),
        ),
        WireFormat::Openai | WireFormat::Responses => (
            u.and_then(|u| u.get("prompt_tokens"))
                .and_then(Value::as_i64)
                .unwrap_or(0),
            u.and_then(|u| u.get("completion_tokens"))
                .and_then(Value::as_i64)
                .unwrap_or(0),
        ),
    }
}

#[allow(clippy::too_many_arguments)]
async fn record_meta(
    state: &Arc<AppState>,
    provider: &str,
    model: &str,
    conn_id: Option<&str>,
    api_key: Option<&str>,
    endpoint: &str,
    prompt: i64,
    completion: i64,
    status: &str,
    error_kind: Option<&str>,
    rtk_saved: i64,
    ctx: Option<&DetailCtx>,
) {
    let meta = match (error_kind, rtk_saved > 0) {
        (Some(k), true) => Some(json!({"error_kind": k, "rtk_saved_bytes": rtk_saved})),
        (Some(k), false) => Some(json!({"error_kind": k})),
        (None, true) => Some(json!({"rtk_saved_bytes": rtk_saved})),
        (None, false) => None,
    };
    let rec = usage::UsageRecord {
        provider: provider.into(),
        model: model.into(),
        connection_id: conn_id.map(String::from),
        api_key: api_key.map(String::from),
        endpoint: endpoint.into(),
        prompt_tokens: prompt,
        completion_tokens: completion,
        cost: 0.0,
        status: status.into(),
        meta,
    };
    let _ = usage::record(&state.db, rec).await;
    if state.enable_request_logs() {
        let extra = ctx.map(|c| {
            let mut m = json!({
                "request": c.request,
                "providerRequest": c.provider_request,
                "latencyMs": c.started.elapsed().as_millis() as u64,
            });
            if let Some(p) = &c.pxpipe {
                m["pxpipe"] = p.clone();
            }
            m
        });
        let _ = usage::insert_request_detail(
            &state.db,
            usage::RequestDetail {
                provider: provider.into(),
                model: model.into(),
                status: status.into(),
                input_tokens: prompt,
                output_tokens: completion,
                endpoint: endpoint.into(),
                extra,
            },
        )
        .await;
    }
}

#[allow(clippy::too_many_arguments)]
async fn record(
    state: &Arc<AppState>,
    provider: &str,
    model: &str,
    conn_id: Option<&str>,
    api_key: Option<&str>,
    endpoint: &str,
    prompt: i64,
    completion: i64,
    status: &str,
    error_kind: Option<&str>,
    ctx: Option<&DetailCtx>,
) {
    let meta = error_kind.map(|k| json!({"error_kind": k}));
    let rec = usage::UsageRecord {
        provider: provider.into(),
        model: model.into(),
        connection_id: conn_id.map(String::from),
        api_key: api_key.map(String::from),
        endpoint: endpoint.into(),
        prompt_tokens: prompt,
        completion_tokens: completion,
        cost: 0.0,
        status: status.into(),
        meta,
    };
    let _ = usage::record(&state.db, rec).await;
    if state.enable_request_logs() {
        let extra = ctx.map(|c| {
            let mut m = json!({
                "request": c.request,
                "providerRequest": c.provider_request,
                "latencyMs": c.started.elapsed().as_millis() as u64,
            });
            if let Some(p) = &c.pxpipe {
                m["pxpipe"] = p.clone();
            }
            m
        });
        let _ = usage::insert_request_detail(
            &state.db,
            usage::RequestDetail {
                provider: provider.into(),
                model: model.into(),
                status: status.into(),
                input_tokens: prompt,
                output_tokens: completion,
                endpoint: endpoint.into(),
                extra,
            },
        )
        .await;
    }
}

async fn sticky_limit(state: &Arc<AppState>, provider: &str) -> u32 {
    settings::get(&state.db)
        .await
        .map(|s| {
            s.provider_strategies
                .get(provider)
                .and_then(|p| p.sticky_round_robin_limit)
                .unwrap_or(s.sticky_round_robin_limit)
        })
        .unwrap_or(3)
}

/// Counts streamed bytes; on end records usage via callback.
struct CountingStream<F: FnMut(u64)> {
    rx: tokio::sync::mpsc::Receiver<Result<Bytes, std::io::Error>>,
    total: u64,
    on_end: Option<F>,
}

impl<F: FnMut(u64)> CountingStream<F> {
    fn new(rx: tokio::sync::mpsc::Receiver<Result<Bytes, std::io::Error>>, on_end: F) -> Self {
        Self {
            rx,
            total: 0,
            on_end: Some(on_end),
        }
    }
}

impl<F: FnMut(u64) + Unpin> futures::Stream for CountingStream<F> {
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        use std::task::Poll::*;
        match self.rx.poll_recv(cx) {
            Ready(Some(Ok(bytes))) => {
                self.total += bytes.len() as u64;
                Ready(Some(Ok(bytes)))
            }
            Ready(Some(Err(e))) => Ready(Some(Err(e))),
            Ready(None) => {
                if let Some(mut f) = self.on_end.take() {
                    f(self.total);
                }
                Ready(None)
            }
            Pending => Pending,
        }
    }
}

use axum::response::IntoResponse;

/// Buffer a forced-stream upstream into one non-streaming client response.
#[allow(clippy::too_many_arguments)]
async fn collect_forced_stream(
    state: &Arc<AppState>,
    target: Target,
    client_format: WireFormat,
    endpoint: &'static str,
    key_str: &Option<String>,
    rtk_saved: i64,
    request_model: &str,
    resp: reqwest::Response,
    detail_ctx: Option<DetailCtx>,
) -> Response {
    use futures::StreamExt;
    let mut parser = SseParser::new();
    let mut acc = make_stage1(to_format(target.format));
    let mut text = String::new();
    let mut tool_calls: Vec<Value> = vec![];
    let mut usage: Option<(i64, i64)> = None;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let Ok(chunk) = chunk else { break };
        for payload in parser.feed(&chunk) {
            if payload == "[DONE]" {
                break;
            }
            let Ok(ev) = serde_json::from_str::<Value>(&payload) else {
                continue;
            };
            for out in acc.handle(&ev) {
                if let Some(u) = out.get("usage") {
                    let p = u.get("prompt_tokens").and_then(Value::as_i64).unwrap_or(0);
                    let c = u
                        .get("completion_tokens")
                        .and_then(Value::as_i64)
                        .unwrap_or(0);
                    usage = Some((p, c));
                }
                if let Some(delta) = out
                    .get("choices")
                    .and_then(|c| c.as_array())
                    .and_then(|a| a.first())
                    .and_then(|c| c.get("delta"))
                {
                    if let Some(t) = delta.get("content").and_then(Value::as_str) {
                        text.push_str(t);
                    }
                    if let Some(calls) = delta.get("tool_calls").and_then(Value::as_array) {
                        tool_calls.extend(calls.iter().cloned());
                    }
                }
            }
        }
    }
    let (p, c) = usage.unwrap_or((0, 0));
    let mut message = json!({"role": "assistant", "content": text});
    if !tool_calls.is_empty() {
        message["tool_calls"] = Value::Array(tool_calls);
    }
    let openai = json!({
        "id": "chatcmpl-collected",
        "object": "chat.completion",
        "choices": [{"index": 0, "message": message, "finish_reason": "stop"}],
        "usage": {"prompt_tokens": p, "completion_tokens": c},
    });
    let out = match client_format {
        WireFormat::Openai | WireFormat::Responses => openai,
        f => translator::from_openai_json(to_format(f), &openai, request_model).unwrap_or(openai),
    };
    record_meta(
        state,
        &target.provider_id,
        request_model,
        target.conn_id.as_deref(),
        key_str.as_deref(),
        endpoint,
        p,
        c,
        "success",
        None,
        rtk_saved,
        detail_ctx.as_ref(),
    )
    .await;
    Json(out).into_response()
}

// ---------------------------------------------------------------------------
// qoder custom executor (M08)
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn run_qoder(
    state: &Arc<AppState>,
    target: &Target,
    body: &Value,
    client_format: WireFormat,
    endpoint: &'static str,
    stream: bool,
    key_str: &Option<String>,
) -> Result<Response, AttemptErr> {
    let creds = target
        .qoder_creds
        .clone()
        .filter(|c| !c.user_id.is_empty() && !c.auth_token.is_empty())
        .ok_or((
            400,
            "qoder connection missing userId/accessToken — reconnect".into(),
            Verdict::NoFallback,
        ))?;

    // model_config: kv cache per conn+model, else live fetch
    let cache_key = format!(
        "qoder-mc:{}:{}",
        target.conn_id.as_deref().unwrap_or(""),
        target.model
    );
    let cached: Option<Value> = state
        .db
        .call({
            let k = cache_key.clone();
            move |conn| {
                Ok(conn
                    .query_row("SELECT value FROM kv WHERE key = ?1", [&k], |r| {
                        r.get::<_, String>(0)
                    })
                    .ok()
                    .and_then(|raw| serde_json::from_str(&raw).ok()))
            }
        })
        .await
        .ok()
        .flatten();
    let model_config = match cached {
        Some(v) => v,
        None => {
            let v = engine::qoder::fetch_model_config(&state.http, &creds, &target.model)
                .await
                .map_err(|e| {
                    (
                        502,
                        e.to_string(),
                        Verdict::Fallback {
                            cooldown_ms: 30_000,
                        },
                    )
                })?;
            let (k, raw) = (cache_key.clone(), v.to_string());
            let _ = state
                .db
                .call(move |conn| {
                    conn.execute(
                        "INSERT INTO kv (key, value) VALUES (?1, ?2)
                         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                        [&k, &raw],
                    )
                    .map_err(|e| Error::Db(e.to_string()))?;
                    Ok(())
                })
                .await;
            v
        }
    };

    // translate to openai (client format → openai), then qoder body
    let mut openai_body = body.clone();
    openai_body["model"] = Value::String(target.model.clone());
    if client_format != WireFormat::Openai {
        openai_body = translator::to_openai_request(to_format(client_format), &openai_body)
            .map_err(|e| (500, e.to_string(), Verdict::NoFallback))?;
    }
    let qoder_body =
        engine::qoder::build_chat_body(&target.model, &openai_body, &model_config, &creds.user_id);
    let body_bytes = qoder_body.to_string();
    let headers =
        engine::qoder::build_cosy_headers(body_bytes.as_bytes(), engine::qoder::CHAT_URL, &creds)
            .map_err(|e| (500, e.to_string(), Verdict::NoFallback))?;

    let resp = send_request(
        &state.http,
        engine::qoder::CHAT_URL,
        &headers,
        &qoder_body,
        120_000,
    )
    .await
    .map_err(|(st, tx)| {
        (
            st,
            tx,
            Verdict::Fallback {
                cooldown_ms: 30_000,
            },
        )
    })?;
    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap_or_default();
        let verdict = judge(&text, status, target.backoff_level);
        return Err((status, text, verdict));
    }

    // upstream: envelope SSE → unwrap → openai chunks; then standard pipeline
    let request_model = body
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or(&target.model)
        .to_string();
    let unwrapped = QoderUnwrap::new(resp.bytes_stream(), target.model.clone());

    if !stream {
        // collect into single JSON (reuse openai accumulator via stage1 Identity)
        return collect_qoder(
            state,
            target,
            client_format,
            endpoint,
            key_str,
            &request_model,
            unwrapped,
        )
        .await;
    }

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(64);
    let cli_fmt = to_format(client_format);
    let model_for_stream = request_model.clone();
    tokio::spawn(async move {
        stream_pipeline(unwrapped, Format::Openai, cli_fmt, &model_for_stream, tx).await;
    });
    let db = state.db.clone();
    let (provider, conn_id, model, key) = (
        target.provider_id.clone(),
        target.conn_id.clone(),
        request_model.clone(),
        key_str.clone(),
    );
    let counted = CountingStream::new(rx, move |total| {
        let est = (total / 4) as i64;
        let db = db.clone();
        let (provider, conn_id, model, key) = (
            provider.clone(),
            conn_id.clone(),
            model.clone(),
            key.clone(),
        );
        tokio::spawn(async move {
            let _ = usage::record(
                &db,
                usage::UsageRecord {
                    provider,
                    model,
                    connection_id: conn_id,
                    api_key: key,
                    endpoint: endpoint.into(),
                    prompt_tokens: 0,
                    completion_tokens: est,
                    cost: 0.0,
                    status: "success".into(),
                    meta: None,
                },
            )
            .await;
        });
    });
    Response::builder()
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-cache")
        .body(Body::from_stream(counted))
        .map_err(|e| (500, e.to_string(), Verdict::NoFallback))
}

/// Unwraps qoder `{statusCodeValue, body}` SSE envelope → openai chunk bytes.
struct QoderUnwrap<S> {
    inner: S,
    parser: SseParser,
    model: String,
    queue: std::collections::VecDeque<Bytes>,
    done: bool,
}

impl<S> QoderUnwrap<S> {
    fn new(inner: S, model: String) -> Self {
        Self {
            inner,
            parser: SseParser::new(),
            model,
            queue: std::collections::VecDeque::new(),
            done: false,
        }
    }
}

impl<S> futures::Stream for QoderUnwrap<S>
where
    S: futures::Stream<Item = Result<Bytes, reqwest::Error>> + Unpin,
{
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        use std::task::Poll::*;
        loop {
            if let Some(b) = self.queue.pop_front() {
                return Ready(Some(Ok(b)));
            }
            if self.done {
                return Ready(None);
            }
            match std::pin::Pin::new(&mut self.inner).poll_next(cx) {
                Ready(Some(Ok(chunk))) => {
                    for payload in self.parser.feed(&chunk) {
                        if let Some(inner) = engine::qoder::unwrap_envelope(&payload, &self.model) {
                            if inner == "[DONE]" {
                                self.queue.push_back(Bytes::from("data: [DONE]\n\n"));
                                self.done = true;
                                break;
                            }
                            self.queue
                                .push_back(Bytes::from(format!("data: {inner}\n\n")));
                        }
                    }
                }
                Ready(Some(Err(e))) => {
                    self.done = true;
                    return Ready(Some(Err(std::io::Error::other(e))));
                }
                Ready(None) => {
                    self.done = true;
                    self.queue.push_back(Bytes::from("data: [DONE]\n\n"));
                }
                Pending => return Pending,
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn collect_qoder(
    state: &Arc<AppState>,
    target: &Target,
    client_format: WireFormat,
    endpoint: &'static str,
    key_str: &Option<String>,
    request_model: &str,
    unwrapped: QoderUnwrap<impl futures::Stream<Item = Result<Bytes, reqwest::Error>> + Unpin>,
) -> Result<Response, AttemptErr> {
    use futures::StreamExt;
    let mut parser = SseParser::new();
    let mut text = String::new();
    let mut usage_pair = (0i64, 0i64);
    let mut stream = unwrapped;
    while let Some(chunk) = stream.next().await {
        let Ok(chunk) = chunk else { break };
        for payload in parser.feed(&chunk) {
            if payload == "[DONE]" {
                break;
            }
            let Ok(ev) = serde_json::from_str::<Value>(&payload) else {
                continue;
            };
            if let Some(u) = ev.get("usage") {
                usage_pair = (
                    u.get("prompt_tokens").and_then(Value::as_i64).unwrap_or(0),
                    u.get("completion_tokens")
                        .and_then(Value::as_i64)
                        .unwrap_or(0),
                );
            }
            if let Some(t) = ev
                .get("choices")
                .and_then(|c| c.as_array())
                .and_then(|a| a.first())
                .and_then(|c| c.get("delta"))
                .and_then(|d| d.get("content"))
                .and_then(Value::as_str)
            {
                text.push_str(t);
            }
        }
    }
    let openai = json!({
        "id": "chatcmpl-qoder",
        "object": "chat.completion",
        "choices": [{"index": 0, "message": {"role": "assistant", "content": text}, "finish_reason": "stop"}],
        "usage": {"prompt_tokens": usage_pair.0, "completion_tokens": usage_pair.1},
    });
    let out = match client_format {
        WireFormat::Openai | WireFormat::Responses => openai,
        f => translator::from_openai_json(to_format(f), &openai, request_model).unwrap_or(openai),
    };
    record_meta(
        state,
        &target.provider_id,
        request_model,
        target.conn_id.as_deref(),
        key_str.as_deref(),
        endpoint,
        usage_pair.0,
        usage_pair.1,
        "success",
        None,
        0,
        None,
    )
    .await;
    Ok(Json(out).into_response())
}
