//! Drop summary line + blanks, cap 200 lines. Port of filters/tree.js.

pub const TREE_MAX_LINES: usize = 200;

pub fn tree(input: &str) -> String {
    let mut filtered: Vec<&str> = vec![];
    for line in input.lines() {
        if line.contains("director") && line.contains("file") {
            continue;
        }
        if line.trim().is_empty() && filtered.is_empty() {
            continue;
        }
        filtered.push(line);
    }
    while filtered
        .last()
        .map(|l| l.trim().is_empty())
        .unwrap_or(false)
    {
        filtered.pop();
    }
    if filtered.len() > TREE_MAX_LINES {
        let cut = filtered.len() - TREE_MAX_LINES;
        return format!(
            "{}\n... +{cut} more lines",
            filtered[..TREE_MAX_LINES].join("\n")
        );
    }
    filtered.join("\n")
}
