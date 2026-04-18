//! Retrieval backend construction shared by `propose` and
//! `propose-catalogue`.
//!
//! Selects Qdrant when `--qdrant-url` is given, otherwise falls back to
//! an in-memory backend (empty or loaded from a JSON file for offline
//! pilots and CI).

use std::path::PathBuf;

use anyhow::Context;
use gapsmith_db_propose::retrieval::{
    InMemoryBackend, Passage, QdrantBackend, RetrievalBackend, SearchQuery,
};
use gapsmith_db_propose::{EmbedderConfig, QdrantConfig};

#[derive(clap::Args, Debug, Clone)]
pub struct RetrievalArgs {
    /// Qdrant base URL (e.g. http://localhost:6333). Omit to use in-memory.
    #[arg(long)]
    pub qdrant_url: Option<String>,

    /// Qdrant collection name.
    #[arg(long, default_value = "gapsmith")]
    pub qdrant_collection: String,

    /// Env var holding the Qdrant API key (empty string disables).
    #[arg(long, default_value = "QDRANT_API_KEY")]
    pub qdrant_api_key_env: String,

    /// Embedder model ID (sentence-transformers compatible).
    #[arg(long, default_value = "NeuML/pubmedbert-base-embeddings")]
    pub embedder_model: String,

    /// Python bridge project directory (contains pyproject.toml).
    #[arg(long, default_value = "python")]
    pub python_project: PathBuf,

    /// JSON file of passages — used when --qdrant-url is not set.
    #[arg(long)]
    pub passages: Option<PathBuf>,

    /// Top-k retrieval hits to inline into the prompt.
    #[arg(long, default_value_t = 8)]
    pub top_k: usize,
}

#[derive(Clone)]
pub enum Retrieval {
    Memory(InMemoryBackend),
    Qdrant(QdrantBackend),
}

impl RetrievalBackend for Retrieval {
    fn search(&self, q: &SearchQuery) -> gapsmith_db_propose::Result<Vec<Passage>> {
        match self {
            Self::Memory(b) => b.search(q),
            Self::Qdrant(b) => b.search(q),
        }
    }
}

pub fn build(args: &RetrievalArgs) -> anyhow::Result<Retrieval> {
    if let Some(url) = &args.qdrant_url {
        let api_key = if args.qdrant_api_key_env.is_empty() {
            None
        } else {
            std::env::var(&args.qdrant_api_key_env).ok()
        };
        let config = QdrantConfig {
            url: url.clone(),
            collection: args.qdrant_collection.clone(),
            api_key,
            embedder: EmbedderConfig {
                model: args.embedder_model.clone(),
                python_project: args.python_project.clone(),
                use_uv: true,
            },
            timeout_secs: 60,
        };
        return Ok(Retrieval::Qdrant(QdrantBackend::new(config)));
    }
    let passages: Vec<Passage> = if let Some(path) = &args.passages {
        let bytes = std::fs::read(path)
            .with_context(|| format!("reading passages from {}", path.display()))?;
        serde_json::from_slice(&bytes)?
    } else {
        Vec::new()
    };
    Ok(Retrieval::Memory(InMemoryBackend::new(passages)))
}
