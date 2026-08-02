//! Compact `git status` (long + porcelain). Port of filters/gitStatus.js.

use regex::Regex;

const STATUS_MAX_FILES: usize = 10;
const STATUS_MAX_UNTRACKED: usize = 10;

pub fn git_status(input: &str) -> String {
    let lines: Vec<&str> = input.lines().collect();
    if lines.is_empty() || (lines.len() == 1 && lines[0].trim().is_empty()) {
        return "Clean working tree".into();
    }

    let re_branch = Regex::new(r"^On branch (\S+)").unwrap();
    let re_porcelain = Regex::new(r"^[ MADRCU?!][ MADRCU?!] ").unwrap();
    let re_long =
        Regex::new(r"^\s*(modified|new file|deleted|renamed|both modified):\s+(.+)$").unwrap();

    let mut branch = String::new();
    let mut staged_files: Vec<String> = vec![];
    let mut modified_files: Vec<String> = vec![];
    let mut untracked_files: Vec<String> = vec![];
    let mut staged = 0usize;
    let mut modified = 0usize;
    let mut untracked = 0usize;
    let mut conflicts = 0usize;

    for raw in lines {
        if raw.trim().is_empty() {
            continue;
        }
        if let Some(m) = re_branch.captures(raw) {
            branch = m[1].to_string();
            continue;
        }
        if let Some(stripped) = raw.strip_prefix("##") {
            branch = stripped.trim().to_string();
            continue;
        }
        if raw.len() >= 3 && re_porcelain.is_match(raw) {
            let b = raw.as_bytes();
            let (x, y) = (b[0] as char, b[1] as char);
            let file = &raw[3..];
            if &raw[..2] == "??" {
                untracked += 1;
                untracked_files.push(file.into());
                continue;
            }
            if "MADRC".contains(x) {
                staged += 1;
                staged_files.push(file.into());
            } else if x == 'U' {
                conflicts += 1;
            }
            if y == 'M' || y == 'D' {
                modified += 1;
                modified_files.push(file.into());
            }
            continue;
        }
        if let Some(m) = re_long.captures(raw) {
            let kind = &m[1];
            let path = m[2].trim().to_string();
            match kind {
                "both modified" => conflicts += 1,
                "modified" | "deleted" => {
                    modified += 1;
                    modified_files.push(path);
                }
                _ => {
                    staged += 1;
                    staged_files.push(path);
                }
            }
        }
    }

    let mut out = String::new();
    if !branch.is_empty() {
        out += &format!("* {branch}\n");
    }
    let section = |out: &mut String, label: &str, count: usize, files: &[String], cap: usize| {
        if count > 0 {
            *out += &format!("{label}: {count} files\n");
            for f in files.iter().take(cap) {
                *out += &format!("   {f}\n");
            }
            if files.len() > cap {
                *out += &format!("   ... +{} more\n", files.len() - cap);
            }
        }
    };
    section(
        &mut out,
        "+ Staged",
        staged,
        &staged_files,
        STATUS_MAX_FILES,
    );
    section(
        &mut out,
        "~ Modified",
        modified,
        &modified_files,
        STATUS_MAX_FILES,
    );
    section(
        &mut out,
        "? Untracked",
        untracked,
        &untracked_files,
        STATUS_MAX_UNTRACKED,
    );
    if conflicts > 0 {
        out += &format!("conflicts: {conflicts} files\n");
    }
    if staged == 0 && modified == 0 && untracked == 0 && conflicts == 0 {
        out += "clean — nothing to commit\n";
    }
    out.trim_end_matches('\n').to_string()
}
