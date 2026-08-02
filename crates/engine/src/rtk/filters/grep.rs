//! Group "file:lineno:content" matches per file, cap 10/file.
//! Port of filters/grep.js.

use std::collections::BTreeMap;

pub const GREP_PER_FILE_MAX: usize = 10;

pub fn grep(input: &str) -> String {
    let mut by_file: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    let mut total = 0usize;

    for line in input.lines() {
        let Some(first) = line.find(':') else { continue };
        let Some(second_rel) = line[first + 1..].find(':') else {
            continue;
        };
        let second = first + 1 + second_rel;
        let file = &line[..first];
        let lineno = &line[first + 1..second];
        let content = &line[second + 1..];
        if lineno.is_empty() || !lineno.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        total += 1;
        by_file
            .entry(file.to_string())
            .or_default()
            .push((lineno.to_string(), content.to_string()));
    }
    if total == 0 {
        return input.to_string();
    }

    let mut out = format!("{total} matches in {}F:\n\n", by_file.len());
    for (file, matches) in &by_file {
        out += &format!("[file] {file} ({}):\n", matches.len());
        for (lineno, content) in matches.iter().take(GREP_PER_FILE_MAX) {
            out += &format!("  {lineno:>4}: {}\n", content.trim());
        }
        if matches.len() > GREP_PER_FILE_MAX {
            out += &format!("  +{}\n", matches.len() - GREP_PER_FILE_MAX);
        }
        out += "\n";
    }
    out
}
