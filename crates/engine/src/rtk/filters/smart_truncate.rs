//! Keep head 120 + tail 60 for ≥250-line blobs. Port of filters/smartTruncate.js.

pub const SMART_TRUNCATE_HEAD: usize = 120;
pub const SMART_TRUNCATE_TAIL: usize = 60;
pub const SMART_TRUNCATE_MIN_LINES: usize = 250;

pub fn smart_truncate(input: &str) -> String {
    let lines: Vec<&str> = input.lines().collect();
    if lines.len() < SMART_TRUNCATE_MIN_LINES {
        return input.to_string();
    }
    let cut = lines.len() - SMART_TRUNCATE_HEAD - SMART_TRUNCATE_TAIL;
    let mut out: Vec<String> = lines[..SMART_TRUNCATE_HEAD].iter().map(|s| s.to_string()).collect();
    out.push(format!("... +{cut} lines truncated"));
    out.extend(lines[lines.len() - SMART_TRUNCATE_TAIL..].iter().map(|s| s.to_string()));
    out.join("\n")
}
