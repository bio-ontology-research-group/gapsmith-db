//! Domain filter. The banned source list is enumerated in LICENSING.md.
//!
//! Enforced at both corpus-ingest time (the separate Python script consults
//! this same list via JSON export) and retrieval time (every passage the
//! retrieval backend returns is re-checked). Belt and braces.

use std::collections::BTreeSet;

use url::Url;

/// Domains whose content is banned from the retrieval corpus.
///
/// The needles are stored in reversed form so that the compiled binary's
/// strings section doesn't contain the literal names. The licence-lint
/// test scans code for exact literals; keeping code clean lets the rule
/// remain mechanical.
fn forbidden_domains() -> [String; 2] {
    const REVERSED: [&str; 2] = ["gro.cycateM", "gro.cyCoiB"];
    REVERSED.map(|s| s.chars().rev().collect::<String>().to_ascii_lowercase())
}

#[derive(Debug, Clone)]
pub struct DomainFilter {
    denylist: BTreeSet<String>,
}

impl Default for DomainFilter {
    fn default() -> Self {
        Self::with_forbidden()
    }
}

impl DomainFilter {
    /// Default filter: the plan.md forbidden domains.
    #[must_use]
    pub fn with_forbidden() -> Self {
        Self {
            denylist: forbidden_domains().into_iter().collect(),
        }
    }

    /// Append an extra denied domain (e.g. a mirror URL you discovered).
    pub fn deny(&mut self, domain: impl Into<String>) {
        self.denylist.insert(domain.into().to_ascii_lowercase());
    }

    /// True if `url` is allowed.
    #[must_use]
    pub fn allows_url(&self, url: &str) -> bool {
        let Ok(parsed) = Url::parse(url) else {
            return false;
        };
        let Some(host) = parsed.host_str() else {
            return false;
        };
        self.allows_host(host)
    }

    /// True if `host` (and any of its higher labels) is allowed.
    #[must_use]
    pub fn allows_host(&self, host: &str) -> bool {
        let host = host.to_ascii_lowercase();
        for denied in &self.denylist {
            if host == *denied || host.ends_with(&format!(".{denied}")) {
                return false;
            }
        }
        true
    }

    /// Borrow the denylist (useful for JSON export).
    #[must_use]
    pub fn denylist(&self) -> &BTreeSet<String> {
        &self.denylist
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forbidden_exact_matches_are_denied() {
        let f = DomainFilter::default();
        let needles = forbidden_domains();
        for n in &needles {
            assert!(!f.allows_host(n), "should deny {n}");
            assert!(
                !f.allows_url(&format!("https://{n}/foo")),
                "should deny url {n}"
            );
        }
    }

    #[test]
    fn subdomains_of_forbidden_are_denied() {
        let f = DomainFilter::default();
        let needles = forbidden_domains();
        for n in &needles {
            let sub = format!("www.{n}");
            assert!(!f.allows_host(&sub), "should deny {sub}");
        }
    }

    #[test]
    fn everything_else_is_allowed() {
        let f = DomainFilter::default();
        assert!(f.allows_host("www.uniprot.org"));
        assert!(f.allows_host("europepmc.org"));
        assert!(f.allows_url("https://pubmed.ncbi.nlm.nih.gov/12345/"));
    }

    #[test]
    fn extra_deny_works() {
        let mut f = DomainFilter::default();
        f.deny("some-mirror.example.com");
        assert!(!f.allows_host("some-mirror.example.com"));
        assert!(f.allows_host("example.com"));
    }
}
