use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub rust: Rust,
    pub llvm: Llvm,
    pub cache: Cache,
    pub targets: Vec<Target>,
    pub xwin: Xwin,
    pub cargo_xwin: Tool,
    pub cargo_zigbuild: Tool,
}

#[derive(Debug, Deserialize)]
pub struct Rust {
    pub toolchain_file: String,
    pub dist_server: String,
    pub upstream: String,
    pub fork: String,
    pub host: String,
    pub packages: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct Llvm {
    pub upstream: String,
    pub fork: String,
    pub release_repo: String,
    pub assets: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct Cache {
    pub rclone_remote: String,
    pub bucket: String,
    pub prefix: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Target {
    pub triple: String,
    pub tool: String,
    pub libc: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Xwin {
    pub repo: String,
    pub upstream: String,
    pub tag: String,
    pub rev: Option<String>,
    pub manifest_version: u8,
    pub terms: String,
    pub arch: Vec<String>,
    pub variant: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct Tool {
    pub repo: String,
    pub upstream: String,
    pub tag: String,
}

#[derive(Debug, Deserialize)]
struct ToolchainFile {
    toolchain: ToolchainSection,
}

#[derive(Debug, Deserialize)]
struct ToolchainSection {
    channel: String,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("parse {}", path.display()))
    }

    /// The compiler version: the channel of rust-toolchain.toml, resolved against the repository root.
    pub fn version(&self, root: &Path) -> Result<String> {
        let path = root.join(&self.rust.toolchain_file);
        let text = std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        let file: ToolchainFile = toml::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
        Ok(file.toolchain.channel)
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Lock {
    pub rust: LockRust,
    pub llvm: LockLlvm,
    #[serde(default)]
    pub tool: Vec<LockTool>,
    #[serde(default)]
    pub artifact: Vec<Artifact>,
    #[serde(default)]
    pub xwin_terms: Option<XwinTerms>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct LockRust {
    pub version: String,
    pub commit: String,
    pub fork_has_tag: bool,
    pub manifest_sha256: String,
    pub manifest_date: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct LockLlvm {
    pub repo: String,
    pub branch: String,
    pub commit: String,
    pub version: String,
    pub fork_has_commit: bool,
    #[serde(default)]
    pub asset: Vec<LlvmAsset>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LlvmAsset {
    pub name: String,
    pub url: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LockTool {
    pub name: String,
    pub fork: String,
    pub tag: String,
    pub commit: String,
    pub fork_has_commit: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Artifact {
    pub package: String,
    pub target: String,
    pub url: String,
    pub sha256: String,
}

/// Recorded when the Windows SDK and CRT were splatted: which terms were accepted, for which manifest, when.
#[derive(Debug, Serialize, Deserialize)]
pub struct XwinTerms {
    pub terms: String,
    pub manifest_version: u8,
    pub accepted_unix: u64,
    pub accepted_by_flag: String,
    pub xwin_commit: String,
}

impl Lock {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| format!("read {} (run `fw toolchain pin` first)", path.display()))?;
        toml::from_str(&text).with_context(|| format!("parse {}", path.display()))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let text = format!("# Written by `fw toolchain pin` from cluster/toolchain.toml and rust-toolchain.toml; do not edit.\n{}", toml::to_string_pretty(self)?);
        std::fs::write(path, text).with_context(|| format!("write {}", path.display()))
    }
}
