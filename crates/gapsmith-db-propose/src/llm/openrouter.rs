//! OpenRouter client.
//!
//! Wire protocol only in Phase 4. `complete` is a real HTTP POST wrapped
//! in blocking tokio but we do not exercise it in tests — the user said
//! they already have OpenClaw + OpenRouter configured, so the credentials
//! come from the environment at runtime.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::LlmBackend;
use crate::schema::Proposal;
use crate::{ProposeError, Result};

const DEFAULT_ENDPOINT: &str = "https://openrouter.ai/api/v1/chat/completions";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenRouterConfig {
    /// Model slug (e.g. "anthropic/claude-opus-4-7").
    pub model: String,
    /// Override the default endpoint if needed.
    #[serde(default)]
    pub endpoint: Option<String>,
    /// Environment variable name for the API key. Default `OPENROUTER_API_KEY`.
    #[serde(default)]
    pub api_key_env: Option<String>,
    /// Sampling temperature.
    #[serde(default)]
    pub temperature: Option<f32>,
    /// Max tokens in the response.
    #[serde(default)]
    pub max_tokens: Option<u32>,
    /// Optional HTTP-Referer / X-Title headers (OpenRouter attribution).
    #[serde(default)]
    pub referer: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
}

impl OpenRouterConfig {
    #[must_use]
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            endpoint: None,
            api_key_env: None,
            temperature: Some(0.2),
            max_tokens: Some(8192),
            referer: None,
            title: None,
        }
    }

    fn endpoint(&self) -> &str {
        self.endpoint.as_deref().unwrap_or(DEFAULT_ENDPOINT)
    }

    fn api_key(&self) -> Result<String> {
        let env = self.api_key_env.as_deref().unwrap_or("OPENROUTER_API_KEY");
        std::env::var(env).map_err(|_| ProposeError::Llm(format!("API key env var {env} not set")))
    }
}

#[derive(Debug, Clone)]
pub struct OpenRouterBackend {
    pub config: OpenRouterConfig,
}

impl OpenRouterBackend {
    #[must_use]
    pub fn new(config: OpenRouterConfig) -> Self {
        Self { config }
    }
}

#[derive(Debug, Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    response_format: ResponseFormat,
}

#[derive(Debug, Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    kind: &'static str,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatResponseMessage,
}

#[derive(Debug, Deserialize)]
struct ChatResponseMessage {
    content: String,
}

impl LlmBackend for OpenRouterBackend {
    fn name(&self) -> &str {
        &self.config.model
    }

    fn complete(&self, prompt: &str) -> Result<Proposal> {
        let api_key = self.config.api_key()?;
        let endpoint = self.config.endpoint().to_string();
        let req = ChatRequest {
            model: &self.config.model,
            messages: vec![
                ChatMessage {
                    role: "system",
                    content: "You are gapsmith-db's pathway proposer. Respond with ONLY a JSON object matching the gapsmith-db Proposal schema.",
                },
                ChatMessage {
                    role: "user",
                    content: prompt,
                },
            ],
            temperature: self.config.temperature,
            max_tokens: self.config.max_tokens,
            response_format: ResponseFormat {
                kind: "json_object",
            },
        };
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(180))
            .build()?;
        let mut rb = client
            .post(&endpoint)
            .bearer_auth(&api_key)
            .header("Content-Type", "application/json");
        if let Some(r) = &self.config.referer {
            rb = rb.header("HTTP-Referer", r);
        }
        if let Some(t) = &self.config.title {
            rb = rb.header("X-Title", t);
        }
        let resp = rb
            .json(&req)
            .send()?
            .error_for_status()
            .map_err(|e| ProposeError::Llm(format!("openrouter http: {e}")))?;
        let parsed: ChatResponse = resp.json()?;
        let content = parsed
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| ProposeError::Llm("openrouter returned no choices".into()))?;
        let mut proposal: Proposal = parse_proposal_content(&content).map_err(|e| {
            ProposeError::Llm(format!("openrouter response not valid Proposal JSON: {e}"))
        })?;
        proposal.model.clone_from(&self.config.model);
        Ok(proposal.hashed())
    }
}

/// Parse a model response into a `Proposal`.
///
/// Tries the raw body first. On failure, falls back to stripping common
/// markdown fences (```json … ```) and slicing from the first `{` to the
/// last `}` — the typical shape of a chat response that ignored
/// `response_format: json_object`. This makes the client robust against
/// free-tier models that add preamble/commentary around the JSON.
fn parse_proposal_content(raw: &str) -> std::result::Result<Proposal, serde_json::Error> {
    if let Ok(p) = serde_json::from_str::<Proposal>(raw) {
        return Ok(p);
    }
    serde_json::from_str(extract_json_object(raw))
}

fn extract_json_object(raw: &str) -> &str {
    let s = raw.trim();
    // Strip leading/trailing triple-backtick fences, optionally with a
    // language tag (```json). We tolerate both ```json and ``` at the
    // start, and an optional trailing ```.
    let s = s
        .strip_prefix("```json")
        .or_else(|| s.strip_prefix("```JSON"))
        .or_else(|| s.strip_prefix("```"))
        .map_or(s, str::trim);
    let s = s.strip_suffix("```").map_or(s, str::trim);
    // Slice from the first `{` to the last `}` to drop any prose the
    // model added before/after.
    let start = s.find('{');
    let end = s.rfind('}');
    match (start, end) {
        (Some(a), Some(b)) if b > a => &s[a..=b],
        _ => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_plain_json_unchanged() {
        let s = r#"{"a":1}"#;
        assert_eq!(extract_json_object(s), s);
    }

    #[test]
    fn extract_strips_json_fence() {
        let s = "```json\n{\"a\":1}\n```";
        assert_eq!(extract_json_object(s), "{\"a\":1}");
    }

    #[test]
    fn extract_strips_bare_fence_and_prose() {
        let s = "Sure, here's the proposal:\n```\n{\"schema_version\":\"1\"}\n```\nLet me know if you need anything else.";
        assert_eq!(extract_json_object(s), "{\"schema_version\":\"1\"}");
    }

    #[test]
    fn extract_slices_on_prose_without_fence() {
        let s = "Here it is: {\"a\":1} — hope that helps!";
        assert_eq!(extract_json_object(s), "{\"a\":1}");
    }
}
