//! Compress build tool output: keep errors/warnings/summary, strip progress.
//! Port of filters/buildOutput.js.

use regex::Regex;

const DEPRECATION_KEEP: usize = 3;

fn yes(re: &Regex, s: &str) -> bool {
    re.is_match(s)
}

pub fn build_output(input: &str) -> String {
    let re_cargo_cont = Regex::new(r"^\s*(-->|\||\d+\s*\||=)").unwrap();
    let re_npm_err = Regex::new(r"(?i)^npm (ERR!|error)").unwrap();
    let re_yarn_err = Regex::new(r"(?i)^yarn error").unwrap();
    let re_npm_dep = Regex::new(r"(?i)^npm warn deprecated").unwrap();
    let re_npm_warn = Regex::new(r"(?i)^npm warn").unwrap();
    let re_yarn_warn = Regex::new(r"(?i)^yarn warn").unwrap();
    let re_error = Regex::new(r"(?i)^error(\[|:)").unwrap();
    let re_warning = Regex::new(r"(?i)^warning(\[|:)").unwrap();
    let re_error_colon = Regex::new(r"(?i)^ERROR:").unwrap();
    let re_bracket_err = Regex::new(r"(?i)^\[ERROR\]").unwrap();
    let re_build_failed = Regex::new(r"(?i)^BUILD FAILED").unwrap();
    let re_bracket_warn = Regex::new(r"(?i)^\[WARNING\]").unwrap();
    let re_compiling = Regex::new(r"(?i)^\s*Compiling\s+\S+").unwrap();
    let re_downloading = Regex::new(r"(?i)^\s*Downloading\s+\S+").unwrap();
    let re_fetching = Regex::new(r"(?i)^Fetching\s+").unwrap();
    let re_summary = Regex::new(r"(?i)(^(added|removed|changed|audited|installed)\s+\d+\s+package|^\s*Finished\s+|^BUILD SUCCESS|^\d+\s+(vulnerabilities|packages?|warnings?|errors?)|^Successfully (installed|built)|^To address .* issues|^Run `npm (audit|fund)`|packages are looking for funding)").unwrap();

    let mut errors: Vec<&str> = vec![];
    let mut warnings: Vec<&str> = vec![];
    let mut deprecations: Vec<&str> = vec![];
    let mut summary: Option<String> = None;
    let mut compiling = 0usize;
    let mut downloading = 0usize;
    let mut in_cargo_error = false;

    for line in input.lines() {
        let trimmed = line.trim();

        if in_cargo_error {
            if trimmed.is_empty() {
                in_cargo_error = false;
                continue;
            }
            if yes(&re_cargo_cont, line) {
                errors.push(line);
                continue;
            }
            in_cargo_error = false;
        }
        if trimmed.is_empty() {
            continue;
        }
        if yes(&re_npm_err, trimmed) || yes(&re_yarn_err, trimmed) {
            errors.push(line);
            continue;
        }
        if yes(&re_npm_dep, trimmed) {
            deprecations.push(line);
            continue;
        }
        if yes(&re_npm_warn, trimmed) || yes(&re_yarn_warn, trimmed) {
            warnings.push(line);
            continue;
        }
        if yes(&re_error, trimmed) || trimmed.starts_with("error -->") {
            errors.push(line);
            in_cargo_error = true;
            continue;
        }
        if yes(&re_warning, trimmed) || trimmed.starts_with("warning -->") {
            warnings.push(line);
            in_cargo_error = true;
            continue;
        }
        if yes(&re_error_colon, trimmed) {
            errors.push(line);
            continue;
        }
        if yes(&re_bracket_err, trimmed) || yes(&re_build_failed, trimmed) {
            errors.push(line);
            continue;
        }
        if yes(&re_bracket_warn, trimmed) {
            warnings.push(line);
            continue;
        }
        if yes(&re_compiling, trimmed) {
            compiling += 1;
            continue;
        }
        if yes(&re_downloading, trimmed) || yes(&re_fetching, trimmed) {
            downloading += 1;
            continue;
        }
        if yes(&re_summary, trimmed) {
            summary = match summary {
                Some(s) => Some(format!("{s}\n{line}")),
                None => Some(line.to_string()),
            };
        }
    }

    let mut out = String::new();
    for d in deprecations.iter().take(DEPRECATION_KEEP) {
        out += &format!("{d}\n");
    }
    if deprecations.len() > DEPRECATION_KEEP {
        out += &format!("... +{} more deprecated packages\n", deprecations.len() - DEPRECATION_KEEP);
    }
    if compiling > 0 {
        out += &format!("Compiled {compiling} packages\n");
    }
    if downloading > 0 {
        out += &format!("Downloaded {downloading} packages\n");
    }
    for e in &errors {
        out += &format!("{e}\n");
    }
    for w in warnings.iter().take(5) {
        out += &format!("{w}\n");
    }
    if warnings.len() > 5 {
        out += &format!("... +{} more warnings\n", warnings.len() - 5);
    }
    if let Some(s) = summary {
        out += &format!("{s}\n");
    }
    let trimmed = out.trim_end_matches('\n').to_string();
    if trimmed.is_empty() {
        input.to_string()
    } else {
        trimmed
    }
}
