//! Compact `git log`: keep headers/subjects/Author/Date, drop bodies.
//! Port of filters/gitLog.js.

use regex::Regex;

pub const GIT_LOG_MAX_LINES: usize = 200;

pub fn git_log(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    let re_commit_plain = Regex::new(r"(?i)^commit [0-9a-f]{7,40}$").unwrap();
    let re_commit_graph = Regex::new(r"(?i)^[*|/\\ ]+commit [0-9a-f]{7,40}").unwrap();
    let re_meta = Regex::new(r"(?i)^[*|/\\ ]*(Author|Date):").unwrap();
    let re_subject = Regex::new(r"^[*|/\\ ]*    \S").unwrap();
    let re_stat = Regex::new(r"^\d+ file\w* changed").unwrap();
    let re_diff = Regex::new(r"^diff --git ").unwrap();
    let re_graph_sha = Regex::new(r"(?i)^[*|/\\ ]+([0-9a-f]{7,40}\s+.+)").unwrap();
    let re_oneline = Regex::new(r"^[0-9a-f]{7,40}\s+").unwrap();
    let re_graph_only = Regex::new(r"^[*|/\\ ]+$").unwrap();
    let re_graph_glyph = Regex::new(r"[*|/\\]").unwrap();

    let mut out: Vec<String> = vec![];
    let mut skipped = 0usize;
    let mut in_commit = false;
    let mut subject_seen = false;

    let push_line = |l: String, out: &mut Vec<String>, skipped: &mut usize| {
        if out.len() < GIT_LOG_MAX_LINES {
            out.push(l);
        } else {
            *skipped += 1;
        }
    };

    for raw in text.lines() {
        let line = raw.trim_end();
        let trimmed = line.trim();

        if re_commit_plain.is_match(trimmed) || re_commit_graph.is_match(trimmed) {
            in_commit = true;
            subject_seen = false;
            push_line(line.to_string(), &mut out, &mut skipped);
            continue;
        }

        if in_commit {
            if re_meta.is_match(trimmed) {
                push_line(trimmed.to_string(), &mut out, &mut skipped);
                continue;
            }
            if trimmed.is_empty() {
                continue;
            }
            if !subject_seen && re_subject.is_match(line) {
                push_line(format!("  Subject: {trimmed}"), &mut out, &mut skipped);
                subject_seen = true;
                continue;
            }
            if re_stat.is_match(trimmed) {
                push_line(format!("  {trimmed}"), &mut out, &mut skipped);
                continue;
            }
            if re_diff.is_match(trimmed) {
                push_line("  ... diff body omitted".into(), &mut out, &mut skipped);
                continue;
            }
            continue;
        }

        if let Some(m) = re_graph_sha.captures(trimmed) {
            push_line(m[1].to_string(), &mut out, &mut skipped);
            continue;
        }
        if re_oneline.is_match(trimmed) {
            push_line(trimmed.to_string(), &mut out, &mut skipped);
            continue;
        }
        if re_graph_only.is_match(trimmed) && re_graph_glyph.is_match(trimmed) {
            continue;
        }
        push_line(trimmed.to_string(), &mut out, &mut skipped);
    }

    if skipped > 0 {
        out.push(format!("... ({skipped} more lines)"));
    }
    let result = out.join("\n");
    if result.is_empty() || result.len() > text.len() {
        return text.to_string();
    }
    result
}
