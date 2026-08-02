//! Ordered detection rules — exact port of autodetect.js.
//! Order: git-log → git-diff → git-status → build-output → grep → find → tree → ls
//!        → search-list → read-numbered → dedup-log → smart-truncate → null

use regex::Regex;

use super::filters;
use super::{DETECT_WINDOW, FilterFn};

const SMART_TRUNCATE_MIN_LINES: usize = 250;
const READ_NUMBERED_MIN_HIT_RATIO: f64 = 0.7;

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("static regex")
}

fn is_grep_line(line: &str) -> bool {
    let Some(first) = line.find(':') else { return false };
    let Some(second_rel) = line[first + 1..].find(':') else {
        return false;
    };
    let lineno = &line[first + 1..first + 1 + second_rel];
    !lineno.is_empty() && lineno.chars().all(|c| c.is_ascii_digit())
}

fn is_path_like(line: &str) -> bool {
    let t = line.trim();
    if t.is_empty() {
        return false;
    }
    let b = t.as_bytes();
    if b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/') {
        return true;
    }
    if t.contains(':') {
        return false;
    }
    t.starts_with('.') || t.starts_with('/') || t.contains('/')
}

fn is_mostly_porcelain(head: &str) -> bool {
    let re_porcelain = re(r"(?m)^[ MADRCU?!][ MADRCU?!] \S");
    let lines: Vec<&str> = head.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.len() < 3 {
        return false;
    }
    let hits = lines.iter().filter(|l| re_porcelain.is_match(l)).count();
    hits as f64 / lines.len() as f64 >= 0.6
}

fn is_line_numbered(lines: &[&str]) -> bool {
    let re_line = re(r"^\s*\d+\|");
    let mut hits = 0usize;
    let mut non_empty = 0usize;
    for l in lines.iter().take(100) {
        if l.is_empty() {
            continue;
        }
        non_empty += 1;
        if re_line.is_match(l) {
            hits += 1;
        }
    }
    non_empty >= 5 && hits as f64 / non_empty as f64 >= READ_NUMBERED_MIN_HIT_RATIO
}

pub fn auto_detect(text: &str) -> Option<(&'static str, FilterFn)> {
    let head: String = text.chars().take(DETECT_WINDOW).collect();
    let head = head.as_str();

    if re(r"(?m)^[*|/\\ ]*commit [0-9a-f]{7,40}$").is_match(head) {
        return Some(("git-log", filters::git_log::git_log));
    }
    if re(r"(?m)^diff --git ").is_match(head) || re(r"(?m)^@@ ").is_match(head) {
        return Some(("git-diff", filters::git_diff::git_diff));
    }
    if re(r"(?m)^On branch |^nothing to commit|^Changes (not |to be )|^Untracked files:")
        .is_match(head)
    {
        return Some(("git-status", filters::git_status::git_status));
    }
    // build output BEFORE porcelain: cargo "Compiling" must not read as git-status
    if re(r"(?im)^(npm (warn|error|ERR!)|yarn (warn|error)|\s*Compiling\s+\S+|\s*Downloading\s+\S+|added \d+ package|\[ERROR\]|BUILD (SUCCESS|FAILED)|\s*Finished\s+|Successfully (installed|built)|ERROR:)").is_match(head) {
        return Some(("build-output", filters::build_output::build_output));
    }
    if is_mostly_porcelain(head) {
        return Some(("git-status", filters::git_status::git_status));
    }

    let lines: Vec<&str> = head.lines().collect();
    let non_empty: Vec<&str> = lines.iter().filter(|l| !l.trim().is_empty()).copied().collect();

    if non_empty.iter().take(5).any(|l| is_grep_line(l)) {
        return Some(("grep", filters::grep::grep));
    }
    if non_empty.len() >= 3 && non_empty.iter().all(|l| is_path_like(l)) {
        return Some(("find", filters::find::find));
    }
    if re(r"[├└]──|│  ").is_match(head) {
        return Some(("tree", filters::tree::tree));
    }
    let ls_row = re(r"(?m)^[-dlbcps][rwx-]{9}");
    if re(r"(?m)^total \d+$").is_match(head) || ls_row.find_iter(head).count() >= 3 {
        return Some(("ls", filters::ls::ls));
    }
    if filters::search_list::HEADER_RE.with(|r| r.is_match(head)) {
        return Some(("search-list", filters::search_list::search_list));
    }
    if lines.len() >= SMART_TRUNCATE_MIN_LINES && is_line_numbered(&lines) {
        return Some(("read-numbered", filters::read_numbered::read_numbered));
    }
    if non_empty.len() >= 5 {
        return Some(("dedup-log", filters::dedup_log::dedup_log));
    }
    if text.lines().count() >= SMART_TRUNCATE_MIN_LINES {
        return Some(("smart-truncate", filters::smart_truncate::smart_truncate));
    }
    None
}
