use crate::lockfile::{ApmLockfile, compute_mcp_fingerprint, sha256_prefixed};
use crate::manifest::ApmManifest;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FindingSeverity {
    Critical,
    Warning,
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditFinding {
    pub severity: FindingSeverity,
    pub rule: String,
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditReport {
    pub ok: bool,
    pub ci_mode: bool,
    pub files_scanned: usize,
    pub findings: Vec<AuditFinding>,
}

/// Returns true if `ch` is a hidden/dangerous Unicode control character used in
/// prompt-injection or Trojan Source attacks:
/// - BiDi overrides & isolates (`U+202A..=U+202E`, `U+2066..=U+2069`, `U+200E`, `U+200F`, `U+061C`)
/// - Zero-width invisibles (`U+200B..=U+200D`, `U+2060`, `U+FEFF` after byte 0)
/// - Unicode Tags block (`U+E0000..=U+E007F`, commonly abused for invisible LLM instructions)
pub fn is_hidden_unicode(ch: char, byte_offset: usize) -> bool {
    let cp = ch as u32;
    matches!(
        cp,
        0x061C
            | 0x200B..=0x200F
            | 0x202A..=0x202E
            | 0x2060
            | 0x2066..=0x2069
            | 0xE0000..=0xE007F
    ) || (cp == 0xFEFF && byte_offset > 0)
}

/// Scan UTF-8 text for hidden Unicode characters that can hijack agent behavior.
pub fn scan_hidden_unicode(path_label: &str, content: &str) -> Vec<AuditFinding> {
    let mut findings = Vec::new();
    for (byte_idx, ch) in content.char_indices() {
        if is_hidden_unicode(ch, byte_idx) {
            findings.push(AuditFinding {
                severity: FindingSeverity::Critical,
                rule: "hidden-unicode".into(),
                path: path_label.to_string(),
                message: format!(
                    "detected hidden/BiDi Unicode codepoint U+{:04X} at byte offset {byte_idx}",
                    ch as u32
                ),
            });
        }
    }
    findings
}

/// Audit a project workspace for:
/// 1. Hidden Unicode / prompt-injection characters in `apm.yml`, `AGENTS.md`, `.apm/`, and deployed files.
/// 2. Lockfile presence and SHA-256 content-integrity verification (drift detection).
/// 3. Manifest-vs-lockfile consistency and transitive MCP approval (`req-sc-016`) in `--ci` mode.
pub fn audit_project(root: &Path, manifest: &ApmManifest, ci_mode: bool) -> AuditReport {
    let mut findings = Vec::new();
    let mut files_scanned = 0usize;

    // 1. Scan apm.yml & AGENTS.md for hidden Unicode
    for rel in ["apm.yml", "AGENTS.md"] {
        let p = root.join(rel);
        if let Ok(text) = std::fs::read_to_string(&p) {
            files_scanned += 1;
            findings.extend(scan_hidden_unicode(rel, &text));
        }
    }

    // 2. Verify lockfile presence & deployed file SHA-256 integrity
    let lock_path = root.join("apm.lock.json");
    match ApmLockfile::load_json(&lock_path) {
        Err(_) => {
            findings.push(AuditFinding {
                severity: if ci_mode {
                    FindingSeverity::Critical
                } else {
                    FindingSeverity::Warning
                },
                rule: "lockfile-present".into(),
                path: "apm.lock.json".into(),
                message: "apm.lock.json is missing; run `fw apm install` to generate it".into(),
            });
        }
        Ok(lock) => {
            for dep in &lock.dependencies {
                for (rel_path, expected_hash) in &dep.deployed_file_hashes {
                    let full = root.join(rel_path);
                    match std::fs::read(&full) {
                        Err(_) => {
                            findings.push(AuditFinding {
                                severity: FindingSeverity::Critical,
                                rule: "deployed-file-missing".into(),
                                path: rel_path.clone(),
                                message: format!(
                                    "locked file for `{}` is missing from disk",
                                    dep.name
                                ),
                            });
                        }
                        Ok(bytes) => {
                            files_scanned += 1;
                            let actual = sha256_prefixed(&bytes);
                            if &actual != expected_hash {
                                findings.push(AuditFinding {
                                    severity: FindingSeverity::Critical,
                                    rule: "content-integrity".into(),
                                    path: rel_path.clone(),
                                    message: format!(
                                        "SHA-256 drift detected for `{}`: expected {expected_hash}, got {actual}",
                                        dep.name
                                    ),
                                });
                            }
                            if let Ok(text) = std::str::from_utf8(&bytes) {
                                findings.extend(scan_hidden_unicode(rel_path, text));
                            }
                        }
                    }
                }
            }

            // 3. OpenAPM req-sc-016: in CI mode, fail any transitive MCP server admitted in
            //    lockfile that is NOT approved in `apm.yml` (direct declaration or sha256 pin).
            if ci_mode {
                for srv in &lock.mcp_servers {
                    if srv.depth == 0 {
                        continue;
                    }
                    let direct_match = manifest.dependencies.mcp.iter().any(|m| {
                        m.name == srv.name && compute_mcp_fingerprint(m) == srv.definition_fingerprint
                    });
                    let pinned = manifest.executables.is_mcp_approved(
                        &srv.source_package,
                        &srv.name,
                        &srv.definition_fingerprint,
                    );
                    if !direct_match && !pinned {
                        findings.push(AuditFinding {
                            severity: FindingSeverity::Critical,
                            rule: "mcp-config-consistency".into(),
                            path: "apm.lock.json".into(),
                            message: format!(
                                "transitive MCP server `{}` from `{}` is admitted in lockfile but not approved in apm.yml (run `approve`)",
                                srv.name, srv.source_package
                            ),
                        });
                    }
                }
            }
        }
    }

    let ok = !findings
        .iter()
        .any(|f| f.severity == FindingSeverity::Critical);
    AuditReport {
        ok,
        ci_mode,
        files_scanned,
        findings,
    }
}
