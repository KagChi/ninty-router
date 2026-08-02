//! Compact `ls -la` output: dirs/, files + human size, ext summary, noise skip.
//! Port of filters/ls.js.

use regex::Regex;

const LS_EXT_SUMMARY_TOP: usize = 5;
const LS_NOISE_DIRS: &[&str] = &[
    "node_modules", ".git", "target", "__pycache__", ".next", "dist", "build", ".cache",
    ".turbo", ".vercel", ".pytest_cache", ".mypy_cache", ".tox", ".venv", "venv", "env",
    "coverage", ".nyc_output", ".DS_Store", "Thumbs.db", ".idea", ".vscode", ".vs",
    "*.egg-info", ".eggs",
];

fn human_size(bytes: u64) -> String {
    if bytes >= 1_048_576 {
        format!("{:.1}M", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1}K", bytes as f64 / 1024.0)
    } else {
        format!("{bytes}B")
    }
}

struct Parsed {
    file_type: char,
    size: u64,
    name: String,
}

fn parse_ls_line(line: &str) -> Option<Parsed> {
    let re = Regex::new(r"\s+(Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)\s+\d{1,2}\s+(\d{4}|\d{2}:\d{2})\s+").unwrap();
    let m = re.find(line)?;
    let name = line[m.end()..].to_string();
    let before = &line[..m.start()];
    let parts: Vec<&str> = before.split_whitespace().collect();
    if parts.len() < 4 {
        return None;
    }
    let file_type = parts[0].chars().next()?;
    // rightmost integer token before the date
    let mut size = 0u64;
    for p in parts.iter().rev() {
        if p.chars().all(|c| c.is_ascii_digit()) && !p.is_empty() {
            if let Ok(n) = p.parse::<u64>() {
                size = n;
                break;
            }
        }
    }
    Some(Parsed { file_type, size, name })
}

pub fn ls(input: &str) -> String {
    let mut dirs: Vec<String> = vec![];
    let mut files: Vec<(String, String)> = vec![];
    let mut by_ext: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for line in input.lines() {
        if line.starts_with("total ") || line.is_empty() {
            continue;
        }
        let Some(p) = parse_ls_line(line) else { continue };
        if p.name == "." || p.name == ".." || LS_NOISE_DIRS.contains(&p.name.as_str()) {
            continue;
        }
        if p.file_type == 'd' {
            dirs.push(p.name);
        } else if p.file_type == '-' || p.file_type == 'l' {
            let ext = match p.name.rfind('.') {
                Some(i) if i > 0 => p.name[i..].to_string(),
                _ => "no ext".to_string(),
            };
            *by_ext.entry(ext).or_insert(0) += 1;
            files.push((p.name.clone(), human_size(p.size)));
        }
    }

    if dirs.is_empty() && files.is_empty() {
        return input.to_string();
    }
    let mut out = String::new();
    for d in &dirs {
        out += &format!("{d}/\n");
    }
    for (name, size) in &files {
        out += &format!("{name}  {size}\n");
    }
    let mut summary = format!("\nSummary: {} files, {} dirs", files.len(), dirs.len());
    if !by_ext.is_empty() {
        let mut exts: Vec<(String, usize)> = by_ext.into_iter().collect();
        exts.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
        let parts: Vec<String> = exts.iter().take(LS_EXT_SUMMARY_TOP).map(|(e, c)| format!("{c} {e}")).collect();
        summary += &format!(" ({}{})", parts.join(", "), if exts.len() > LS_EXT_SUMMARY_TOP {
            format!(", +{} more)", exts.len() - LS_EXT_SUMMARY_TOP)
        } else {
            ")".to_string()
        });
    }
    out + &summary
}
