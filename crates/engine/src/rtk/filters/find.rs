//! Group paths by parent dir, cap 10/dir + 20 dirs. Port of filters/find.js.

use std::collections::BTreeMap;

pub const FIND_PER_DIR_MAX: usize = 10;
pub const FIND_TOTAL_DIR_MAX: usize = 20;

pub fn find(input: &str) -> String {
    let lines: Vec<&str> = input.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return input.to_string();
    }

    let mut by_dir: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for path in &lines {
        let last_sep = path.rfind('/').into_iter().chain(path.rfind('\\')).max();
        let (dir, base) = match last_sep {
            None => (".".to_string(), path.to_string()),
            Some(i) => {
                let d = &path[..i];
                (
                    if d.is_empty() {
                        "/".to_string()
                    } else {
                        d.to_string()
                    },
                    path[i + 1..].to_string(),
                )
            }
        };
        by_dir.entry(dir).or_default().push(base);
    }

    let dirs: Vec<&String> = by_dir.keys().collect();
    let mut out = format!("{} files in {} dirs:\n\n", lines.len(), dirs.len());
    for dir in dirs.iter().take(FIND_TOTAL_DIR_MAX) {
        let files = &by_dir[*dir];
        let label = dir.replace('\\', "/");
        out += &format!("{label}/  ({})\n", files.len());
        for f in files.iter().take(FIND_PER_DIR_MAX) {
            out += &format!("  {f}\n");
        }
        if files.len() > FIND_PER_DIR_MAX {
            out += &format!("  +{}\n", files.len() - FIND_PER_DIR_MAX);
        }
    }
    if dirs.len() > FIND_TOTAL_DIR_MAX {
        out += &format!("\n+{} more dirs\n", dirs.len() - FIND_TOTAL_DIR_MAX);
    }
    out
}
