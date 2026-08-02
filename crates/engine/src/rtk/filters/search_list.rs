//! Compact Cursor Glob "Result of search..." lists. Port of filters/searchList.js.

use regex::Regex;
use std::collections::BTreeMap;

const PER_DIR_MAX: usize = 10;
const TOTAL_DIR_MAX: usize = 20;

thread_local! {
    pub static HEADER_RE: Regex =
        Regex::new(r"^Result of search in '[^']*' \(total (\d+) files?\):").unwrap();
}

pub fn search_list(input: &str) -> String {
    let mut lines = input.lines();
    let Some(header) = lines.next() else {
        return input.to_string();
    };

    let mut paths: Vec<&str> = vec![];
    for raw in lines {
        let t = raw.trim();
        if let Some(p) = t.strip_prefix("- ") {
            paths.push(p);
        }
    }
    if paths.is_empty() {
        return input.to_string();
    }

    let mut by_dir: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for p in &paths {
        let (dir, name) = match p.rfind('/') {
            None => (".".to_string(), p.to_string()),
            Some(i) => {
                let d = &p[..i];
                (
                    if d.is_empty() {
                        "/".into()
                    } else {
                        d.to_string()
                    },
                    p[i + 1..].to_string(),
                )
            }
        };
        by_dir.entry(dir).or_default().push(name);
    }

    let dirs: Vec<&String> = by_dir.keys().collect();
    let mut out = format!(
        "{header}\n{} files in {} dirs:\n\n",
        paths.len(),
        dirs.len()
    );
    for dir in dirs.iter().take(TOTAL_DIR_MAX) {
        let names = &by_dir[*dir];
        out += &format!("{dir}/ ({}):\n", names.len());
        for n in names.iter().take(PER_DIR_MAX) {
            out += &format!("  {n}\n");
        }
        if names.len() > PER_DIR_MAX {
            out += &format!("  +{}\n", names.len() - PER_DIR_MAX);
        }
        out += "\n";
    }
    if dirs.len() > TOTAL_DIR_MAX {
        out += &format!("+{} more dirs\n", dirs.len() - TOTAL_DIR_MAX);
    }
    out.trim_end_matches('\n').to_string()
}
