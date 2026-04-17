//! Licence-lint as a real cargo test.
//!
//! Fails if the two forbidden source names (constructed at runtime to keep
//! this file itself lint-clean) appear in any committed file. Fetched
//! upstream artefacts under `data/<source>/` are gitignored and excluded —
//! they may contain xrefs to banned sources as ID strings (e.g. "XYZ:RXN"
//! in gapseq's MNXref cross-reference tables), but since we do not
//! redistribute those files, the exclusion is policy-safe. Only what is
//! committed is what we ship.
//!
//! Matches the `just licence-lint` target but runs under `cargo test`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
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
// occurrences of the two names are forbidden in committed code/data/prompts.
fn needles() -> [String; 2] {
    [format!("{}cyc", "meta"), format!("{}cyc", "bio")]
}

/// Enumerate committed files via `git ls-files`. Anything outside the
/// index — fetched upstream artefacts, proposals, caches — is out of scope.
fn tracked_files(root: &PathBuf) -> Vec<PathBuf> {
    let out = Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(root)
        .output()
        .expect("git ls-files failed — run the test inside a git worktree");
    assert!(out.status.success(), "git ls-files exited non-zero");
    out.stdout
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| root.join(std::str::from_utf8(s).expect("non-utf8 path")))
        .collect()
}

fn should_scan_file(p: &std::path::Path) -> bool {
    let s = p.to_string_lossy();
    // Code, data pins, prompts, corpus — per LICENSING.md. Markdown is
    // deliberately excluded: LICENSING.md itself names the banned sources.
    let code = s.ends_with(".rs") || s.ends_with(".py");
    let data_pin = s.ends_with("SOURCE.toml")
        || (s.contains("/data/") && (s.ends_with(".tsv") || s.ends_with(".json")));
    let prompt = s.contains("/prompts/") || s.contains("/corpus/");
    code || data_pin || prompt
}

#[test]
fn no_forbidden_sources_in_code_data_or_prompts() {
    let root = workspace_root();
    let needles = needles();
    let mut hits = Vec::new();
    for path in tracked_files(&root) {
        if !should_scan_file(&path) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for (i, line) in text.lines().enumerate() {
            let low = line.to_ascii_lowercase();
            if needles.iter().any(|n| low.contains(n)) {
                hits.push((path.clone(), i + 1, line.to_string()));
            }
        }
    }
    if !hits.is_empty() {
        for (p, line, content) in &hits {
            eprintln!("{}:{line}: {content}", p.display());
        }
        panic!("licence-lint: {} forbidden reference(s) found", hits.len());
    }
}
