//! Configuration, FX_* environment overrides, Skills loader, and AGENTS.md discovery.
//! Matches `vercel-labs/fx/scripts/cli_shape_registry.json` & `src/core/config/`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const FX_VERSION: &str = "0.0.17";
pub const DEFAULT_MODEL: &str = "anthropic/claude-sonnet-4.5";
pub const DEFAULT_GATEWAY_BASE_URL: &str = "https://ai-gateway.vercel.sh";
pub const DEFAULT_GATEWAY_CHAT_URL: &str = "https://ai-gateway.vercel.sh/v1/chat/completions";
pub const DEFAULT_MAX_AGENT_STEPS: u32 = 100;
pub const DEFAULT_AUTO_COMPACT_PERCENT: u8 = 85;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FxConfig {
    pub fx_home: PathBuf,
    pub model: String,
    pub provider: String,
    pub provider_order: Vec<String>,
    pub provider_strict: bool,
    pub effort: String,
    pub fast: bool,
    pub ultrafast: bool,
    pub permission_mode: String,
    pub auto_compact_percent: u8,
    pub max_agent_steps: u32,
    pub gateway_base_url: String,
    pub gateway_chat_url: String,
    pub review_model: Option<String>,
    pub disable_keychain: bool,
    pub no_open_browser: bool,
    pub auto_upgrade: bool,
    pub sound: bool,
    pub api_key_present: bool,
    pub configured_providers_json: Option<String>,
}

impl FxConfig {
    pub fn from_env() -> Self {
        let fx_home = std::env::var("FX_HOME")
            .ok()
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(default_fx_home);

        let model = env_or("FX_MODEL", DEFAULT_MODEL);
        let provider = env_or("FX_PROVIDER", "gateway");
        let provider_order = std::env::var("FX_PROVIDER_ORDER")
            .ok()
            .map(|s| {
                s.split(',')
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>()
            })
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| vec!["gateway".to_owned(), "codex".to_owned(), "grok".to_owned()]);

        let auto_compact_percent = std::env::var("FX_AUTO_COMPACT_PERCENT")
            .ok()
            .and_then(|v| v.parse::<u8>().ok())
            .map(|v| v.clamp(20, 99))
            .unwrap_or(DEFAULT_AUTO_COMPACT_PERCENT);

        let max_agent_steps = std::env::var("FX_MAX_AGENT_STEPS")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(DEFAULT_MAX_AGENT_STEPS);

        let api_key_present = std::env::var("AI_GATEWAY_API_KEY")
            .ok()
            .is_some_and(|v| !v.is_empty());

        Self {
            fx_home,
            model,
            provider,
            provider_order,
            provider_strict: env_bool("FX_PROVIDER_STRICT", false),
            effort: env_or("FX_EFFORT", "medium"),
            fast: env_bool("FX_FAST", false),
            ultrafast: env_bool("FX_ULTRAFAST", false),
            permission_mode: env_or("FX_PERMISSION_MODE", "auto"),
            auto_compact_percent,
            max_agent_steps,
            gateway_base_url: env_or("FX_GATEWAY_BASE_URL", DEFAULT_GATEWAY_BASE_URL),
            gateway_chat_url: env_or("FX_GATEWAY_CHAT_URL", DEFAULT_GATEWAY_CHAT_URL),
            review_model: std::env::var("FX_REVIEW_MODEL").ok().filter(|v| !v.is_empty()),
            disable_keychain: env_bool("FX_DISABLE_KEYCHAIN", false),
            no_open_browser: env_bool("FX_NO_OPEN_BROWSER", false),
            auto_upgrade: env_bool("FX_AUTO_UPGRADE", true),
            sound: env_bool("FX_SOUND", false),
            api_key_present,
            configured_providers_json: std::env::var("FX_CONFIGURED_PROVIDERS")
                .ok()
                .filter(|v| !v.is_empty()),
        }
    }

    pub fn sessions_dir(&self) -> PathBuf {
        self.fx_home.join("sessions")
    }

    pub fn config_file(&self) -> PathBuf {
        self.fx_home.join("config.json")
    }
}

fn default_fx_home() -> PathBuf {
    if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        PathBuf::from(home).join(".fx")
    } else {
        PathBuf::from(".fx")
    }
}

fn env_or(key: &str, fallback: &str) -> String {
    std::env::var(key)
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| fallback.to_owned())
}

fn env_bool(key: &str, fallback: bool) -> bool {
    match std::env::var(key).ok().as_deref() {
        Some("1" | "true" | "TRUE" | "yes" | "on") => true,
        Some("0" | "false" | "FALSE" | "no" | "off") => false,
        _ => fallback,
    }
}

/// Skill metadata loaded lazily (name + description only, full body on demand).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillEntry {
    pub name: String,
    pub description: String,
    pub source_dir: String,
    pub path: PathBuf,
}

/// Discover skills across `.fx/skills`, `~/.fx/skills`, `.claude/skills`, `.agents/skills`.
pub fn discover_skills(workspace: &Path, fx_home: &Path) -> Vec<SkillEntry> {
    let mut dirs = vec![
        workspace.join(".fx").join("skills"),
        workspace.join(".claude").join("skills"),
        workspace.join(".agents").join("skills"),
        fx_home.join("skills"),
    ];
    if let Some(parent) = fx_home.parent() {
        dirs.push(parent.join(".claude").join("skills"));
        dirs.push(parent.join(".agents").join("skills"));
    }

    let mut by_name: BTreeMap<String, SkillEntry> = BTreeMap::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let skill_md = entry.path().join("SKILL.md");
            if !skill_md.is_file() {
                continue;
            }
            let Ok(raw) = std::fs::read_to_string(&skill_md) else {
                continue;
            };
            let (name, desc) = parse_skill_frontmatter(&raw, &entry.file_name().to_string_lossy());
            by_name.entry(name.clone()).or_insert(SkillEntry {
                name,
                description: desc,
                source_dir: dir.display().to_string(),
                path: skill_md,
            });
        }
    }
    by_name.into_values().collect()
}

fn parse_skill_frontmatter(raw: &str, fallback_name: &str) -> (String, String) {
    let mut name = fallback_name.to_owned();
    let mut desc = String::new();
    let trimmed = raw.trim_start();
    if let Some(rest) = trimmed.strip_prefix("---")
        && let Some(end) = rest.find("\n---")
    {
        for line in rest[..end].lines() {
            if let Some(v) = line.strip_prefix("name:") {
                let v = v.trim().trim_matches('"').trim_matches('\'');
                if !v.is_empty() {
                    name = v.to_owned();
                }
            } else if let Some(v) = line.strip_prefix("description:") {
                let v = v.trim().trim_matches('"').trim_matches('\'');
                if !v.is_empty() {
                    desc = v.to_owned();
                }
            }
        }
    }
    (name, desc)
}

/// Discover `AGENTS.md` files (global `~/.fx/AGENTS.md` + workspace root).
pub fn discover_agents_md(workspace: &Path, fx_home: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let global = fx_home.join("AGENTS.md");
    if global.is_file() {
        out.push(global);
    }
    let local = workspace.join("AGENTS.md");
    if local.is_file() {
        out.push(local);
    }
    out
}
