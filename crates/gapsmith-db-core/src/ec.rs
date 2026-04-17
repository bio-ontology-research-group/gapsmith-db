//! EC (Enzyme Commission) number. Parses `a.b.c.d` with optional wildcards
//! (`-` or `*`) at any level after the first digit.

use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// An EC number level is either a concrete u16 or a wildcard (`-`/`*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EcWildcard {
    Concrete(u16),
    Wildcard,
}

impl std::fmt::Display for EcWildcard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EcWildcard::Concrete(n) => write!(f, "{n}"),
            EcWildcard::Wildcard => f.write_str("-"),
        }
    }
}

/// A four-level EC number. Levels 2–4 may be wildcards; level 1 must be
/// concrete (1–7 in current IUBMB nomenclature, but we don't enforce the
/// range so that new top-level classes continue to parse).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EcNumber {
    pub class: u16,
    pub subclass: EcWildcard,
    pub sub_subclass: EcWildcard,
    pub serial: EcWildcard,
}

impl EcNumber {
    #[must_use]
    pub fn is_fully_specified(&self) -> bool {
        matches!(
            (self.subclass, self.sub_subclass, self.serial),
            (
                EcWildcard::Concrete(_),
                EcWildcard::Concrete(_),
                EcWildcard::Concrete(_)
            )
        )
    }
}

impl std::fmt::Display for EcNumber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}.{}.{}.{}",
            self.class, self.subclass, self.sub_subclass, self.serial
        )
    }
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum EcParseError {
    #[error("empty EC number")]
    Empty,
    #[error("EC number must have exactly 4 dot-separated levels, got {0}")]
    WrongLevelCount(usize),
    #[error("level {level}: invalid digit '{raw}'")]
    BadDigit { level: u8, raw: String },
    #[error("level 1 (class) must be concrete, got wildcard")]
    ClassIsWildcard,
}

impl FromStr for EcNumber {
    type Err = EcParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s
            .trim()
            .trim_start_matches("EC")
            .trim_start_matches(':')
            .trim();
        if trimmed.is_empty() {
            return Err(EcParseError::Empty);
        }
        let parts: Vec<&str> = trimmed.split('.').collect();
        if parts.len() != 4 {
            return Err(EcParseError::WrongLevelCount(parts.len()));
        }
        let class = parse_level(parts[0], 1)?;
        let EcWildcard::Concrete(class) = class else {
            return Err(EcParseError::ClassIsWildcard);
        };
        Ok(EcNumber {
            class,
            subclass: parse_level(parts[1], 2)?,
            sub_subclass: parse_level(parts[2], 3)?,
            serial: parse_level(parts[3], 4)?,
        })
    }
}

fn parse_level(raw: &str, level: u8) -> Result<EcWildcard, EcParseError> {
    let raw = raw.trim();
    if raw == "-" || raw == "*" || raw.is_empty() {
        return Ok(EcWildcard::Wildcard);
    }
    raw.parse::<u16>()
        .map(EcWildcard::Concrete)
        .map_err(|_| EcParseError::BadDigit {
            level,
            raw: raw.to_string(),
        })
}

impl Serialize for EcNumber {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for EcNumber {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        EcNumber::from_str(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_concrete() {
        let ec: EcNumber = "1.2.3.4".parse().unwrap();
        assert_eq!(ec.class, 1);
        assert!(ec.is_fully_specified());
        assert_eq!(ec.to_string(), "1.2.3.4");
    }

    #[test]
    fn parses_with_wildcards() {
        let ec: EcNumber = "1.2.-.-".parse().unwrap();
        assert_eq!(ec.to_string(), "1.2.-.-");
        assert!(!ec.is_fully_specified());
        let ec2: EcNumber = "1.*.*.*".parse().unwrap();
        assert_eq!(ec2.to_string(), "1.-.-.-");
    }

    #[test]
    fn strips_ec_prefix() {
        let ec: EcNumber = "EC:1.1.1.1".parse().unwrap();
        assert_eq!(ec.to_string(), "1.1.1.1");
        let ec2: EcNumber = "EC 2.7.1.1".parse().unwrap();
        assert_eq!(ec2.to_string(), "2.7.1.1");
    }

    #[test]
    fn rejects_bad_input() {
        assert!("1.2.3".parse::<EcNumber>().is_err());
        assert!("abc".parse::<EcNumber>().is_err());
        assert!("-.1.1.1".parse::<EcNumber>().is_err()); // class wildcard
        assert!("".parse::<EcNumber>().is_err());
    }

    #[test]
    fn serde_roundtrip() {
        let ec: EcNumber = "6.3.2.-".parse().unwrap();
        let json = serde_json::to_string(&ec).unwrap();
        assert_eq!(json, "\"6.3.2.-\"");
        let back: EcNumber = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ec);
    }
}
