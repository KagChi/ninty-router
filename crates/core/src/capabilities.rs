//! Model capabilities (vision/reasoning) — port of open-sse/providers/capabilities.js.
//! Resolution order: exact MODEL → PROVIDER[model] → PATTERN (ordered) → default.
//! Only vision/reasoning are ported (UI badge set); thinking metadata stays in
//! the registry transports.

/// (vision, reasoning)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct Caps {
    pub vision: bool,
    pub reasoning: bool,
}

const V: Caps = Caps { vision: true, reasoning: false };
const R: Caps = Caps { vision: false, reasoning: true };
const VR: Caps = Caps { vision: true, reasoning: true };
const NONE: Caps = Caps { vision: false, reasoning: false };

/// Exact model-id overrides (subset of MODEL_CAPABILITIES relevant to shipped providers).
fn exact(model: &str) -> Option<Caps> {
    Some(match model {
        "claude-opus-5" | "claude-opus-4.6" | "claude-opus-4.7" | "claude-opus-4.8"
        | "claude-sonnet-4.6" | "claude-sonnet-5" => VR,
        "glm-4.6v" => VR,
        "vision-model" => VR,
        "coder-model" => R,
        "kimi-k3" | "k3" | "kimi-for-coding" | "kimi-for-coding-highspeed" | "kimi-k2.7-code"
        | "kimi-k2.7-code-highspeed" => VR,
        _ => return None,
    })
}

/// Provider-specific overrides (PROVIDER_CAPABILITIES subset).
fn provider(provider: &str, model: &str) -> Option<Caps> {
    match provider {
        "codebuddy-cn" | "codebuddy-intl" => Some(match model {
            "glm-5v-turbo" | "minimax-m3" | "minimax-m2.7" | "kimi-k2.7" | "kimi-k2.6"
            | "kimi-k2.5" | "hy3-preview" | "deepseek-v4-pro" | "deepseek-v4-flash" => VR,
            "glm-5.2" | "glm-5.1" | "glm-5.0" | "glm-5.0-turbo" | "glm-4.7"
            | "deepseek-v3-2-volc" => R,
            _ => return None,
        }),
        _ => None,
    }
}

/// PATTERN_CAPABILITIES — glob (* wildcard), anchored, case-insensitive, ordered.
const PATTERNS: &[(&str, Caps)] = &[
    ("*claude*opus-5*", VR),
    ("*claude*opus-4.6*", VR),
    ("*claude*opus-4.7*", VR),
    ("*claude*opus-4.8*", VR),
    ("*claude*sonnet-4.6*", VR),
    ("*claude*sonnet-4.7*", VR),
    ("*claude*haiku*", VR),
    ("*claude*opus*", VR),
    ("*claude*sonnet*", VR),
    ("*claude*fable*", VR),
    ("*claude*mythos*", VR),
    ("*claude-3*", V),
    ("*claude*", VR),
    ("*gemini*image*", V),
    ("*gemini-3*pro*", VR),
    ("*gemini-3*", VR),
    ("*gemini-2.5*", VR),
    ("*gemini-2*", V),
    ("*gemini*", V),
    ("*gemma*", V),
    ("*gpt-5*codex*", R),
    ("*gpt-5*", VR),
    ("*gpt-4o*", V),
    ("*gpt-4.1*", V),
    ("*gpt-4-turbo*", V),
    ("*gpt-4*", NONE),
    ("*gpt-oss*", R),
    ("*o1-mini*", R),
    ("*o1*", VR),
    ("*o3*", VR),
    ("*o4*", VR),
    ("*grok-code*", R),
    ("*grok-4.5*", VR),
    ("*grok-4*", VR),
    ("*grok-3*", VR),
    ("*grok*", VR),
    ("*qwen*vl*", VR),
    ("*qwen*omni*", VR),
    ("*qwen*coder*", R),
    ("*qwen*max*", R),
    ("*qwen3.5*", VR),
    ("*qwen3.6*", VR),
    ("*qwen3.7*", VR),
    ("*qwen*plus*", VR),
    ("*qwen*235b*", R),
    ("*qwq*", R),
    ("*qwen*", R),
    ("*kimi*k3*", VR),
    ("*kimi*for-coding*", VR),
    ("*kimi*k2.7*code*", VR),
    ("*kimi*k2*", VR),
    ("*kimi*", R),
    ("*glm-5*", R),
    ("*glm-4.7*", R),
    ("*glm-4*", R),
    ("*glm*", R),
    ("*deepseek-v4*", R),
    ("*reasoner*", R),
    ("*deepseek-r*", R),
    ("*deepseek-chat*", NONE),
    ("*deepseek*", R),
    ("*minimax-m3*", VR),
    ("*minimax-m2.7*", R),
    ("*minimax*", R),
    ("*mimo*v2.5*", V),
    ("*mimo*omni*", V),
    ("*mimo*", V),
    ("*llama-4*", V),
    ("*codestral*", NONE),
    ("*mistral-large*", V),
    ("*command-a-vision*", V),
    ("*laguna-s-2.1*free*", R),
    ("*laguna-s-2.1*", R),
    ("*laguna*", R),
    ("*hunyuan*", R),
    ("hy3*", R),
    ("*step-*", R),
    ("*nemotron*", R),
    ("*ling-*", R),
];

/// Anchored glob: `*` matches any sequence; case-insensitive.
fn glob_match(pattern: &str, text: &str) -> bool {
    let p = pattern.as_bytes();
    let t = text.as_bytes();
    // iterative two-pointer with backtracking on '*'
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star, mut mark) = (usize::MAX, 0usize);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == t[ti] || p[pi] == b'?') {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == b'*' {
            star = pi;
            mark = ti;
            pi += 1;
        } else if star != usize::MAX {
            pi = star + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}

/// Resolve capabilities: exact → provider → pattern → default(none).
pub fn capabilities(provider_id: &str, model: &str) -> Caps {
    let m = model.to_lowercase();
    if let Some(c) = exact(model) {
        return c;
    }
    if let Some(c) = provider(provider_id, model) {
        return c;
    }
    for (pat, caps) in PATTERNS {
        if glob_match(&pat.to_lowercase(), &m) {
            return *caps;
        }
    }
    Caps::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_anchored() {
        assert!(glob_match("*claude*", "claude-opus-4.8"));
        assert!(!glob_match("*claude*", "gpt-4"));
        assert!(glob_match("*claude*", "anthropic-claude-x"));
        assert!(glob_match("hy3*", "hy3-preview"));
        assert!(!glob_match("hy3*", "xhy3"));
        assert!(glob_match("*glm-4.7*", "glm-4.7"));
    }

    #[test]
    fn caps_chain() {
        assert_eq!(capabilities("claude", "claude-opus-4.8"), VR);
        assert_eq!(capabilities("gemini", "gemini-2.5-pro"), VR);
        assert_eq!(capabilities("codebuddy-cn", "glm-5.2"), R);
        assert_eq!(capabilities("codebuddy-cn", "glm-5v-turbo"), VR);
        assert_eq!(capabilities("kimi", "kimi-latest"), R);
        assert_eq!(capabilities("deepseek", "deepseek-v4-pro"), R);
        assert_eq!(capabilities("opencode", "big-pickle"), Caps::default());
        assert_eq!(capabilities("minimax", "minimax-m3"), VR);
        assert_eq!(capabilities("glm", "glm-4.6v"), VR); // exact override beats glm text pattern
    }
}
