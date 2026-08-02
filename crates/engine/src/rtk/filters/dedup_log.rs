//! Collapse consecutive duplicate lines + blank dedupe + hard cap.
//! Port of filters/dedupLog.js.

pub const DEDUP_LINE_MAX: usize = 2000;

pub fn dedup_log(input: &str) -> String {
    let mut out: Vec<String> = vec![];
    let mut prev: Option<&str> = None;
    let mut run_count = 0usize;
    let mut blank_streak = 0usize;

    macro_rules! flush_run {
        () => {
            if prev.is_some() && run_count > 1 {
                out.push(format!("  ... ({} duplicate lines)", run_count - 1));
            }
        };
    }

    for line in input.lines() {
        if line.trim().is_empty() {
            if blank_streak < 1 {
                out.push(line.to_string());
            }
            blank_streak += 1;
            flush_run!();
            prev = None;
            run_count = 0;
            continue;
        }
        blank_streak = 0;
        if Some(line) == prev {
            run_count += 1;
            continue;
        }
        flush_run!();
        out.push(line.to_string());
        prev = Some(line);
        run_count = 1;
        if out.len() >= DEDUP_LINE_MAX {
            out.push(format!("... (truncated at {DEDUP_LINE_MAX} lines)"));
            return out.join("\n");
        }
    }
    flush_run!();
    out.join("\n")
}
