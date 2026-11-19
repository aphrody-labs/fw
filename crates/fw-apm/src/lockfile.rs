use crate::manifest::{McpServerSpec, PackageSource, PrimitiveKind};
use flate2::Compression;
use flate2::write::GzEncoder;
use fw_nix::compute_fixed_output_store_entry;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::Write;
use std::path::Path;

pub const LOCKFILE_VERSION: &str = "1";
pub const FW_APM_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lowercase 64-char SHA-256 hex digest.
pub fn sha256_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for b in digest {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

/// `sha256:<64 hex>` content integrity string matching `microsoft/apm` lockfile spec.
pub fn sha256_prefixed(bytes: &[u8]) -> String {
    format!("sha256:{}", sha256_hex(bytes))
}

/// Deterministic SHA-256 hash over a sorted map of `rel_path -> sha256:<hex>`.
pub fn compute_tree_hash(file_hashes: &BTreeMap<String, String>) -> String {
    let mut hasher = Sha256::new();
    for (path, hash) in file_hashes {
        hasher.update(path.as_bytes());
        hasher.update(b"\0");
        hasher.update(hash.as_bytes());
        hasher.update(b"\n");
    }
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(71);
    hex.push_str("sha256:");
    for b in digest {
        hex.push(HEX[(b >> 4) as usize] as char);
        hex.push(HEX[(b & 0x0f) as usize] as char);
    }
    hex
}

/// Canonical definition fingerprint for an MCP server declaration (OpenAPM `req-sc-016`).
pub fn compute_mcp_fingerprint(spec: &McpServerSpec) -> String {
    let canonical = json!({
        "name": spec.name,
        "transport": spec.transport.as_str(),
        "command": spec.command,
        "args": spec.args,
        "url": spec.url,
        "env": spec.env,
    });
    let bytes = serde_json::to_vec(&canonical).unwrap_or_default();
    sha256_prefixed(&bytes)
}

/// Compute a deterministic NixOS `/nix/store/<hash>-<name>` path for a locked package
/// via `fw-nix` NAR encoding.
pub fn compute_package_nix_store_path(name: &str, content_hash: &str) -> String {
    let safe_name: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '-'
            }
        })
        .collect();
    let entry = compute_fixed_output_store_entry(&safe_name, content_hash.as_bytes(), false);
    entry.store_path
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockedDependency {
    pub repo_url: String,
    pub name: String,
    pub version: String,
    pub package_type: PrimitiveKind,
    pub source: PackageSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_commit: Option<String>,
    pub depth: usize,
    pub deployed_files: Vec<String>,
    pub deployed_file_hashes: BTreeMap<String, String>,
    pub content_hash: String,
    pub nix_store_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declared_license: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockedMcpServer {
    pub name: String,
    pub transport: String,
    pub source_package: String,
    pub depth: usize,
    pub definition_fingerprint: String,
    pub admitted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpOmission {
    pub name: String,
    pub source_package: String,
    pub reason: String,
    pub definition_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApmLockfile {
    pub lockfile_version: String,
    pub apm_version: String,
    pub manifest_sha256: String,
    pub dependencies: Vec<LockedDependency>,
    #[serde(default)]
    pub mcp_servers: Vec<LockedMcpServer>,
    #[serde(default)]
    pub mcp_omissions: Vec<McpOmission>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SbomFormat {
    CycloneDx,
    Spdx,
}

impl ApmLockfile {
    pub fn load_json(path: &Path) -> Result<Self, String> {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| format!("failed to read lockfile {}: {e}", path.display()))?;
        serde_json::from_str(&raw)
            .map_err(|e| format!("failed to parse lockfile {}: {e}", path.display()))
    }

    pub fn save_json(&self, path: &Path) -> Result<(), String> {
        let json_str = serde_json::to_string_pretty(self)
            .map_err(|e| format!("failed to serialize lockfile: {e}"))?;
        std::fs::write(path, format!("{json_str}\n"))
            .map_err(|e| format!("failed to write lockfile {}: {e}", path.display()))
    }

    /// Render an `apm.lock.yaml`-compatible view of this lockfile.
    pub fn to_yaml(&self) -> String {
        let mut out = String::with_capacity(1024);
        let _ = writeln!(out, "lockfile_version: '{}'", self.lockfile_version);
        let _ = writeln!(out, "apm_version: {}", self.apm_version);
        let _ = writeln!(out, "manifest_sha256: {}", self.manifest_sha256);
        out.push_str("dependencies:\n");
        for dep in &self.dependencies {
            let _ = writeln!(out, "- repo_url: {}", dep.repo_url);
            let _ = writeln!(out, "  name: {}", dep.name);
            let _ = writeln!(out, "  version: {}", dep.version);
            let _ = writeln!(out, "  package_type: {}", dep.package_type.as_str());
            let _ = writeln!(out, "  depth: {}", dep.depth);
            out.push_str("  deployed_files:\n");
            for f in &dep.deployed_files {
                let _ = writeln!(out, "  - {f}");
            }
            out.push_str("  deployed_file_hashes:\n");
            for (k, v) in &dep.deployed_file_hashes {
                let _ = writeln!(out, "    {k}: {v}");
            }
            let _ = writeln!(out, "  source: {}", dep.source.as_str());
            if let Some(lp) = &dep.local_path {
                let _ = writeln!(out, "  local_path: {lp}");
            }
            if let Some(gr) = &dep.git_ref {
                let _ = writeln!(out, "  git_ref: {gr}");
            }
            let _ = writeln!(out, "  content_hash: {}", dep.content_hash);
            let _ = writeln!(out, "  nix_store_path: {}", dep.nix_store_path);
            if let Some(lic) = &dep.declared_license {
                let _ = writeln!(out, "  declared_license: {lic}");
            }
        }
        out
    }

    /// Export a CycloneDX 1.5 or SPDX 2.3 JSON SBOM directly from lockfile provenance.
    pub fn export_sbom(&self, project_name: &str, format: SbomFormat) -> Value {
        match format {
            SbomFormat::CycloneDx => {
                let components: Vec<Value> = self
                    .dependencies
                    .iter()
                    .map(|d| {
                        let raw_sha = d
                            .content_hash
                            .strip_prefix("sha256:")
                            .unwrap_or(&d.content_hash);
                        json!({
                            "type": "library",
                            "name": d.name,
                            "version": d.version,
                            "purl": format!("pkg:apm/{}@{}", d.name, d.version),
                            "hashes": [{ "alg": "SHA-256", "content": raw_sha }],
                            "properties": [
                                { "name": "apm:repo_url", "value": d.repo_url },
                                { "name": "apm:package_type", "value": d.package_type.as_str() },
                                { "name": "fw:nix_store_path", "value": d.nix_store_path },
                            ]
                        })
                    })
                    .collect();
                json!({
                    "bomFormat": "CycloneDX",
                    "specVersion": "1.5",
                    "version": 1,
                    "metadata": {
                        "component": { "type": "application", "name": project_name }
                    },
                    "components": components
                })
            }
            SbomFormat::Spdx => {
                let packages: Vec<Value> = self
                    .dependencies
                    .iter()
                    .map(|d| {
                        let raw_sha = d
                            .content_hash
                            .strip_prefix("sha256:")
                            .unwrap_or(&d.content_hash);
                        json!({
                            "SPDXID": format!("SPDXRef-Package-{}", d.name),
                            "name": d.name,
                            "versionInfo": d.version,
                            "downloadLocation": d.repo_url,
                            "licenseDeclared": d.declared_license.as_deref().unwrap_or("NOASSERTION"),
                            "checksums": [{ "algorithm": "SHA256", "checksumValue": raw_sha }]
                        })
                    })
                    .collect();
                json!({
                    "spdxVersion": "SPDX-2.3",
                    "dataLicense": "CC0-1.0",
                    "SPDXID": "SPDXRef-DOCUMENT",
                    "name": project_name,
                    "packages": packages
                })
            }
        }
    }

    /// Compress this lockfile + manifest metadata into a deterministic gzip bundle using `flate2`.
    pub fn pack_gzip_bundle(&self) -> Result<Vec<u8>, String> {
        let payload = serde_json::to_vec(self)
            .map_err(|e| format!("failed to serialize bundle payload: {e}"))?;
        let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
        encoder
            .write_all(&payload)
            .map_err(|e| format!("gzip write failed: {e}"))?;
        encoder
            .finish()
            .map_err(|e| format!("gzip finish failed: {e}"))
    }
}
