//! PXPIPE: render bulky Claude-format context as dense PNGs via pxpipe-proxy.
//! The transform itself is a Node library (text→PNG rendering); we invoke it
//! through a subprocess shim installed under `$DATA_DIR/pxpipe`. Fail-open like
//! every token saver: any error/timeout leaves the request untouched.
//!
//! Port of $REF/open-sse/rtk/pxpipe.js + $REF/src/lib/pxpipe/{install,loader,service}.js.

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

pub const DEFAULT_TIMEOUT_MS: u64 = 15000;
pub const DEFAULT_MIN_CHARS: usize = 25000;
/// pxpipe's own profitability gate assumes ~4 chars/token; reuse it for the
/// estimated before/after numbers surfaced in stats (marked "estimated" in UI).
const EST_CHARS_PER_TOKEN: usize = 4;
pub const PXPIPE_PACKAGE: &str = "pxpipe-proxy";
const INSTALL_TIMEOUT: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PxpipeSummary {
    pub applied: bool,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_chars: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compressed_body_chars: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imaged_chars: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_bytes: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_before_est: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_after_est: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_saved_est: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub saved_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_owns_control: Option<bool>,
}

fn skipped(reason: &str) -> PxpipeSummary {
    PxpipeSummary {
        applied: false,
        reason: reason.to_string(),
        ..Default::default()
    }
}

fn body_chars(body: &Value) -> usize {
    serde_json::to_string(body).map(|s| s.len()).unwrap_or(0)
}

fn est_tokens(chars: usize) -> usize {
    chars / EST_CHARS_PER_TOKEN
}

#[derive(Debug, Clone)]
pub struct PxpipeOpts {
    pub enabled: bool,
    pub min_chars: usize,
    pub timeout_ms: u64,
    /// Model id passed through to pxpipe for profitability heuristics.
    pub model: String,
}

impl Default for PxpipeOpts {
    fn default() -> Self {
        Self {
            enabled: false,
            min_chars: DEFAULT_MIN_CHARS,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            model: String::new(),
        }
    }
}

// ---------- install state ----------

pub fn pxpipe_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("pxpipe")
}

fn package_root(data_dir: &Path) -> PathBuf {
    pxpipe_dir(data_dir)
        .join("node_modules")
        .join(PXPIPE_PACKAGE)
}

fn library_entry(data_dir: &Path) -> PathBuf {
    package_root(data_dir)
        .join("dist")
        .join("core")
        .join("library.js")
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallInfo {
    pub installed: bool,
    pub version: Option<String>,
    pub path: Option<String>,
}

/// installed = library entry exists on disk.
pub fn get_install_info(data_dir: &Path) -> InstallInfo {
    let pkg_json = package_root(data_dir).join("package.json");
    let entry = library_entry(data_dir);
    if !pkg_json.exists() || !entry.exists() {
        return InstallInfo {
            installed: false,
            version: None,
            path: None,
        };
    }
    let version = std::fs::read_to_string(&pkg_json)
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| v.get("version")?.as_str().map(String::from));
    InstallInfo {
        installed: true,
        version,
        path: Some(package_root(data_dir).to_string_lossy().to_string()),
    }
}

/// Find a JS runtime able to execute the shim. bun preferred (always present in
/// dev; Docker ships node). Same PATH-extension trick as the reference: packaged
/// environments often miss node bin dirs.
pub fn find_runtime() -> Option<String> {
    const EXTRA: &[&str] = &["/usr/local/bin", "/opt/homebrew/bin", "/usr/bin", "/bin"];
    let path = std::env::var("PATH").unwrap_or_default();
    let extended = format!(
        "{}:{}:{}",
        EXTRA.join(":"),
        std::env::var("HOME")
            .map(|h| format!("{h}/.local/bin:{h}/.bun/bin"))
            .unwrap_or_default(),
        path
    );
    for cand in ["bun", "node"] {
        if let Ok(out) = std::process::Command::new("which")
            .arg(cand)
            .env("PATH", &extended)
            .output()
        {
            if out.status.success() {
                let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !p.is_empty() {
                    return Some(p);
                }
            }
        }
    }
    None
}

/// Shim script: reads {body_b64, model, options} on stdin, prints the transform
/// result as JSON on stdout. Written on first use; entry path injected.
fn ensure_shim(data_dir: &Path) -> std::io::Result<PathBuf> {
    let entry = library_entry(data_dir);
    let shim = pxpipe_dir(data_dir).join("shim.mjs");
    let script = format!(
        r#"import {{ pathToFileURL }} from "url";
const mod = await import(pathToFileURL({entry:?}).href);
if (typeof mod.transformAnthropicMessages !== "function") {{
  console.error("installed pxpipe package does not export transformAnthropicMessages");
  process.exit(2);
}}
let chunks = [];
for await (const c of process.stdin) chunks.push(c);
const {{ body_b64, model, options }} = JSON.parse(Buffer.concat(chunks).toString("utf8"));
const result = await mod.transformAnthropicMessages({{
  body: Buffer.from(body_b64, "base64"),
  model,
  options,
}});
process.stdout.write(JSON.stringify({{
  applied: result.applied === true,
  reason: result.reason || null,
  detail: result.detail || null,
  body_b64: result.body ? Buffer.from(result.body).toString("base64") : null,
  info: result.info || null,
  cache: result.cache || null,
}}));
"#,
        entry = entry.to_string_lossy().as_ref()
    );
    // Rewrite only when changed (entry path moves with DATA_DIR).
    if let Ok(existing) = std::fs::read_to_string(&shim) {
        if existing == script {
            return Ok(shim);
        }
    }
    std::fs::create_dir_all(pxpipe_dir(data_dir))?;
    std::fs::write(&shim, script)?;
    Ok(shim)
}

// ---------- transform ----------

#[derive(Debug, Deserialize)]
struct ShimOutput {
    applied: bool,
    reason: Option<String>,
    detail: Option<String>,
    body_b64: Option<String>,
    info: Option<Value>,
    cache: Option<Value>,
}

async fn run_shim(
    data_dir: &Path,
    body: &Value,
    model: &str,
    min_chars: usize,
    timeout: Duration,
) -> Result<ShimOutput, String> {
    let runtime = find_runtime().ok_or("no_js_runtime")?;
    let shim = ensure_shim(data_dir).map_err(|e| e.to_string())?;
    let payload = serde_json::json!({
        "body_b64": base64::engine::general_purpose::STANDARD.encode(body.to_string()),
        "model": model,
        "options": { "minCompressChars": min_chars },
    });
    let mut child = Command::new(runtime)
        .arg(shim)
        .current_dir(pxpipe_dir(data_dir))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut stdin = child.stdin.take().ok_or("no_stdin")?;
    let write_task = tokio::spawn(async move {
        let _ = stdin.write_all(payload.to_string().as_bytes()).await;
    });
    let out = match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => return Err(e.to_string()),
        Err(_) => return Err("timeout".to_string()),
    };
    let _ = write_task.await;
    if !out.status.success() {
        return Err(format!("shim exit {}", out.status));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())
}

/// Transform a Claude-format request body through pxpipe. Returns
/// (new_body, summary) — new_body is None when nothing changed.
/// Caller gates on format == Claude (translator::Format::Claude upstream).
pub async fn compress_with_pxpipe(
    body: &Value,
    opts: &PxpipeOpts,
    data_dir: &Path,
) -> (Option<Value>, PxpipeSummary) {
    if !opts.enabled {
        return (None, skipped("disabled"));
    }
    let started = Instant::now();
    let original_chars = body_chars(body);
    let threshold = if opts.min_chars > 0 {
        opts.min_chars
    } else {
        DEFAULT_MIN_CHARS
    };
    if original_chars < threshold {
        return (
            None,
            PxpipeSummary {
                original_chars: Some(original_chars),
                threshold: Some(threshold),
                ..skipped("below_threshold")
            },
        );
    }
    let info = get_install_info(data_dir);
    if !info.installed {
        return (
            None,
            PxpipeSummary {
                original_chars: Some(original_chars),
                ..skipped("not_installed")
            },
        );
    }
    let timeout = Duration::from_millis(if opts.timeout_ms > 0 {
        opts.timeout_ms
    } else {
        DEFAULT_TIMEOUT_MS
    });
    match run_shim(data_dir, body, &opts.model, threshold, timeout).await {
        Err(e) => {
            let is_timeout = e == "timeout";
            (
                None,
                PxpipeSummary {
                    original_chars: Some(original_chars),
                    duration_ms: Some(started.elapsed().as_millis() as u64),
                    detail: if is_timeout { None } else { Some(e) },
                    ..skipped(if is_timeout {
                        "timeout"
                    } else {
                        "transform_error"
                    })
                },
            )
        }
        Ok(out) if !out.applied => (
            None,
            PxpipeSummary {
                detail: out.detail,
                original_chars: Some(original_chars),
                duration_ms: Some(started.elapsed().as_millis() as u64),
                ..skipped(out.reason.as_deref().unwrap_or("passthrough"))
            },
        ),
        Ok(out) => {
            let Some(b64) = out.body_b64 else {
                return (
                    None,
                    PxpipeSummary {
                        original_chars: Some(original_chars),
                        ..skipped("passthrough")
                    },
                );
            };
            let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64) else {
                return (None, skipped("transform_error"));
            };
            let Ok(new_body) = serde_json::from_slice::<Value>(&bytes) else {
                return (None, skipped("transform_error"));
            };
            let compressed_body_chars = body_chars(&new_body);
            let info = out.info.unwrap_or(Value::Null);
            let imaged_chars = info
                .get("compressedChars")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize;
            let image_count = info.get("imageCount").and_then(Value::as_u64).unwrap_or(0) as usize;
            let image_bytes = info.get("imageBytes").and_then(Value::as_u64).unwrap_or(0) as usize;
            // The transformed body is BIGGER in bytes (base64 PNGs) but cheaper in
            // tokens: images bill by pixels (Anthropic: pixels/750), not by encoded
            // length. After-estimate = remaining-text tokens + image tokens.
            let image_tokens_est = info
                .get("imageTokens")
                .and_then(Value::as_u64)
                .map(|v| v as usize)
                .or_else(|| {
                    info.get("imagePixels")
                        .and_then(Value::as_u64)
                        .map(|p| (p / 750) as usize)
                })
                .unwrap_or(image_count * 4761);
            let tokens_before_est = info
                .get("baselineTokens")
                .and_then(Value::as_u64)
                .map(|v| v as usize)
                .unwrap_or_else(|| est_tokens(original_chars));
            let tokens_after_est =
                est_tokens(original_chars.saturating_sub(imaged_chars)) + image_tokens_est;
            let tokens_saved_est = tokens_before_est.saturating_sub(tokens_after_est);
            let saved_pct = if tokens_before_est > 0 {
                ((tokens_saved_est as f64 / tokens_before_est as f64) * 10000.0).round() / 100.0
            } else {
                0.0
            };
            (
                Some(new_body),
                PxpipeSummary {
                    applied: true,
                    reason: "applied".to_string(),
                    original_chars: Some(original_chars),
                    compressed_body_chars: Some(compressed_body_chars),
                    imaged_chars: Some(imaged_chars),
                    image_count: Some(image_count),
                    image_bytes: Some(image_bytes),
                    tokens_before_est: Some(tokens_before_est),
                    tokens_after_est: Some(tokens_after_est),
                    tokens_saved_est: Some(tokens_saved_est),
                    saved_pct: Some(saved_pct),
                    duration_ms: Some(started.elapsed().as_millis() as u64),
                    cache_owns_control: out
                        .cache
                        .as_ref()
                        .and_then(|c| c.get("ownsCacheControl"))
                        .and_then(Value::as_bool),
                    ..Default::default()
                },
            )
        }
    }
}

/// One-line log string for applied transforms (port of formatPxpipeLog).
pub fn format_pxpipe_log(s: &PxpipeSummary) -> Option<String> {
    if !s.applied {
        return None;
    }
    Some(format!(
        "imaged {}ch → {} image(s) | est {}→{} tokens (-{}%) | {}ms",
        s.imaged_chars.unwrap_or(0),
        s.image_count.unwrap_or(0),
        s.tokens_before_est.unwrap_or(0),
        s.tokens_after_est.unwrap_or(0),
        s.saved_pct.unwrap_or(0.0),
        s.duration_ms.unwrap_or(0),
    ))
}

// ---------- install / health ----------

/// npm/bun install pxpipe-proxy into $DATA_DIR/pxpipe. Blocking caller should
/// spawn; serialized by the API layer (kv flag).
pub async fn install_pxpipe(data_dir: &Path) -> Result<String, String> {
    std::fs::create_dir_all(pxpipe_dir(data_dir)).map_err(|e| e.to_string())?;
    let runtime = find_runtime().ok_or("no JS runtime (node/bun) found")?;
    let is_bun = runtime.ends_with("bun");
    // npm/bun install --prefix <dir> <pkg>
    let (cmd, args): (String, Vec<String>) = if is_bun {
        (
            runtime.clone(),
            vec![
                "add".into(),
                "--cwd".into(),
                pxpipe_dir(data_dir).to_string_lossy().to_string(),
                PXPIPE_PACKAGE.into(),
            ],
        )
    } else {
        let npm = if runtime.ends_with("node") {
            // npm lives next to node
            let dir = Path::new(&runtime)
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_default();
            let cand = dir.join("npm");
            if cand.exists() {
                cand.to_string_lossy().to_string()
            } else {
                "npm".to_string()
            }
        } else {
            "npm".to_string()
        };
        (
            npm,
            vec![
                "install".into(),
                "--prefix".into(),
                pxpipe_dir(data_dir).to_string_lossy().to_string(),
                PXPIPE_PACKAGE.into(),
            ],
        )
    };
    let log_path = pxpipe_dir(data_dir).join("install.log");
    let log = std::fs::File::create(&log_path).map_err(|e| e.to_string())?;
    let out = match tokio::time::timeout(
        INSTALL_TIMEOUT,
        Command::new(cmd)
            .args(args)
            .stdout(log.try_clone().map_err(|e| e.to_string())?)
            .stderr(log)
            .output(),
    )
    .await
    {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => return Err(e.to_string()),
        Err(_) => return Err("install timeout".to_string()),
    };
    if !out.status.success() {
        return Err(format!("install failed, see {}", log_path.display()));
    }
    Ok(get_install_info(data_dir)
        .version
        .unwrap_or_else(|| "unknown".to_string()))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthCheck {
    pub id: String,
    pub label: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthReport {
    pub healthy: bool,
    pub checks: Vec<HealthCheck>,
    pub error: Option<String>,
}

/// PRD health checklist, adapted: installed? → shim runs → test transform
/// answers with a machine-readable reason.
pub async fn run_health_check(data_dir: &Path) -> HealthReport {
    let mut checks = Vec::new();
    let info = get_install_info(data_dir);
    checks.push(HealthCheck {
        id: "installed".into(),
        label: "PXPIPE installed".into(),
        ok: info.installed,
        detail: info.version.as_ref().map(|v| format!("v{v}")),
    });
    if !info.installed {
        return HealthReport {
            healthy: false,
            checks,
            error: Some("pxpipe not installed".into()),
        };
    }
    let runtime = find_runtime();
    checks.push(HealthCheck {
        id: "runtime".into(),
        label: "JS runtime found".into(),
        ok: runtime.is_some(),
        detail: runtime.clone(),
    });
    if runtime.is_none() {
        return HealthReport {
            healthy: false,
            checks,
            error: Some("no JS runtime (node/bun) found".into()),
        };
    }
    let test_body = serde_json::json!({
        "model": "claude-fable-5",
        "max_tokens": 16,
        "messages": [{ "role": "user", "content": "ping" }],
    });
    let started = Instant::now();
    match run_shim(
        data_dir,
        &test_body,
        "claude-fable-5",
        1,
        Duration::from_millis(DEFAULT_TIMEOUT_MS),
    )
    .await
    {
        Ok(out) => {
            checks.push(HealthCheck {
                id: "transform".into(),
                label: "Test request transforms".into(),
                ok: true,
                detail: Some(format!(
                    "{}ms ({})",
                    started.elapsed().as_millis(),
                    out.reason.unwrap_or_else(|| "ok".into())
                )),
            });
            HealthReport {
                healthy: true,
                checks,
                error: None,
            }
        }
        Err(e) => {
            checks.push(HealthCheck {
                id: "transform".into(),
                label: "Test request transforms".into(),
                ok: false,
                detail: Some(e.clone()),
            });
            HealthReport {
                healthy: false,
                checks,
                error: Some(format!("Self-test failed: {e}")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn below_threshold_skips_without_install() {
        let dir = std::env::temp_dir().join("ninty-pxpipe-test-none");
        let body = serde_json::json!({"messages": [{"role": "user", "content": "hi"}]});
        let rt = tokio::runtime::Runtime::new().unwrap();
        let (new_body, s) = rt.block_on(compress_with_pxpipe(
            &body,
            &PxpipeOpts {
                enabled: true,
                ..Default::default()
            },
            &dir,
        ));
        assert!(new_body.is_none());
        assert_eq!(s.reason, "below_threshold");
        assert!(s.original_chars.unwrap() < DEFAULT_MIN_CHARS);
    }

    #[test]
    fn not_installed_skips_fail_open() {
        let dir = std::env::temp_dir().join("ninty-pxpipe-test-none");
        let body =
            serde_json::json!({"messages": [{"role": "user", "content": "x".repeat(30000)}]});
        let rt = tokio::runtime::Runtime::new().unwrap();
        let (new_body, s) = rt.block_on(compress_with_pxpipe(
            &body,
            &PxpipeOpts {
                enabled: true,
                ..Default::default()
            },
            &dir,
        ));
        assert!(new_body.is_none());
        assert_eq!(s.reason, "not_installed");
    }

    #[test]
    fn disabled_skips() {
        let dir = std::env::temp_dir().join("ninty-pxpipe-test-none");
        let body = serde_json::json!({"messages": []});
        let rt = tokio::runtime::Runtime::new().unwrap();
        let (_, s) = rt.block_on(compress_with_pxpipe(&body, &PxpipeOpts::default(), &dir));
        assert_eq!(s.reason, "disabled");
    }

    #[test]
    fn log_line_format() {
        let s = PxpipeSummary {
            applied: true,
            reason: "applied".into(),
            imaged_chars: Some(30000),
            image_count: Some(2),
            tokens_before_est: Some(7500),
            tokens_after_est: Some(1200),
            saved_pct: Some(84.0),
            duration_ms: Some(1200),
            ..Default::default()
        };
        assert_eq!(
            format_pxpipe_log(&s).unwrap(),
            "imaged 30000ch → 2 image(s) | est 7500→1200 tokens (-84%) | 1200ms"
        );
    }
}
