//! PubMed identifier. Numeric but carried as a string to tolerate leading
//! zeros and extremely large future IDs without silently truncating.

use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Pmid(String);

impl Pmid {
    #[must_use]
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Pmid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for Pmid {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Pmid(
            s.trim().trim_start_matches("PMID:").trim().to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_prefix() {
        let p: Pmid = "PMID:12345".parse().unwrap();
        assert_eq!(p.as_str(), "12345");
    }
}
