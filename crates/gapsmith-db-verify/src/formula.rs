//! Chemical formula parsing.
//!
//! Handles standard formulae like `C6H12O6`, parenthesised groups
//! (`Ca(OH)2`), and common R/X placeholders (treated as a "skip" signal).
//! Returns a `BTreeMap<String, i64>` of element → count, which supports
//! addition and comparison for atom-balance checks.

use std::collections::BTreeMap;

use thiserror::Error;

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum FormulaError {
    #[error("empty formula")]
    Empty,
    #[error("unbalanced parenthesis at byte {0}")]
    UnbalancedParen(usize),
    #[error("expected element symbol at byte {0}, got {1:?}")]
    ExpectedElement(usize, char),
    #[error("R/X placeholder in formula — cannot balance")]
    HasPlaceholder,
}

pub type AtomCounts = BTreeMap<String, i64>;

/// Parse a chemical formula string into element counts. Placeholder
/// symbols (`R`, `R1`..`R9`, `X`, `Z`, `*`) cause [`FormulaError::HasPlaceholder`]
/// — callers are expected to handle that as "skip, not balance-able".
pub fn parse(s: &str) -> Result<AtomCounts, FormulaError> {
    let s = s.trim();
    if s.is_empty() {
        return Err(FormulaError::Empty);
    }
    let bytes = s.as_bytes();
    let mut pos = 0;
    let mut counts = AtomCounts::new();
    parse_group(bytes, &mut pos, &mut counts, 1, 0)?;
    if pos != bytes.len() {
        return Err(FormulaError::ExpectedElement(pos, bytes[pos] as char));
    }
    Ok(counts)
}

fn parse_group(
    bytes: &[u8],
    pos: &mut usize,
    counts: &mut AtomCounts,
    multiplier: i64,
    depth: u32,
) -> Result<(), FormulaError> {
    while *pos < bytes.len() {
        let c = bytes[*pos] as char;
        match c {
            '(' => {
                let start = *pos;
                *pos += 1;
                let mut inner = AtomCounts::new();
                parse_group(bytes, pos, &mut inner, 1, depth + 1)?;
                if *pos >= bytes.len() || bytes[*pos] as char != ')' {
                    return Err(FormulaError::UnbalancedParen(start));
                }
                *pos += 1;
                let n = read_count(bytes, pos);
                for (k, v) in inner {
                    *counts.entry(k).or_insert(0) += v * n * multiplier;
                }
            }
            ')' => {
                if depth == 0 {
                    return Err(FormulaError::UnbalancedParen(*pos));
                }
                return Ok(());
            }
            c if c.is_ascii_alphabetic() => {
                let element = read_element(bytes, pos);
                if is_placeholder(&element) {
                    return Err(FormulaError::HasPlaceholder);
                }
                let n = read_count(bytes, pos);
                *counts.entry(element).or_insert(0) += n * multiplier;
            }
            c if c.is_ascii_whitespace() || c == '·' || c == '.' => {
                *pos += 1;
            }
            '+' | '-' => {
                // charge suffix at end; just skip + optional digits.
                *pos += 1;
                let _ = read_count(bytes, pos);
            }
            _ => {
                return Err(FormulaError::ExpectedElement(*pos, c));
            }
        }
    }
    Ok(())
}

fn read_element(bytes: &[u8], pos: &mut usize) -> String {
    // Uppercase then zero or more lowercase.
    let start = *pos;
    *pos += 1;
    while *pos < bytes.len() && (bytes[*pos] as char).is_ascii_lowercase() {
        *pos += 1;
    }
    String::from_utf8_lossy(&bytes[start..*pos]).into_owned()
}

fn read_count(bytes: &[u8], pos: &mut usize) -> i64 {
    let start = *pos;
    while *pos < bytes.len() && (bytes[*pos] as char).is_ascii_digit() {
        *pos += 1;
    }
    if start == *pos {
        1
    } else {
        std::str::from_utf8(&bytes[start..*pos])
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1)
    }
}

fn is_placeholder(sym: &str) -> bool {
    matches!(
        sym,
        "R" | "R1" | "R2" | "R3" | "R4" | "R5" | "R6" | "R7" | "R8" | "R9" | "X" | "Z" | "A" | "*"
    )
}

/// Difference of two atom counts: `lhs - rhs`. Zero counts omitted.
#[must_use]
pub fn diff(lhs: &AtomCounts, rhs: &AtomCounts) -> AtomCounts {
    let mut out = lhs.clone();
    for (k, v) in rhs {
        *out.entry(k.clone()).or_insert(0) -= *v;
    }
    out.retain(|_, v| *v != 0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_water() {
        let c = parse("H2O").unwrap();
        assert_eq!(c.get("H"), Some(&2));
        assert_eq!(c.get("O"), Some(&1));
    }

    #[test]
    fn parses_glucose() {
        let c = parse("C6H12O6").unwrap();
        assert_eq!(c.get("C"), Some(&6));
        assert_eq!(c.get("H"), Some(&12));
        assert_eq!(c.get("O"), Some(&6));
    }

    #[test]
    fn parses_parenthesised() {
        let c = parse("Ca(OH)2").unwrap();
        assert_eq!(c.get("Ca"), Some(&1));
        assert_eq!(c.get("O"), Some(&2));
        assert_eq!(c.get("H"), Some(&2));
    }

    #[test]
    fn parses_with_charge_suffix() {
        let c = parse("HSO4-").unwrap();
        assert_eq!(c.get("H"), Some(&1));
        assert_eq!(c.get("S"), Some(&1));
        assert_eq!(c.get("O"), Some(&4));
    }

    #[test]
    fn rejects_placeholder() {
        assert_eq!(parse("C6H12R"), Err(FormulaError::HasPlaceholder));
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(parse(""), Err(FormulaError::Empty));
    }

    #[test]
    fn diff_is_signed() {
        let a = parse("H2O").unwrap();
        let b = parse("O").unwrap();
        let d = diff(&a, &b);
        assert_eq!(d.get("H"), Some(&2));
        assert!(!d.contains_key("O"), "O should cancel");
    }
}
