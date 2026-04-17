//! `gapsmith-db release` — build a signed-adjacent release tarball.
#![allow(
    clippy::needless_pass_by_value,
    clippy::manual_let_else,
    clippy::map_unwrap_or,
    clippy::unnecessary_wraps
)]
//!
//! Contents:
//!
//! ```text
//! gapsmith-db-<version>/
//!   README.md
//!   LICENSING.md
//!   CITATIONS.md
//!   db.gapsmith
//!   tsv/*.tsv
//!   tsv/stats.json
//!   MANIFEST.json    aggregated inputs
//!   RECEIPT.json     reproducibility receipt
//! ```
//!
//! A sidecar `*.tar.gz.sha256` is always written. Proper GPG signing is
//! left to the release pipeline (`gpg --detach-sign`) — we don't hard-
//! depend on a key here.

use std::io::Read;
use std::path::Path;

use anyhow::{Context, bail};
use chrono::Utc;
use gapsmith_db_core::{Database, serde_io};
use gapsmith_db_ingest::source::SourceId;
use gapsmith_db_propose::DecisionLog;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tracing::info;

use crate::ReleaseArgs;

pub fn run(args: ReleaseArgs) -> anyhow::Result<()> {
    let version = args
        .version
        .clone()
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
    let stem = format!("gapsmith-db-{version}");
    let workdir = tempfile_dir()?;
    let root = workdir.path().join(&stem);
    std::fs::create_dir_all(&root)?;

    // Copy documentation.
    copy_if_exists("README.md", &root)?;
    copy_if_exists("LICENSING.md", &root)?;

    // Citations: one line per source that was used in this build.
    let citations = build_citations(&args.data_root)?;
    std::fs::write(root.join("CITATIONS.md"), citations)?;

    // Binary DB + TSV tables.
    let db_dst = root.join("db.gapsmith");
    std::fs::copy(&args.db, &db_dst)
        .with_context(|| format!("copy {} -> {}", args.db.display(), db_dst.display()))?;
    copy_tree(&args.tsv_dir, &root.join("tsv"))?;

    // MANIFEST.json — aggregated source + db info.
    let manifest = build_manifest(&args.data_root, &args.db)?;
    std::fs::write(
        root.join("MANIFEST.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;

    // RECEIPT.json — everything you need to reproduce this build.
    let receipt = build_receipt(&args, &version, &manifest)?;
    std::fs::write(
        root.join("RECEIPT.json"),
        serde_json::to_string_pretty(&receipt)?,
    )?;

    // Build tarball.
    if let Some(parent) = args.out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tar_gz = std::fs::File::create(&args.out)?;
    let enc = flate2::write::GzEncoder::new(tar_gz, flate2::Compression::default());
    let mut tar = tar::Builder::new(enc);
    tar.append_dir_all(&stem, &root)?;
    tar.finish()?;

    // Sidecar sha256.
    let checksum = sha256_file(&args.out)?;
    let sha_path = args.out.with_extension("tar.gz.sha256");
    std::fs::write(&sha_path, format!("{checksum}  {}\n", args.out.display()))?;

    info!(
        path = %args.out.display(),
        sha256 = %checksum,
        "release tarball built"
    );
    println!("wrote {} (sha256 {})", args.out.display(), checksum);
    Ok(())
}

// --- helpers ---------------------------------------------------------------

fn tempfile_dir() -> anyhow::Result<tempfile::TempDir> {
    Ok(tempfile::tempdir()?)
}

fn copy_if_exists(path: &str, dst_dir: &Path) -> anyhow::Result<()> {
    let p = Path::new(path);
    if p.exists() {
        let dst = dst_dir.join(
            p.file_name()
                .ok_or_else(|| anyhow::anyhow!("bad path {path}"))?,
        );
        std::fs::copy(p, dst)?;
    }
    Ok(())
}

fn copy_tree(src: &Path, dst: &Path) -> anyhow::Result<()> {
    if !src.exists() {
        bail!("TSV dir {} missing", src.display());
    }
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let e = entry?;
        let p = e.path();
        if p.is_file() {
            std::fs::copy(&p, dst.join(e.file_name()))?;
        }
    }
    Ok(())
}

fn build_citations(data_root: &Path) -> anyhow::Result<String> {
    use std::fmt::Write;
    let mut out = String::new();
    writeln!(
        out,
        "# gapsmith-db release citations\n\n\
         Generated {}.\n\n\
         ## Upstream sources incorporated\n",
        Utc::now().to_rfc3339()
    )?;
    for id in SourceId::ALL {
        let spec = match gapsmith_db_ingest::source::SourceSpec::load(data_root, *id) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let pin = spec
            .pin()
            .map(|p| format!("{}={}", p.kind(), p.value()))
            .unwrap_or_else(|| "(no pin)".into());
        writeln!(out, "- **{}** ({}): {}", spec.name, pin, spec.attribution)?;
        writeln!(out, "  - Licence: {}", spec.licence)?;
        writeln!(out, "  - Upstream: {}", spec.upstream_url)?;
    }
    writeln!(
        out,
        "\n_Licence-clean by construction. See LICENSING.md for the \
         enforcement policy and the enumerated list of banned sources._"
    )?;
    Ok(out)
}

#[derive(Debug, Clone, Serialize)]
struct SourceRecord {
    id: String,
    name: String,
    pin_kind: String,
    pin_value: String,
    sha256: Option<String>,
    licence: String,
    manifest_retrieved_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ReleaseManifest {
    release_version: String,
    built_at: chrono::DateTime<chrono::Utc>,
    db_sha256: String,
    db_stats: gapsmith_db_core::DatabaseStats,
    sources: Vec<SourceRecord>,
}

fn build_manifest(data_root: &Path, db_path: &Path) -> anyhow::Result<ReleaseManifest> {
    let db: Database = serde_io::read_binary(db_path)?;
    let db_sha256 = sha256_file(db_path)?;
    let mut sources = Vec::new();
    for id in SourceId::ALL {
        let spec = match gapsmith_db_ingest::source::SourceSpec::load(data_root, *id) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let manifest_path = data_root.join(id.as_str()).join("MANIFEST.json");
        let retrieved_at = if manifest_path.exists() {
            let text = std::fs::read_to_string(&manifest_path).unwrap_or_default();
            let v: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
            v.get("retrieved_at")
                .and_then(|s| s.as_str())
                .map(str::to_string)
        } else {
            None
        };
        let pin = spec.pin();
        sources.push(SourceRecord {
            id: id.to_string(),
            name: spec.name.clone(),
            pin_kind: pin
                .as_ref()
                .map(|p| p.kind().to_string())
                .unwrap_or_default(),
            pin_value: pin
                .as_ref()
                .map(|p| p.value().to_string())
                .unwrap_or_default(),
            sha256: spec.sha256.clone(),
            licence: spec.licence.clone(),
            manifest_retrieved_at: retrieved_at,
        });
    }
    Ok(ReleaseManifest {
        release_version: env!("CARGO_PKG_VERSION").to_string(),
        built_at: Utc::now(),
        db_sha256,
        db_stats: db.stats(),
        sources,
    })
}

#[derive(Debug, Clone, Serialize)]
struct Receipt {
    release_version: String,
    built_at: chrono::DateTime<chrono::Utc>,
    gapsmith_db_commit: Option<String>,
    rust_toolchain: String,
    db_sha256: String,
    decision_chain_head: String,
    decision_count: usize,
    sources: Vec<SourceRecord>,
}

fn build_receipt(
    args: &ReleaseArgs,
    version: &str,
    manifest: &ReleaseManifest,
) -> anyhow::Result<Receipt> {
    let log = DecisionLog::at(&args.proposals_dir);
    let entries = log.read_all().unwrap_or_default();
    let head = log.head().unwrap_or_default();
    Ok(Receipt {
        release_version: version.to_string(),
        built_at: Utc::now(),
        gapsmith_db_commit: std::env::var("GIT_COMMIT").ok(),
        rust_toolchain: rustc_version_string(),
        db_sha256: manifest.db_sha256.clone(),
        decision_chain_head: head,
        decision_count: entries.len(),
        sources: manifest.sources.clone(),
    })
}

fn sha256_file(path: &Path) -> anyhow::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0_u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}

fn rustc_version_string() -> String {
    std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "rustc (unknown)".into())
}
