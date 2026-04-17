//! Fetch engine shared by all source modules.
//!
//! Contract:
//!
//! 1. Each source returns a [`FetchPlan`] — a list of [`FetchStep`]s.
//! 2. For every step, the engine: downloads to a temp file, computes SHA256,
//!    verifies against the pin (or records on first run), atomically renames
//!    into place.
//! 3. When every step succeeds, a `MANIFEST.json` is written to the source
//!    directory with the provenance summary.

use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::hash::{sha256_file, verify_sha256};
use crate::http::HttpClient;
use crate::manifest::{Manifest, ManifestEntry};
use crate::source::{Pin, SourceId, SourceSpec};
use crate::{IngestError, Result};

/// What the fetch engine must do with an external artefact.
#[derive(Debug, Clone)]
pub struct FetchStep {
    pub url: String,
    /// Target path relative to the source directory (`data/<id>/`).
    pub relative_path: PathBuf,
    /// Expected SHA256 from the pin, or None to record on first run.
    pub expected_sha256: Option<String>,
    pub extract: ExtractMode,
    /// Human-readable name used in logs (e.g. "reactions.tsv").
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractMode {
    /// Save bytes as-is.
    Raw,
    /// Gunzip into `relative_path` (target name drops `.gz`).
    Gzip,
    /// Untar archive into `relative_path` (target is a directory).
    TarGz,
}

impl std::fmt::Display for ExtractMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ExtractMode::Raw => "raw",
            ExtractMode::Gzip => "gzip",
            ExtractMode::TarGz => "tar.gz",
        })
    }
}

/// Full plan for one source.
#[derive(Debug, Clone)]
pub struct FetchPlan {
    pub source: SourceId,
    pub version_label: String,
    pub steps: Vec<FetchStep>,
}

/// Runtime context shared across a fetch invocation. The flags each have
/// distinct semantics (CLI gates); grouping them into an enum would not
/// simplify call sites.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug)]
pub struct FetchContext {
    pub http: HttpClient,
    pub data_root: PathBuf,
    pub dry_run: bool,
    pub verify_only: bool,
    pub accept_first_run: bool,
    pub kegg_acknowledged: bool,
}

impl FetchContext {
    #[must_use]
    pub fn source_dir(&self, id: SourceId) -> PathBuf {
        SourceSpec::source_dir(&self.data_root, id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PinStatus {
    /// Pin present and hash verified.
    Verified,
    /// Pin present but `SOURCE.toml` has no recorded hash. First run:
    /// the computed hash is printed for the maintainer to commit.
    RecordedFirstHash { sha256: String },
    /// Pin absent; only allowed with `--accept-first-run`.
    Unpinned { sha256: String, pin_value: String },
}

#[derive(Debug)]
pub struct FetchOutcome {
    pub source: SourceId,
    pub steps: Vec<StepOutcome>,
    pub manifest: Option<Manifest>,
    pub pin_status: PinStatus,
}

#[derive(Debug)]
pub struct StepOutcome {
    pub label: String,
    pub url: String,
    pub target: PathBuf,
    pub sha256: String,
    pub bytes_written: bool,
}

/// Render a plan as a human-readable string (for dry-run).
#[must_use]
pub fn render_plan(plan: &FetchPlan, spec: &SourceSpec, ctx: &FetchContext) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let source_dir = ctx.source_dir(plan.source);
    let pin = spec.pin().map_or_else(
        || "UNPINNED".to_string(),
        |p| format!("{}={}", p.kind(), p.value()),
    );
    let hash = spec.pinned_hash().unwrap_or("(none)");
    let _ = writeln!(out, "source   : {}", plan.source);
    let _ = writeln!(out, "pin      : {pin}");
    let _ = writeln!(out, "sha256   : {hash}");
    let _ = writeln!(out, "version  : {}", plan.version_label);
    let _ = writeln!(out, "dir      : {}", source_dir.display());
    let _ = writeln!(out, "steps    :");
    for s in &plan.steps {
        let _ = writeln!(
            out,
            "  - {} ({}) → {}\n    url: {}",
            s.label,
            s.extract,
            s.relative_path.display(),
            s.url,
        );
    }
    out
}

/// Execute a plan for one source.
pub async fn execute(
    plan: FetchPlan,
    spec: &SourceSpec,
    ctx: &FetchContext,
) -> Result<FetchOutcome> {
    let source_dir = ctx.source_dir(plan.source);
    std::fs::create_dir_all(&source_dir)?;

    let mut step_outcomes = Vec::with_capacity(plan.steps.len());
    let mut any_bytes_written = false;

    for step in &plan.steps {
        let target = source_dir.join(&step.relative_path);
        let (sha, bytes_written) = run_step(step, &target, ctx).await?;
        any_bytes_written |= bytes_written;

        if let Some(expected) = step.expected_sha256.as_deref() {
            verify_sha256(&step.url, expected, &sha)?;
        }

        step_outcomes.push(StepOutcome {
            label: step.label.clone(),
            url: step.url.clone(),
            target,
            sha256: sha,
            bytes_written,
        });
    }

    // Determine PinStatus at the plan level.
    let pin_status = compute_pin_status(spec, &step_outcomes, ctx)?;

    let manifest = if ctx.verify_only || !any_bytes_written {
        None
    } else {
        let (primary_url, primary_sha) = step_outcomes
            .first()
            .map(|s| (s.url.clone(), s.sha256.clone()))
            .unwrap_or_default();
        let m = Manifest {
            source: spec.name.clone(),
            version: plan.version_label.clone(),
            retrieved_at: Utc::now(),
            sha256: primary_sha,
            url: primary_url,
            extra_files: step_outcomes
                .iter()
                .skip(1)
                .map(|s| ManifestEntry {
                    relative_path: s
                        .target
                        .strip_prefix(&source_dir)
                        .unwrap_or(&s.target)
                        .to_string_lossy()
                        .into_owned(),
                    sha256: s.sha256.clone(),
                    url: s.url.clone(),
                })
                .collect(),
        };
        m.write(&source_dir)?;
        Some(m)
    };

    Ok(FetchOutcome {
        source: plan.source,
        steps: step_outcomes,
        manifest,
        pin_status,
    })
}

async fn run_step(step: &FetchStep, target: &Path, ctx: &FetchContext) -> Result<(String, bool)> {
    if ctx.verify_only {
        if !target.exists() {
            return Err(IngestError::Other(format!(
                "verify-only: missing local artefact {}",
                target.display()
            )));
        }
        let sha = sha256_file(target)?;
        return Ok((sha, false));
    }

    let source_dir = target
        .parent()
        .ok_or_else(|| IngestError::Other(format!("target {} has no parent", target.display())))?;
    std::fs::create_dir_all(source_dir)?;

    let download_to = match step.extract {
        ExtractMode::Raw => target.to_path_buf(),
        ExtractMode::Gzip | ExtractMode::TarGz => source_dir.join(format!(
            ".tmp.{}",
            step.relative_path
                .file_name()
                .map_or_else(|| "artefact".into(), |s| s.to_string_lossy().into_owned())
        )),
    };

    let wrote = ctx.http.download(&step.url, &download_to).await?;

    // Extract if needed. Extraction is always relative to `source_dir`.
    match step.extract {
        ExtractMode::Raw => {}
        ExtractMode::Gzip => extract_gzip(&download_to, target)?,
        ExtractMode::TarGz => extract_tar_gz(&download_to, target)?,
    }

    let hashed_path = match step.extract {
        // For archives we hash the downloaded archive, not the extracted tree;
        // that's the thing `SOURCE.toml::sha256` actually pins.
        ExtractMode::Raw => target.to_path_buf(),
        ExtractMode::Gzip | ExtractMode::TarGz => download_to.clone(),
    };
    let sha = sha256_file(&hashed_path)?;

    if matches!(step.extract, ExtractMode::Gzip | ExtractMode::TarGz) && !ctx.verify_only {
        // Keep the archive for reproducibility (hash matches MANIFEST). It's
        // cheap to re-extract and expensive to re-download.
    }

    Ok((sha, wrote))
}

fn extract_gzip(src: &Path, dest: &Path) -> Result<()> {
    use std::fs::File;
    use std::io::copy;
    let f = File::open(src)?;
    let mut decoder = flate2::read::GzDecoder::new(f);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut out = File::create(dest)?;
    copy(&mut decoder, &mut out)?;
    Ok(())
}

fn extract_tar_gz(src: &Path, dest_dir: &Path) -> Result<()> {
    use std::fs::File;
    let f = File::open(src)?;
    let dec = flate2::read::GzDecoder::new(f);
    let mut archive = tar::Archive::new(dec);
    std::fs::create_dir_all(dest_dir)?;
    archive.unpack(dest_dir)?;
    Ok(())
}

fn compute_pin_status(
    spec: &SourceSpec,
    outcomes: &[StepOutcome],
    ctx: &FetchContext,
) -> Result<PinStatus> {
    let primary_sha = outcomes
        .first()
        .map(|s| s.sha256.clone())
        .unwrap_or_default();
    if let Some(pin) = spec.pin() {
        if spec.pinned_hash().is_some() {
            Ok(PinStatus::Verified)
        } else {
            warn!(
                source = %spec.name,
                pin = pin.value(),
                sha256 = %primary_sha,
                "no sha256 in SOURCE.toml — record this hash to pin the source"
            );
            Ok(PinStatus::RecordedFirstHash {
                sha256: primary_sha,
            })
        }
    } else {
        if !ctx.accept_first_run {
            return Err(IngestError::UnpinnedSource(spec.name.clone()));
        }
        info!(source = %spec.name, "--accept-first-run: recording pin");
        Ok(PinStatus::Unpinned {
            sha256: primary_sha,
            pin_value: "(record me in SOURCE.toml)".into(),
        })
    }
}

#[must_use]
pub fn format_pin(pin: Option<&Pin>) -> String {
    pin.map_or_else(
        || "UNPINNED".to_string(),
        |p| format!("{}={}", p.kind(), p.value()),
    )
}
