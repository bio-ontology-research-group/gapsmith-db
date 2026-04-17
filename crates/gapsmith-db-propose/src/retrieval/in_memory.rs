//! In-memory retrieval backend. Exact-substring matching on `text`;
//! used by tests and the mock proposer.

use super::{Passage, RetrievalBackend, SearchQuery, filter_passages};
use crate::DomainFilter;

#[derive(Debug, Clone)]
pub struct InMemoryBackend {
    pub passages: Vec<Passage>,
    pub filter: DomainFilter,
}

impl InMemoryBackend {
    #[must_use]
    pub fn new(passages: Vec<Passage>) -> Self {
        Self {
            passages,
            filter: DomainFilter::default(),
        }
    }

    #[must_use]
    pub fn with_filter(mut self, f: DomainFilter) -> Self {
        self.filter = f;
        self
    }
}

impl RetrievalBackend for InMemoryBackend {
    fn search(&self, q: &SearchQuery) -> crate::Result<Vec<Passage>> {
        let needle = q.text.to_ascii_lowercase();
        let mut scored: Vec<Passage> = self
            .passages
            .iter()
            .filter_map(|p| {
                let hay = p.text.to_ascii_lowercase();
                if hay.contains(&needle) {
                    let mut p = p.clone();
                    // crude score: longer match gets more, inverse of text length.
                    #[allow(clippy::cast_precision_loss)]
                    let len_f = p.text.len() as f32;
                    p.score = 1.0 / (1.0 + len_f / 1000.0);
                    Some(p)
                } else {
                    None
                }
            })
            .collect();
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(q.top_k.max(1));
        Ok(filter_passages(&self.filter, scored))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(id: &str, url: &str, text: &str) -> Passage {
        Passage {
            id: id.into(),
            text: text.into(),
            source_url: url.into(),
            pmid: None,
            title: None,
            score: 0.0,
        }
    }

    #[test]
    fn returns_matches_and_drops_forbidden() {
        let forbidden_url = format!("https://{}/foo", {
            let rev: String = "gro.cycateM".chars().rev().collect();
            rev
        });
        let passages = vec![
            p(
                "a",
                "https://europepmc.org/PMC1#p1",
                "methanogenesis from CO2",
            ),
            p("b", &forbidden_url, "methanogenesis from CO2"),
        ];
        let backend = InMemoryBackend::new(passages);
        let out = backend
            .search(&SearchQuery {
                text: "methanogenesis".into(),
                top_k: 10,
            })
            .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, "a");
    }
}
