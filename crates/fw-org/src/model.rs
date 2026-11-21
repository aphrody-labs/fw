use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::Path;

/// The declaration. Every `settings`, `profile` and `security` table is sent to GitHub as it is written: its keys are the API field names,
/// so a new GitHub setting needs a new line in the file and no new code.
#[derive(Debug, Clone, Deserialize)]
pub struct Org {
    pub org: Meta,
    /// Fields of `PATCH /orgs/{org}`.
    pub profile: toml::Table,
    pub actions: Actions,
    pub class: Classes,
    #[serde(default)]
    pub security: BTreeMap<String, SecurityConfiguration>,
    #[serde(default)]
    pub secret: BTreeMap<String, Secret>,
    #[serde(default)]
    pub labels: Vec<Label>,
    #[serde(default)]
    pub labels_rename: BTreeMap<String, String>,
    #[serde(default)]
    pub ruleset: BTreeMap<String, Ruleset>,
    #[serde(default)]
    pub team: BTreeMap<String, Team>,
    #[serde(default)]
    pub repo: BTreeMap<String, RepoDeclaration>,
    #[serde(default)]
    pub topic_by_kind: BTreeMap<String, String>,
    #[serde(default)]
    pub practice: BTreeMap<String, Practice>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Meta {
    pub name: String,
    pub user: String,
    pub token_env: String,
    pub plan_file: String,
    /// Identity file of the stack: `[repos]` gives the kind of every product repository, `[retired]` the repositories to archive.
    pub identity_file: String,
    /// Forks whose Actions stay enabled when a run of these events started within `fork_actions_window_days`.
    pub fork_actions_events: Vec<String>,
    pub fork_actions_window_days: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Actions {
    /// Fields of `PUT /orgs/{org}/actions/permissions/workflow`.
    pub workflow: toml::Table,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Classes {
    pub original: Class,
    pub fork: Class,
    pub private: Class,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Class {
    /// Fields of `PATCH /repos/{owner}/{repo}`.
    pub settings: toml::Table,
    /// Key of `[security.*]` attached to the repositories of this class; empty for none.
    #[serde(default)]
    pub security: String,
    #[serde(default)]
    pub labels: bool,
    #[serde(default)]
    pub rulesets: Vec<String>,
    /// Repository Actions switch; `false` keeps it only where the fork still runs its own workflows.
    #[serde(default)]
    pub actions_enabled: Option<bool>,
    /// Fields of `PUT /repos/{owner}/{repo}/actions/permissions/workflow`.
    #[serde(default)]
    pub workflow: toml::Table,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SecurityConfiguration {
    pub description: String,
    /// Fields of `POST /orgs/{org}/code-security/configurations`.
    pub settings: toml::Table,
    /// `all`, `public`, `private_or_internal` or `none`: the default for new repositories.
    #[serde(default)]
    pub default_for_new_repos: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Secret {
    /// Repositories that need the secret are found by reading `secrets.<NAME>` and `secrets: inherit` in their workflows.
    pub visibility: String,
    #[serde(default)]
    pub extra_repos: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Label {
    pub name: String,
    pub color: String,
    pub description: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Ruleset {
    pub description: String,
    pub target: String,
    pub enforcement: String,
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Rules as the API takes them: `type` and optional `parameters`.
    pub rules: Vec<toml::Table>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Team {
    pub description: String,
    pub privacy: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RepoDeclaration {
    pub description: Option<String>,
    pub homepage: Option<String>,
    /// Added to the live topics; a topic is never removed.
    #[serde(default)]
    pub topics: Vec<String>,
    /// Overrides of the class settings, for example `has_wiki = true` on a repository whose wiki is the documentation.
    #[serde(default)]
    pub settings: toml::Table,
    #[serde(default)]
    pub discussions: bool,
    /// Key of the `[security.*]` configuration to attach instead of the class one (aggregated trees that Dependabot cannot update).
    pub security: Option<String>,
    /// Rulesets of this repository instead of the class ones.
    pub rulesets: Option<Vec<String>>,
    /// A mirror of an upstream tree at a pinned branch: no default-branch, license or homepage findings.
    #[serde(default)]
    pub mirror: bool,
    /// Keeps Actions enabled on a fork regardless of its recent runs.
    #[serde(default)]
    pub actions: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Practice {
    pub source: String,
}

impl Org {
    pub fn load(path: &Path) -> Result<(Self, String)> {
        let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        let org: Self = toml::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
        for (name, ruleset) in &org.ruleset {
            if !matches!(ruleset.target.as_str(), "branch" | "tag") {
                return Err(anyhow!("ruleset {name}: target must be branch or tag"));
            }
        }
        for class in [&org.class.original, &org.class.fork, &org.class.private] {
            for key in &class.rulesets {
                if !org.ruleset.contains_key(key) {
                    return Err(anyhow!("a class names ruleset {key} which is not declared"));
                }
            }
            if !class.security.is_empty() && !org.security.contains_key(&class.security) {
                return Err(anyhow!("a class names security configuration {} which is not declared", class.security));
            }
        }
        Ok((org, text))
    }

    pub fn class_of(&self, repo: &Value) -> (&'static str, &Class) {
        if repo["private"].as_bool() == Some(true) {
            ("private", &self.class.private)
        } else if repo["fork"].as_bool() == Some(true) {
            ("fork", &self.class.fork)
        } else {
            ("original", &self.class.original)
        }
    }
}

pub fn table_to_json(table: &toml::Table) -> Map<String, Value> {
    match serde_json::to_value(table) {
        Ok(Value::Object(map)) => map,
        _ => Map::new(),
    }
}

/// Keys of `want` whose value differs from the live object; a key the live object lacks counts as different.
pub fn changed_fields(want: &Map<String, Value>, live: &Value) -> Map<String, Value> {
    want.iter().filter(|(key, value)| live.get(key.as_str()) != Some(*value)).map(|(key, value)| (key.clone(), value.clone())).collect()
}

pub fn describe(fields: &Map<String, Value>, live: &Value) -> String {
    fields.iter().map(|(key, value)| format!("{key} {} -> {value}", live.get(key.as_str()).map(Value::to_string).unwrap_or_else(|| "unset".into()))).collect::<Vec<_>>().join(", ")
}
