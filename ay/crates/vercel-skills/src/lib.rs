#![doc = "Agent Skills RFC discovery index and SHA-256 verification helpers for Vercel skills."]
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used)]

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Errors emitted when validating agent skill indices or `SKILL.md` frontmatter.
#[derive(Debug, Error)]
pub enum SkillVerifyError {
    /// Failed to parse index JSON.
    #[error("invalid skills index JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// Missing YAML frontmatter fence.
    #[error("skill markdown missing YAML frontmatter")]
    MissingFrontmatter,
    /// Digest mismatch between file bytes and index entry.
    #[error("sha256 mismatch for `{name}`: expected {expected}, got {actual}")]
    DigestMismatch {
        /// Skill name.
        name: String,
        /// Expected SHA-256 hex digest.
        expected: String,
        /// Computed SHA-256 hex digest.
        actual: String,
    },
}

/// Entry inside `.well-known/agent-skills/index.json`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct SkillIndexEntry {
    /// Canonical skill name.
    pub name: String,
    /// Short skill description.
    pub description: String,
    /// Relative URL (`/skills/<name>/SKILL.md`).
    pub url: String,
    /// Lowercase hex SHA-256 digest of `SKILL.md`.
    pub sha256: String,
    /// UTF-8 byte length.
    pub bytes: usize,
}

/// Top-level RFC discovery index payload.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct WellKnownSkillsIndex {
    /// RFC schema URL.
    #[serde(rename = "$schema")]
    pub schema: String,
    /// ISO-8601 timestamp.
    pub generated_at: String,
    /// Published skill entries.
    pub skills: Vec<SkillIndexEntry>,
}

/// Computes lowercase hex SHA-256 digest over payload bytes.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(64);
    for byte in digest {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

/// Verifies that `SKILL.md` content begins with a valid `---` YAML frontmatter block.
///
/// # Errors
/// Returns [`SkillVerifyError::MissingFrontmatter`] if delimiters are absent.
pub fn verify_frontmatter(markdown: &str) -> Result<(), SkillVerifyError> {
    let normalized = markdown.replace("\r\n", "\n");
    let Some(rest) = normalized.strip_prefix("---\n") else {
        return Err(SkillVerifyError::MissingFrontmatter);
    };
    if !rest.contains("\n---") {
        return Err(SkillVerifyError::MissingFrontmatter);
    }
    Ok(())
}

/// Verifies a single [`SkillIndexEntry`] against its raw `SKILL.md` bytes.
///
/// # Errors
/// Returns [`SkillVerifyError::DigestMismatch`] when SHA-256 differs.
pub fn verify_entry_bytes(entry: &SkillIndexEntry, payload: &[u8]) -> Result<(), SkillVerifyError> {
    let actual = sha256_hex(payload);
    if actual != entry.sha256 {
        return Err(SkillVerifyError::DigestMismatch {
            name: entry.name.clone(),
            expected: entry.sha256.clone(),
            actual,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifies_frontmatter_and_sha256_roundtrip() -> Result<(), SkillVerifyError> {
        let sample = "---\nname: vercel\ndescription: Vercel CLI skill\n---\n\n# Vercel\n";
        verify_frontmatter(sample)?;
        let digest = sha256_hex(sample.as_bytes());
        let entry = SkillIndexEntry {
            name: "vercel".to_string(),
            description: "Vercel CLI skill".to_string(),
            url: "/skills/vercel/SKILL.md".to_string(),
            sha256: digest,
            bytes: sample.len(),
        };
        verify_entry_bytes(&entry, sample.as_bytes())?;
        Ok(())
    }
}
