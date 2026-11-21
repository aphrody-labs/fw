//! Gateway configuration: one TOML file, secrets named by environment variable and read on the host only.

use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub gateway: Settings,
    #[serde(default, rename = "provider")]
    pub providers: Vec<ProviderConfig>,
}

/// What `tools/list` shows an agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Expose {
    /// The three meta-tools only: the context cost does not depend on how many tools are registered.
    #[default]
    Meta,
    /// Every registered tool as a first-level typed tool.
    Typed,
    Both,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub expose: Expose,
    pub search_limit: usize,
    /// Weight of description words against tag words in `search_tools` (`TagSearchStrategy`).
    pub description_weight: f64,
    /// Tool results longer than this are cut, with the original size stated in the text.
    pub max_result_bytes: usize,
    /// Descriptions in `search_tools` answers are cut to this many characters.
    pub max_description_chars: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self { expose: Expose::Meta, search_limit: 8, description_weight: 0.3, max_result_bytes: 65_536, max_description_chars: 200 }
    }
}

/// A secret that never leaves the host: the name of the environment variable holding it, and where it goes.
#[derive(Debug, Clone, Deserialize)]
pub struct Secret {
    pub env: String,
    /// Header that carries it, `Authorization` when absent.
    pub header: Option<String>,
    /// Prefix before the value, `Bearer` when absent; an empty string sends the raw value.
    pub scheme: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ProviderConfig {
    /// An API that publishes OpenAPI: every operation becomes a UTCP tool called directly over HTTP.
    Openapi {
        name: String,
        /// URL or file path of the OpenAPI document.
        spec: String,
        /// Replaces the server URL declared in the document.
        base_url: Option<String>,
        auth: Option<Secret>,
        /// Only operations carrying one of these tags are registered; all when empty.
        #[serde(default)]
        tags: Vec<String>,
    },
    /// An existing MCP server, consumed through the rmcp client.
    Mcp {
        name: String,
        url: Option<String>,
        command: Option<String>,
        #[serde(default)]
        args: Vec<String>,
        /// Host environment variables forwarded to the child process, by name.
        #[serde(default)]
        env: Vec<String>,
        auth: Option<Secret>,
    },
    /// A UTCP manual (JSON with a `tools` array). The protocols its tools may use are listed in `allow`; the
    /// provider's own is the default, so a third-party manual cannot reach `cli` or `text`.
    Manual {
        name: String,
        source: String,
        #[serde(default)]
        allow: Vec<String>,
        auth: Option<Secret>,
    },
}

impl ProviderConfig {
    pub fn name(&self) -> &str {
        match self {
            Self::Openapi { name, .. } | Self::Mcp { name, .. } | Self::Manual { name, .. } => name,
        }
    }
}

impl Config {
    pub fn parse(text: &str) -> anyhow::Result<Self> {
        let config: Self = toml::from_str(text)?;
        let mut seen = std::collections::HashSet::new();
        for provider in &config.providers {
            let name = provider.name();
            if name.is_empty() || name.contains('.') {
                anyhow::bail!("provider name {name:?} must be non-empty and contain no dot: UTCP prefixes tools with `name.`");
            }
            if !seen.insert(name.to_string()) {
                anyhow::bail!("provider name {name:?} is declared twice");
            }
        }
        Ok(config)
    }

    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path).map_err(|error| anyhow::anyhow!("{}: {error}", path.display()))?;
        Self::parse(&text)
    }
}
