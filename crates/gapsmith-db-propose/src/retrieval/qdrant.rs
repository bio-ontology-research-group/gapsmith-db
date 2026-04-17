//! Qdrant HTTP retrieval backend.
//!
//! Embeds the query text via the gapsmith Python bridge (sentence-
//! transformers, configured by `embedder.model`) and posts the resulting
//! vector to Qdrant's `POST /collections/<name>/points/search`. Results
//! are mapped to [`Passage`] and passed through the [`DomainFilter`] as a
//! belt-and-braces guard.
//!
//! The Python bridge is invoked as a subprocess; the call sequence
//! mirrors the verifier bridge in `gapsmith-db-verify::py_bridge` so a
//! single mental model applies to both. Embedding is the only heavyweight
//! op in the path — everything else is HTTP.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};
use tracing::debug;

use super::{Passage, RetrievalBackend, SearchQuery, filter_passages};
use crate::{DomainFilter, ProposeError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedderConfig {
    /// HuggingFace model ID (sentence-transformers compatible).
    pub model: String,
    /// Python project directory containing `pyproject.toml` for the bridge.
    pub python_project: PathBuf,
    /// Invoke through `uv run` when true; bare `python` otherwise.
    #[serde(default = "default_true")]
    pub use_uv: bool,
}

fn default_true() -> bool {
    true
}

impl Default for EmbedderConfig {
    fn default() -> Self {
        Self {
            model: "NeuML/pubmedbert-base-embeddings".into(),
            python_project: PathBuf::from("python"),
            use_uv: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QdrantConfig {
    pub url: String,
    pub collection: String,
    /// Optional API key. Read from env var by convention (e.g. QDRANT_API_KEY).
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub embedder: EmbedderConfig,
    /// Tolerance before the backend decides the HTTP client is broken.
    /// `Duration` isn't (de)serialisable cheaply, so we use a bare u64.
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

fn default_timeout_secs() -> u64 {
    30
}

#[derive(Debug, Clone)]
pub struct QdrantBackend {
    pub config: QdrantConfig,
    pub filter: DomainFilter,
}

impl QdrantBackend {
    #[must_use]
    pub fn new(config: QdrantConfig) -> Self {
        Self {
            config,
            filter: DomainFilter::default(),
        }
    }

    #[must_use]
    pub fn with_filter(mut self, f: DomainFilter) -> Self {
        self.filter = f;
        self
    }

    fn embed_query(&self, text: &str) -> Result<Vec<f32>> {
        #[derive(Serialize)]
        struct Req<'a> {
            text: &'a str,
            model: &'a str,
        }
        #[derive(Deserialize)]
        struct Resp {
            vector: Vec<f32>,
            #[allow(dead_code)]
            #[serde(default)]
            model: String,
            #[allow(dead_code)]
            #[serde(default)]
            dim: usize,
            #[serde(default)]
            note: Option<String>,
        }
        let req = Req {
            text,
            model: &self.config.embedder.model,
        };
        let req_bytes = serde_json::to_vec(&req)?;
        let mut cmd = if self.config.embedder.use_uv {
            let mut c = Command::new("uv");
            c.arg("run")
                .arg("--project")
                .arg(&self.config.embedder.python_project)
                .arg("--extra")
                .arg("retrieval")
                .arg("python");
            c
        } else {
            Command::new("python")
        };
        cmd.arg("-m")
            .arg("gapsmith_bridge.verify")
            .arg("--action")
            .arg("embed")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        debug!(model = %self.config.embedder.model, "spawning embed bridge");
        let mut child = cmd
            .spawn()
            .map_err(|e| ProposeError::Retrieval(format!("bridge spawn: {e}")))?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(&req_bytes)
                .map_err(|e| ProposeError::Retrieval(format!("bridge stdin: {e}")))?;
        }
        let out = child
            .wait_with_output()
            .map_err(|e| ProposeError::Retrieval(format!("bridge wait: {e}")))?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(ProposeError::Retrieval(format!(
                "embed bridge exited {:?}: {stderr}",
                out.status.code()
            )));
        }
        let resp: Resp = serde_json::from_slice(&out.stdout)
            .map_err(|e| ProposeError::Retrieval(format!("bridge decode: {e}")))?;
        if resp.vector.is_empty() {
            return Err(ProposeError::Retrieval(
                resp.note.unwrap_or_else(|| "empty embedding vector".into()),
            ));
        }
        Ok(resp.vector)
    }

    fn qdrant_search(&self, vector: &[f32], top_k: usize) -> Result<Vec<QdrantPoint>> {
        #[derive(Serialize)]
        struct Req<'a> {
            vector: &'a [f32],
            limit: usize,
            with_payload: bool,
        }
        #[derive(Deserialize)]
        struct Envelope {
            result: Vec<QdrantPoint>,
        }
        let url = format!(
            "{}/collections/{}/points/search",
            self.config.url.trim_end_matches('/'),
            self.config.collection,
        );
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(self.config.timeout_secs))
            .build()
            .map_err(|e| ProposeError::Retrieval(format!("http client: {e}")))?;
        let mut req = client.post(&url).json(&Req {
            vector,
            limit: top_k,
            with_payload: true,
        });
        if let Some(api_key) = &self.config.api_key {
            req = req.header("api-key", api_key);
        }
        let resp = req
            .send()
            .map_err(|e| ProposeError::Retrieval(format!("qdrant POST: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            return Err(ProposeError::Retrieval(format!("qdrant {status}: {body}")));
        }
        let env: Envelope = resp
            .json()
            .map_err(|e| ProposeError::Retrieval(format!("qdrant decode: {e}")))?;
        Ok(env.result)
    }
}

#[derive(Debug, Deserialize)]
struct QdrantPoint {
    #[allow(dead_code)]
    #[serde(default)]
    id: serde_json::Value,
    #[serde(default)]
    score: Option<f32>,
    #[serde(default)]
    payload: QdrantPayload,
}

#[derive(Debug, Default, Deserialize)]
struct QdrantPayload {
    #[serde(default)]
    passage_id: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    source_url: Option<String>,
    #[serde(default)]
    pmid: Option<String>,
    #[serde(default)]
    title: Option<String>,
}

fn point_to_passage(p: QdrantPoint) -> Option<Passage> {
    let payload = p.payload;
    Some(Passage {
        id: payload.passage_id?,
        text: payload.text.unwrap_or_default(),
        source_url: payload.source_url.unwrap_or_default(),
        pmid: payload.pmid,
        title: payload.title,
        score: p.score.unwrap_or(0.0),
    })
}

impl RetrievalBackend for QdrantBackend {
    fn search(&self, q: &SearchQuery) -> Result<Vec<Passage>> {
        let vector = self.embed_query(&q.text)?;
        let points = self.qdrant_search(&vector, q.top_k)?;
        let passages: Vec<Passage> = points.into_iter().filter_map(point_to_passage).collect();
        Ok(filter_passages(&self.filter, passages))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults() {
        let e = EmbedderConfig::default();
        assert!(e.use_uv);
        assert!(e.model.contains("pubmedbert"));
    }

    #[test]
    fn point_to_passage_requires_passage_id() {
        let p = QdrantPoint {
            id: serde_json::json!(0),
            score: Some(0.1),
            payload: QdrantPayload::default(),
        };
        assert!(point_to_passage(p).is_none());
    }

    #[test]
    fn point_to_passage_fills_fields() {
        let p = QdrantPoint {
            id: serde_json::json!(0),
            score: Some(0.42),
            payload: QdrantPayload {
                passage_id: Some("europepmc:PMC1#p0".into()),
                text: Some("hello".into()),
                source_url: Some("https://europepmc.org/article/PMC/PMC1".into()),
                pmid: Some("12345".into()),
                title: Some("Some title".into()),
            },
        };
        let passage = point_to_passage(p).unwrap();
        assert_eq!(passage.id, "europepmc:PMC1#p0");
        assert_eq!(passage.pmid.as_deref(), Some("12345"));
        assert!((passage.score - 0.42).abs() < f32::EPSILON);
    }
}
