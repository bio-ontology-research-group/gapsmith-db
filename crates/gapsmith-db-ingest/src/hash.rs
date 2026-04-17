//! SHA256 hashing and atomic rename helpers.

use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::{IngestError, Result};

/// Streaming SHA256 of a file. 64 KiB buffer.
pub fn sha256_file(path: &Path) -> Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0_u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Verify `actual` matches `expected` (case-insensitive hex).
pub fn verify_sha256(url: &str, expected: &str, actual: &str) -> Result<()> {
    if expected.eq_ignore_ascii_case(actual) {
        Ok(())
    } else {
        Err(IngestError::HashMismatch {
            url: url.to_string(),
            expected: expected.to_string(),
            actual: actual.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_of_known_input() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x");
        std::fs::write(&p, b"hello").unwrap();
        // sha256("hello")
        assert_eq!(
            sha256_file(&p).unwrap(),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn verify_is_case_insensitive() {
        assert!(verify_sha256("u", "ABCdef", "abcdef").is_ok());
        assert!(verify_sha256("u", "abcdef", "ffffff").is_err());
    }
}
