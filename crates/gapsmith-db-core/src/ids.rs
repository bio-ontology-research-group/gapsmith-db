//! Strongly-typed identifiers for the three top-level entities.
//!
//! IDs are opaque strings with a newtype wrapper so that a `CompoundId`
//! cannot be accidentally passed where a `ReactionId` is expected. By
//! convention, internal IDs are prefixed: `C<hex>` for compounds,
//! `R<hex>` for reactions, `P<hex>` for pathways. Upstream IDs (ChEBI,
//! Rhea, SEED) are kept in the `xrefs` field of each entity.

use serde::{Deserialize, Serialize};

macro_rules! define_id {
    ($name:ident, $prefix:expr, $doc:expr) => {
        #[doc = $doc]
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub const PREFIX: &'static str = $prefix;

            #[must_use]
            pub fn new(s: impl Into<String>) -> Self {
                Self(s.into())
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }

            #[must_use]
            pub fn into_inner(self) -> String {
                self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self {
                Self(s)
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self(s.to_string())
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
    };
}

define_id!(
    CompoundId,
    "C",
    "Internal compound identifier (prefix `C`)."
);
define_id!(
    ReactionId,
    "R",
    "Internal reaction identifier (prefix `R`)."
);
define_id!(PathwayId, "P", "Internal pathway identifier (prefix `P`).");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_matches_newtype() {
        let c = CompoundId::new("C00000001");
        assert_eq!(c.to_string(), "C00000001");
        assert_eq!(c.as_ref(), "C00000001");
    }

    #[test]
    fn serde_is_transparent() {
        let r = ReactionId::new("R0042");
        let s = serde_json::to_string(&r).unwrap();
        assert_eq!(s, "\"R0042\"");
        let back: ReactionId = serde_json::from_str(&s).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn prefixes_are_distinct() {
        assert_ne!(CompoundId::PREFIX, ReactionId::PREFIX);
        assert_ne!(ReactionId::PREFIX, PathwayId::PREFIX);
    }
}
