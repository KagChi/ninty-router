//! RTK filter + autodetect fixtures. Each fixture must compress ≥30%
//! (mirrors the reference behaviour) and autodetect must pick the right filter.

use engine::rtk::{self, autodetect, filters};

fn big(line: &str, n: usize) -> String {
    line.repeat(n)
}

fn pct(input: &str, out: &str) -> f64 {
    (1.0 - out.len() as f64 / input.len() as f64) * 100.0
}

// ---------------------------------------------------------------- fixtures

fn git_diff_fixture() -> String {
    let mut s = String::new();
    for f in 0..8 {
        s.push_str(&format!("diff --git a/src/f{f}.rs b/src/f{f}.rs\nindex 111..222 100644\n--- a/src/f{f}.rs\n+++ b/src/f{f}.rs\n@@ -1,250 +1,250 @@ fn f{f}\n"));
        for i in 0..240 {
            s.push_str(&format!(
                "-old line {i} with some content to make it long enough\n"
            ));
            s.push_str(&format!(
                "+new line {i} with some content to make it long enough\n"
            ));
            s.push_str(" context line\n");
        }
    }
    s
}

fn git_status_fixture() -> String {
    let mut s = String::from("On branch main\nChanges to be committed:\n");
    for i in 0..40 {
        s.push_str(&format!("  new file:   src/new/file{i}.rs\n"));
    }
    s.push_str("Changes not staged for commit:\n");
    for i in 0..40 {
        s.push_str(&format!("  modified:   src/mod/file{i}.rs\n"));
    }
    s.push_str("Untracked files:\n");
    s
}

fn git_log_fixture() -> String {
    let mut s = String::new();
    for i in 0..60 {
        s.push_str(&format!(
            "commit {:040x}\nAuthor: Dev <d@x.io>\nDate:   Mon Aug 1 10:00:00 2026 +0000\n\n    feat: change number {i} with a reasonably long subject\n\n    body paragraph that should be dropped entirely\n    more body text here\n\n",
            i as u64 * 0x1111111 + 0xabcdef
        ));
    }
    s
}

fn build_fixture() -> String {
    let mut s = String::new();
    for i in 0..300 {
        s.push_str(&format!("   Compiling crate{i} v0.1.{i} (/tmp/crate{i})\n"));
        s.push_str(&format!("   Downloading dep{i} v1.0.{i}\n"));
    }
    s.push_str("error[E0308]: mismatched types\n --> src/main.rs:10:5\n  |\n10 |     let x: u32 = \"s\";\n  |     ^^^ expected u32\n");
    s.push_str("warning: unused variable\n");
    s.push_str("    Finished `dev` profile in 42s\n");
    s
}

fn grep_fixture() -> String {
    let mut s = String::new();
    for f in 0..10 {
        for l in 0..30 {
            s.push_str(&format!("src/dir/file{f}.rs:{l}:    let match_{l} = find_pattern_in_codebase({l}); // extra content padding\n"));
        }
    }
    s
}

fn find_fixture() -> String {
    let mut s = String::new();
    for d in 0..15 {
        for f in 0..25 {
            s.push_str(&format!("./src/components/dir{d}/component_file_{f}.tsx\n"));
        }
    }
    s
}

fn ls_fixture() -> String {
    let mut s = String::from("total 9999\n");
    for i in 0..60 {
        s.push_str(&format!(
            "-rw-r--r--  1 user  staff  4096 Aug  1 10:00 some_source_file_{i}.rs\n"
        ));
        s.push_str(&format!(
            "drwxr-xr-x  2 user  staff    64 Aug  1 10:00 dir{i}\n"
        ));
    }
    s.push_str("-rw-r--r--  1 user  staff  1024 Aug  1 10:00 node_modules\n");
    s
}

fn tree_fixture() -> String {
    let mut s = String::new();
    for i in 0..300 {
        s.push_str(&format!("├── folder{i}\n│   ├── file{i}.rs\n"));
    }
    s.push_str("\n300 directories, 600 files\n");
    s
}

fn dedup_fixture() -> String {
    let mut s = String::new();
    for i in 0..200 {
        s.push_str(&format!("INFO request {i} started\n"));
        s.push_str(&big("DEBUG polling queue empty\n", 20));
    }
    s
}

fn numbered_fixture() -> String {
    let mut s = String::new();
    for i in 1..=400 {
        s.push_str(&format!(
            "  {i}|fn some_function_{i}() {{ do_something_with_a_long_name({i}); }}\n"
        ));
    }
    s
}

fn search_list_fixture() -> String {
    let mut s = String::from("Result of search in '**/*.tsx' (total 200 files):\n");
    for d in 0..10 {
        for f in 0..20 {
            s.push_str(&format!("- src/components/dir{d}/file{f}.tsx\n"));
        }
    }
    s
}

fn plain_blob_fixture() -> String {
    let mut s = String::new();
    for i in 0..400 {
        s.push_str(&format!(
            "This is plain prose line {i} with no structure whatsoever, just words.\n"
        ));
    }
    s
}

// ---------------------------------------------------------------- gates

#[test]
fn gate_tiny_input_unchanged() {
    let small = "diff --git a/x b/x\n@@ -1 +1 @@\n-a\n+b\n";
    let r = rtk::compress(small);
    assert!(r.filter.is_none());
    assert_eq!(r.text, small);
}

#[test]
fn gate_never_grow_never_empty() {
    // incompressible blob ≥500B, <250 lines, no duplicates → no filter fires
    let s = big("x", 600);
    let r = rtk::compress(&s);
    assert_eq!(r.text, s);
}

// ---------------------------------------------------------------- filters

#[test]
fn git_diff() {
    let f = git_diff_fixture();
    let (name, _) = autodetect::auto_detect(&f).unwrap();
    assert_eq!(name, "git-diff");
    let out = filters::git_diff::git_diff(&f);
    assert!(pct(&f, &out) >= 30.0, "{}%", pct(&f, &out));
}

#[test]
fn git_status() {
    let f = git_status_fixture();
    let (name, _) = autodetect::auto_detect(&f).unwrap();
    assert_eq!(name, "git-status");
    let out = filters::git_status::git_status(&f);
    assert!(pct(&f, &out) >= 30.0, "{}%", pct(&f, &out));
}

#[test]
fn git_log() {
    let f = git_log_fixture();
    let (name, _) = autodetect::auto_detect(&f).unwrap();
    assert_eq!(name, "git-log");
    let out = filters::git_log::git_log(&f);
    assert!(pct(&f, &out) >= 30.0, "{}%", pct(&f, &out));
}

#[test]
fn build_output() {
    let f = build_fixture();
    let (name, _) = autodetect::auto_detect(&f).unwrap();
    assert_eq!(name, "build-output");
    let out = filters::build_output::build_output(&f);
    assert!(pct(&f, &out) >= 30.0, "{}%", pct(&f, &out));
    assert!(out.contains("error[E0308]"));
    assert!(out.contains("Compiled 300 packages"));
}

#[test]
fn grep() {
    let f = grep_fixture();
    let (name, _) = autodetect::auto_detect(&f).unwrap();
    assert_eq!(name, "grep");
    let out = filters::grep::grep(&f);
    assert!(pct(&f, &out) >= 30.0, "{}%", pct(&f, &out));
    assert!(out.contains("300 matches in 10F"));
}

#[test]
fn find() {
    let f = find_fixture();
    let (name, _) = autodetect::auto_detect(&f).unwrap();
    assert_eq!(name, "find");
    let out = filters::find::find(&f);
    assert!(pct(&f, &out) >= 30.0, "{}%", pct(&f, &out));
}

#[test]
fn ls() {
    let f = ls_fixture();
    let (name, _) = autodetect::auto_detect(&f).unwrap();
    assert_eq!(name, "ls");
    let out = filters::ls::ls(&f);
    assert!(pct(&f, &out) >= 30.0, "{}%", pct(&f, &out));
    assert!(!out.contains("node_modules"));
    assert!(out.contains("Summary:"));
}

#[test]
fn tree() {
    let f = tree_fixture();
    let (name, _) = autodetect::auto_detect(&f).unwrap();
    assert_eq!(name, "tree");
    let out = filters::tree::tree(&f);
    assert!(pct(&f, &out) >= 30.0, "{}%", pct(&f, &out));
    assert!(!out.contains("directories"));
}

#[test]
fn dedup_log() {
    let f = dedup_fixture();
    let (name, _) = autodetect::auto_detect(&f).unwrap();
    assert_eq!(name, "dedup-log");
    let out = filters::dedup_log::dedup_log(&f);
    assert!(pct(&f, &out) >= 30.0, "{}%", pct(&f, &out));
    assert!(out.contains("duplicate lines"));
}

#[test]
fn read_numbered() {
    // direct filter: realistic numbered dump
    let f = numbered_fixture();
    let out = filters::read_numbered::read_numbered(&f);
    assert!(pct(&f, &out) >= 30.0, "{}%", pct(&f, &out));
    assert!(out.contains("file continues"));
    // autodetect: fires only when ≥250 lines fit in the 1024-char window
    // (reference behaviour — same gate in autodetect.js)
    let dense = "1|x
"
    .repeat(300);
    let (name, _) = autodetect::auto_detect(&dense).unwrap();
    assert_eq!(name, "read-numbered");
}

#[test]
fn search_list() {
    let f = search_list_fixture();
    let (name, _) = autodetect::auto_detect(&f).unwrap();
    assert_eq!(name, "search-list");
    let out = filters::search_list::search_list(&f);
    assert!(pct(&f, &out) >= 30.0, "{}%", pct(&f, &out));
}

#[test]
fn smart_truncate_fallback() {
    // ≥250 lines, no structure → dedup-log first if ≥5 non-empty (it is),
    // so exercise smart_truncate directly + via explicit resolve
    let f = plain_blob_fixture();
    let out = filters::smart_truncate::smart_truncate(&f);
    assert!(pct(&f, &out) >= 30.0);
    assert!(out.contains("lines truncated"));
    let (_, func) = rtk::resolve_filter("smart-truncate").unwrap();
    assert_eq!(func("short"), "short");
}

// ---------------------------------------------------------------- body pass

#[test]
fn compress_messages_tool_shapes() {
    let diff = git_diff_fixture();
    let mut body = serde_json::json!({
        "messages": [
            {"role": "user", "content": "fix it"},
            {"role": "tool", "content": diff},
        ]
    });
    let stats = rtk::compress_messages(&mut body).unwrap();
    assert!(stats.saved() > 0);
    let out = body["messages"][1]["content"].as_str().unwrap();
    assert!(out.len() < diff.len() / 2);
}

#[test]
fn compress_messages_skips_errors() {
    let diff = git_diff_fixture();
    let mut body = serde_json::json!({
        "messages": [
            {"role": "user", "content": [
                {"type": "tool_result", "is_error": true, "content": diff},
                {"type": "tool_result", "content": diff},
            ]},
        ]
    });
    let stats = rtk::compress_messages(&mut body).unwrap();
    assert!(stats.saved() > 0);
    let blocks = body["messages"][0]["content"].as_array().unwrap();
    assert_eq!(blocks[0]["content"].as_str().unwrap(), diff); // error untouched
    assert!(blocks[1]["content"].as_str().unwrap().len() < diff.len() / 2);
}
