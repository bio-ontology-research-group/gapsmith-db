//! Subcellular compartments. Enum over the common cases plus an `Other`
//! variant so that rare compartments from upstream sources round-trip
//! without an ingestion crash.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Compartment {
    Cytosol,
    Extracellular,
    Periplasm,
    Mitochondrion,
    Nucleus,
    Endoplasmic,
    Golgi,
    Peroxisome,
    Lysosome,
    Vacuole,
    Chloroplast,
    Thylakoid,
    CellWall,
    Other(String),
}

impl Compartment {
    /// Parse a short code commonly used by ModelSEED / BiGG.
    /// `c0`/`c` → Cytosol, `e0`/`e` → Extracellular, `p0`/`p` → Periplasm.
    /// Unrecognised codes become `Other(code)`.
    #[must_use]
    pub fn from_code(code: &str) -> Self {
        let c = code.trim().to_ascii_lowercase();
        // Strip trailing digit (e.g. ModelSEED's `c0`).
        let stripped = c.trim_end_matches(|ch: char| ch.is_ascii_digit());
        match stripped {
            "c" | "cytosol" | "cyt" => Compartment::Cytosol,
            "e" | "extracellular" | "ext" => Compartment::Extracellular,
            "p" | "periplasm" | "per" => Compartment::Periplasm,
            "m" | "mitochondrion" | "mit" => Compartment::Mitochondrion,
            "n" | "nucleus" => Compartment::Nucleus,
            "r" | "er" | "endoplasmic" => Compartment::Endoplasmic,
            "g" | "golgi" => Compartment::Golgi,
            "x" | "peroxisome" => Compartment::Peroxisome,
            "l" | "lysosome" => Compartment::Lysosome,
            "v" | "vacuole" => Compartment::Vacuole,
            "h" | "chloroplast" => Compartment::Chloroplast,
            "u" | "thylakoid" => Compartment::Thylakoid,
            "w" | "cellwall" | "cw" => Compartment::CellWall,
            _ => Compartment::Other(code.to_string()),
        }
    }

    #[must_use]
    pub fn short_code(&self) -> &str {
        match self {
            Compartment::Cytosol => "c",
            Compartment::Extracellular => "e",
            Compartment::Periplasm => "p",
            Compartment::Mitochondrion => "m",
            Compartment::Nucleus => "n",
            Compartment::Endoplasmic => "r",
            Compartment::Golgi => "g",
            Compartment::Peroxisome => "x",
            Compartment::Lysosome => "l",
            Compartment::Vacuole => "v",
            Compartment::Chloroplast => "h",
            Compartment::Thylakoid => "u",
            Compartment::CellWall => "w",
            Compartment::Other(s) => s,
        }
    }
}

impl std::fmt::Display for Compartment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.short_code())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modelseed_codes() {
        assert_eq!(Compartment::from_code("c0"), Compartment::Cytosol);
        assert_eq!(Compartment::from_code("e0"), Compartment::Extracellular);
        assert_eq!(Compartment::from_code("p"), Compartment::Periplasm);
    }

    #[test]
    fn falls_through_to_other() {
        match Compartment::from_code("xyz") {
            Compartment::Other(s) => assert_eq!(s, "xyz"),
            c => panic!("expected Other, got {c:?}"),
        }
    }
}
