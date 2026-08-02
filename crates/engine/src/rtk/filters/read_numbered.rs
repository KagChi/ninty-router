//! Numbered file dumps ("  N|content"): head+tail truncation.
//! Port of filters/readNumbered.js.

use super::smart_truncate::{SMART_TRUNCATE_HEAD, SMART_TRUNCATE_MIN_LINES, SMART_TRUNCATE_TAIL};

pub fn read_numbered(input: &str) -> String {
    let lines: Vec<&str> = input.lines().collect();
    if lines.len() < SMART_TRUNCATE_MIN_LINES {
        return input.to_string();
    }
    let cut = lines.len() - SMART_TRUNCATE_HEAD - SMART_TRUNCATE_TAIL;
    let mut out: Vec<String> = lines[..SMART_TRUNCATE_HEAD].iter().map(|s| s.to_string()).collect();
    out.push(format!("... +{cut} lines truncated (file continues)"));
    out.extend(lines[lines.len() - SMART_TRUNCATE_TAIL..].iter().map(|s| s.to_string()));
    out.join("\n")
}
