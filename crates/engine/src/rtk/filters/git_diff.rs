//! Compact unified diff: file headers, per-hunk cap, +/- counting.
//! Port of filters/gitDiff.js.

pub const GIT_DIFF_HUNK_MAX_LINES: usize = 100;

pub fn git_diff(diff: &str) -> String {
    let max_lines = 500;
    let mut result: Vec<String> = Vec::new();
    let mut current_file = String::new();
    let mut added = 0usize;
    let mut removed = 0usize;
    let mut in_hunk = false;
    let mut hunk_shown = 0usize;
    let mut hunk_skipped = 0usize;
    let mut was_truncated = false;

    'outer: for line in diff.lines() {
        if line.starts_with("diff --git") {
            if hunk_skipped > 0 {
                result.push(format!("  ... ({hunk_skipped} lines truncated)"));
                was_truncated = true;
                hunk_skipped = 0;
            }
            if !current_file.is_empty() && (added > 0 || removed > 0) {
                result.push(format!("  +{added} -{removed}"));
            }
            current_file = line
                .split(" b/")
                .skip(1)
                .collect::<Vec<_>>()
                .join(" b/");
            if current_file.is_empty() {
                current_file = "unknown".into();
            }
            result.push(format!("\n{current_file}"));
            added = 0;
            removed = 0;
            in_hunk = false;
            hunk_shown = 0;
        } else if line.starts_with("@@") {
            if hunk_skipped > 0 {
                result.push(format!("  ... ({hunk_skipped} lines truncated)"));
                was_truncated = true;
                hunk_skipped = 0;
            }
            in_hunk = true;
            hunk_shown = 0;
            result.push(format!("  {line}"));
        } else if in_hunk {
            if line.starts_with('+') && !line.starts_with("+++") {
                added += 1;
                if hunk_shown < GIT_DIFF_HUNK_MAX_LINES {
                    result.push(format!("  {line}"));
                    hunk_shown += 1;
                } else {
                    hunk_skipped += 1;
                }
            } else if line.starts_with('-') && !line.starts_with("---") {
                removed += 1;
                if hunk_shown < GIT_DIFF_HUNK_MAX_LINES {
                    result.push(format!("  {line}"));
                    hunk_shown += 1;
                } else {
                    hunk_skipped += 1;
                }
            } else if hunk_shown < GIT_DIFF_HUNK_MAX_LINES && !line.starts_with('\\') && hunk_shown > 0 {
                result.push(format!("  {line}"));
                hunk_shown += 1;
            }
        }

        if result.len() >= max_lines {
            result.push("\n... (more changes truncated)".into());
            was_truncated = true;
            break 'outer;
        }
    }

    if hunk_skipped > 0 {
        result.push(format!("  ... ({hunk_skipped} lines truncated)"));
    }
    if !current_file.is_empty() && (added > 0 || removed > 0) {
        result.push(format!("  +{added} -{removed}"));
    }
    if was_truncated {
        result.push("[full diff: rtk git diff --no-compact]".into());
    }
    result.join("\n")
}
