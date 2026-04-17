//! Reaction reversibility. Strict three-valued; the distinction between
//! `Forward` and `Reverse` matters because reaction directionality is
//! often written as "A -> B" in the source and inverted after thermodynamic
//! analysis.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reversibility {
    /// Left-to-right only.
    Forward,
    /// Right-to-left only.
    Reverse,
    /// Either direction.
    Reversible,
}

impl Reversibility {
    /// Parse common directionality arrows: `->`, `=>`, `<=>`, `<=`, `=`.
    #[must_use]
    pub fn from_arrow(s: &str) -> Option<Self> {
        match s.trim() {
            "->" | "=>" => Some(Reversibility::Forward),
            "<-" | "<=" => Some(Reversibility::Reverse),
            "<=>" | "=" | "<->" => Some(Reversibility::Reversible),
            _ => None,
        }
    }

    /// Parse a ModelSEED `direction` field: `>` / `<` / `=` / `?`.
    #[must_use]
    pub fn from_modelseed(s: &str) -> Option<Self> {
        match s.trim() {
            ">" => Some(Reversibility::Forward),
            "<" => Some(Reversibility::Reverse),
            "=" | "?" => Some(Reversibility::Reversible),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_arrows() {
        assert_eq!(
            Reversibility::from_arrow("->"),
            Some(Reversibility::Forward)
        );
        assert_eq!(
            Reversibility::from_arrow("<=>"),
            Some(Reversibility::Reversible)
        );
        assert_eq!(
            Reversibility::from_arrow("<-"),
            Some(Reversibility::Reverse)
        );
    }

    #[test]
    fn parses_modelseed() {
        assert_eq!(
            Reversibility::from_modelseed(">"),
            Some(Reversibility::Forward)
        );
        assert_eq!(
            Reversibility::from_modelseed("="),
            Some(Reversibility::Reversible)
        );
        assert_eq!(
            Reversibility::from_modelseed("?"),
            Some(Reversibility::Reversible)
        );
    }
}
