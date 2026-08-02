//! System-prompt savers: Caveman (terse style) + Ponytail (lazy senior dev).
//! Prompts ported verbatim from cavemanPrompts.js / ponytailPrompt.js;
//! injection ported from systemInject.js.

use crate::translator::Format;
use serde_json::{json, Value};

const SEP: &str = "\n\n";

// ---------------------------------------------------------------------------
// caveman prompts
// ---------------------------------------------------------------------------

const CV_BOUNDARIES: &str = "Code blocks, file paths, commands, errors, URLs: keep exact. Security warnings, irreversible action confirmations, multi-step ordered sequences: write normal. Resume terse style after.";
const CV_EXAMPLES: &str = "Not: \"Sure! I'd be happy to help you with that. The issue you're experiencing is likely caused by...\" Yes: \"Bug in auth middleware. Token expiry check use `<` not `<=`. Fix:\"";
const CV_AUTO_CLARITY: &str = "Auto-Clarity: drop caveman for security warnings, irreversible actions, multi-step sequences where fragment ambiguity risks misread, or when user repeats a question. Resume after the clear part.";
const CV_PERSISTENCE: &str =
    "ACTIVE EVERY RESPONSE. No revert after many turns. No filler drift. Still active if unsure.";
const CV_NO_INVENTED: &str = "No invented abbreviations. Standard well-known tech acronyms (DB, API, HTTP, URL, JSON, ID, OS, CPU) OK. Names of code symbols, function names, API names, error strings: keep verbatim.";
const CV_PRESERVE_LANG: &str = "Preserve the user's dominant language. User wrote Vietnamese, reply Vietnamese. User wrote English, reply English. Wenyan/classical-Chinese levels override this language-preservation rule. Code identifiers, error strings, file paths, commands: keep in their original form regardless of language.";
const CV_NO_SELF_REF: &str = "No self-reference. Do not name or announce the style (no \"caveman mode\", no \"me caveman think\", no \"compressed mode active\"). Just respond.";
const CV_NO_DECORATION: &str = "No decorative emoji. No narrating tool calls (\"I will now search\", \"I used X to find Y\"). No status phrases (\"Sure!\", \"Of course!\", \"I'd be happy to\"). No causal arrow shorthand (\"A -> B -> fails\"). State the thing, the action, the reason. Then next step.";

pub fn caveman_prompt(level: &str) -> Option<String> {
    let intro: Vec<&str> = match level {
        "lite" => vec![
            "Respond tersely. Keep grammar and full sentences but drop filler, hedging and pleasantries (just/really/basically/sure/of course/I'd be happy to).",
            "Pattern: state the thing, the action, the reason. Then next step.",
        ],
        "full" => vec![
            "Respond like terse caveman. All technical substance stay exact, only fluff die.",
            "Drop: articles (a/an/the), filler (just/really/basically/actually/simply), pleasantries, hedging. Fragments OK. Short synonyms (big not extensive, fix not implement a solution for).",
            "Pattern: [thing] [action] [reason]. [next step].",
        ],
        "ultra" => vec![
            "Respond ultra-terse. Maximum compression. Telegraphic.",
            "Strip conjunctions. One word when one word enough.",
            "Pattern: [thing] [action] [reason]. [next step].",
        ],
        "wenyan-lite" => vec![
            "Respond semi-classical. Drop filler/hedging but keep grammar structure, classical register.",
            "Use classical Chinese sentence patterns where natural. Keep English for technical terms.",
        ],
        "wenyan" => vec![
            "Respond classical Chinese (文言文). Maximum classical terseness. 80-90% character reduction.",
            "Classical sentence patterns, verbs precede objects, subjects often omitted, classical particles (之/乃/為/其).",
            "Keep English for code, commands, function names, API names, error strings.",
        ],
        "wenyan-ultra" => vec![
            "Respond extreme classical compression (文言文 ultra). Maximum compression, ultra terse.",
            "Same classical rules as wenyan-full but even more compressed. One classical particle per clause.",
        ],
        _ => return None,
    };
    let mut parts = intro;
    parts.extend([
        CV_EXAMPLES,
        CV_BOUNDARIES,
        CV_AUTO_CLARITY,
        CV_PERSISTENCE,
        CV_NO_INVENTED,
        CV_PRESERVE_LANG,
        CV_NO_SELF_REF,
        CV_NO_DECORATION,
    ]);
    Some(parts.join(" "))
}

// ---------------------------------------------------------------------------
// ponytail prompts
// ---------------------------------------------------------------------------

const PT_PERSONA: &str = "You are a lazy senior developer. Lazy means efficient, not careless. The best code is the code never written.";
const PT_LADDER: &str = "Before writing code, stop at the first rung that holds: 1) Does this need to exist at all? (YAGNI) 2) Stdlib does it? Use it. 3) Native platform feature covers it? Use it (CSS over JS, DB constraint over app code). 4) Already-installed dependency solves it? Use it; never add a new one for what a few lines can do. 5) Can it be one line? One line. 6) Only then: the minimum code that works.";
const PT_RULES: &str = "No unrequested abstractions (no interface with one implementation, no factory for one product, no config for a value that never changes). No boilerplate or scaffolding \"for later\". Deletion over addition. Boring over clever. Fewest files possible; shortest working diff wins. Two stdlib options the same size: take the edge-case-correct one. Mark deliberate simplifications with a `ponytail:` comment naming the ceiling and upgrade path.";
const PT_OUTPUT: &str = "Code first. Then at most three short lines: what was skipped, when to add it. No essays or design notes. Pattern: `[code] → skipped: [X], add when [Y].`";
const PT_NOT_LAZY: &str = "Never simplify away: input validation at trust boundaries, error handling that prevents data loss, security, accessibility, anything explicitly requested. Non-trivial logic leaves ONE runnable check behind (an assert-based self-check or one small test file; no frameworks). Trivial one-liners need no test.";
const PT_PERSISTENCE: &str =
    "ACTIVE EVERY RESPONSE. No drift back to over-building. Still active if unsure.";

pub fn ponytail_prompt(level: &str) -> Option<String> {
    let intro = match level {
        "lite" => "Lite: build what's asked, but name the lazier alternative in one line. User picks.",
        "full" => "Full: the ladder enforced. Stdlib and native first. Shortest diff, shortest explanation.",
        "ultra" => "Ultra: YAGNI extremist. Deletion before addition. Ship the one-liner and challenge the rest of the requirement in the same response.",
        _ => return None,
    };
    Some(
        [
            PT_PERSONA,
            intro,
            PT_LADDER,
            PT_RULES,
            PT_OUTPUT,
            PT_NOT_LAZY,
            PT_PERSISTENCE,
        ]
        .join(" "),
    )
}

// ---------------------------------------------------------------------------
// injection (port of systemInject.js)
// ---------------------------------------------------------------------------

pub fn inject_system_prompt(body: &mut Value, format: Format, prompt: &str) {
    if prompt.is_empty() {
        return;
    }
    match format {
        Format::Claude => inject_claude(body, prompt),
        Format::Gemini => inject_gemini(body, prompt),
        Format::Openai | Format::Responses => inject_messages(body, prompt),
    }
}

fn inject_messages(body: &mut Value, prompt: &str) {
    if let Some(instr) = body.get("instructions").and_then(Value::as_str) {
        let joined = if instr.is_empty() {
            prompt.to_string()
        } else {
            format!("{instr}{SEP}{prompt}")
        };
        body["instructions"] = Value::String(joined);
        return;
    }
    let key = if body.get("messages").is_some() {
        "messages"
    } else if body.get("input").is_some() {
        "input"
    } else {
        return;
    };
    let Some(arr) = body.get_mut(key).and_then(Value::as_array_mut) else {
        return;
    };
    let idx = arr.iter().position(|m| {
        matches!(
            m.get("role").and_then(Value::as_str),
            Some("system") | Some("developer")
        )
    });
    match idx {
        Some(i) => {
            let msg = &mut arr[i];
            match msg.get_mut("content") {
                Some(Value::String(s)) => {
                    *s = format!("{s}{SEP}{prompt}");
                }
                Some(Value::Array(parts)) => {
                    parts.push(json!({"type": "input_text", "text": prompt}));
                }
                _ => {
                    msg["content"] = Value::String(prompt.to_string());
                }
            }
        }
        None => arr.insert(0, json!({"role": "system", "content": prompt})),
    }
}

fn inject_claude(body: &mut Value, prompt: &str) {
    match body.get_mut("system") {
        Some(Value::String(s)) if !s.is_empty() => {
            *s = format!("{s}{SEP}{prompt}");
        }
        Some(Value::Array(blocks)) => {
            let block = json!({"type": "text", "text": prompt});
            let last_cache = blocks
                .iter()
                .rposition(|b| b.get("cache_control").is_some());
            match last_cache {
                Some(i) => blocks.insert(i, block),
                None => blocks.push(block),
            }
        }
        _ => body["system"] = Value::String(prompt.to_string()),
    }
}

fn inject_gemini(body: &mut Value, prompt: &str) {
    let has_request = body.get("request").is_some();
    let target = if has_request {
        body.get_mut("request").expect("checked above")
    } else {
        body
    };
    let key = if target.get("system_instruction").is_some() {
        "system_instruction"
    } else {
        "systemInstruction"
    };
    if let Some(parts) = target
        .get_mut(key)
        .and_then(|s| s.get_mut("parts"))
        .and_then(Value::as_array_mut)
    {
        parts.push(json!({"text": prompt}));
        return;
    }
    target[key] = json!({"parts": [{"text": prompt}]});
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_array_inserts_before_cache_block() {
        let mut body = json!({"system": [
            {"type": "text", "text": "base"},
            {"type": "text", "text": "cached", "cache_control": {"type": "ephemeral"}}
        ]});
        inject_system_prompt(&mut body, Format::Claude, "PROMPT");
        let sys = body["system"].as_array().unwrap();
        assert_eq!(sys.len(), 3);
        assert_eq!(sys[1]["text"], "PROMPT");
        assert!(sys[2].get("cache_control").is_some());
    }

    #[test]
    fn openai_appends_to_system_message() {
        let mut body = json!({"messages": [
            {"role": "system", "content": "base"},
            {"role": "user", "content": "hi"}
        ]});
        inject_system_prompt(&mut body, Format::Openai, "PROMPT");
        assert_eq!(body["messages"][0]["content"], "base\n\nPROMPT");
    }

    #[test]
    fn openai_inserts_system_when_missing() {
        let mut body = json!({"messages": [{"role": "user", "content": "hi"}]});
        inject_system_prompt(&mut body, Format::Openai, "PROMPT");
        assert_eq!(body["messages"][0]["role"], "system");
    }

    #[test]
    fn gemini_parts() {
        let mut body = json!({"contents": []});
        inject_system_prompt(&mut body, Format::Gemini, "PROMPT");
        assert_eq!(body["systemInstruction"]["parts"][0]["text"], "PROMPT");
    }

    #[test]
    fn prompts_exist() {
        for l in [
            "lite",
            "full",
            "ultra",
            "wenyan-lite",
            "wenyan",
            "wenyan-ultra",
        ] {
            assert!(caveman_prompt(l).is_some(), "{l}");
        }
        for l in ["lite", "full", "ultra"] {
            assert!(ponytail_prompt(l).is_some(), "{l}");
        }
        assert!(caveman_prompt("nope").is_none());
    }
}
