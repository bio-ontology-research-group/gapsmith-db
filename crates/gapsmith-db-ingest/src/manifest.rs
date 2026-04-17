//! `MANIFEST.json` — provenance written after a successful fetch.

use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{IngestError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub source: String,
    pub version: String,
    pub retrieved_at: DateTime<Utc>,
    pub sha256: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_files: Vec<ManifestEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub relative_path: String,
    pub sha256: String,
    pub url: String,
}

impl Manifest {
    pub fn write(&self, source_dir: &Path) -> Result<()> {
        let path = source_dir.join("MANIFEST.json");
        let text = serde_json::to_string_pretty(self)
            .map_err(|e| IngestError::Other(format!("MANIFEST.json serialise: {e}")))?;
        std::fs::write(path, text)?;
        Ok(())
    }
}
