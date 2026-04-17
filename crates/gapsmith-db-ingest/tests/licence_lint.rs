//! Licence-lint as a real cargo test.
//!
//! Fails if the two forbidden source names (constructed at runtime to keep
//! this file itself lint-clean) appear in any code, data pin, or prompt
//! path inside the workspace. Documentation is allowed to discuss the rule.
//! Matches the `just licence-lint` target but runs in `cargo test`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    // tests run from the crate dir; walk up to the workspace Cargo.toml.
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        if p.join("Cargo.toml").exists() && p.join("crates").is_dir() {
            return p;
        }
        assert!(
            p.pop(),
            "could not find workspace root from {}",
            p.display()
        );
    }
}

// Needles constructed so this file itself does not match. Literal
// occurrences of the two names are forbidden in code/data/prompts.
fn needles() -> [String; 2] {
    [format!("{}cyc", "meta"), format!("{}cyc", "bio")]
}

fn scan(path: &Path, hits: &mut Vec<(PathBuf, usize, String)>, needles: &[String]) {
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if p.is_dir() {
            if matches!(
                name,
                "target" | ".git" | "node_modules" | ".venv" | "__pycache__"
            ) {
                continue;
            }
            scan(&p, hits, needles);
        } else if should_scan_file(&p)
            && let Ok(text) = std::fs::read_to_string(&p)
        {
            for (i, line) in text.lines().enumerate() {
                let low = line.to_ascii_lowercase();
                if needles.iter().any(|n| low.contains(n)) {
                    hits.push((p.clone(), i + 1, line.to_string()));
                }
            }
        }
    }
}

fn should_scan_file(p: &Path) -> bool {
    let s = p.to_string_lossy();
    // Code, data pins, prompts, corpus — per LICENSING.md.
    let code = s.ends_with(".rs") || s.ends_with(".py");
    let data_pin = s.ends_with("/SOURCE.toml")
        || (s.contains("/data/") && (s.ends_with(".tsv") || s.ends_with(".json")));
    let prompt = s.contains("/prompts/") || s.contains("/corpus/");
    code || data_pin || prompt
}

#[test]
fn no_forbidden_sources_in_code_data_or_prompts() {
    let root = workspace_root();
    let needles = needles();
    let mut hits = Vec::new();
    scan(&root, &mut hits, &needles);
    if !hits.is_empty() {
        for (p, line, content) in &hits {
            eprintln!("{}:{line}: {content}", p.display());
        }
        panic!("licence-lint: {} forbidden reference(s) found", hits.len());
    }
}
