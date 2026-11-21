use crate::github::{Github, date_days_ago, parallel};
use crate::model::Org;
use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::BTreeMap;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Default)]
pub struct RepoLive {
    pub name: String,
    pub id: u64,
    pub summary: Value,
    /// Full repository object (merge options, security and analysis); read for originals only.
    pub detail: Value,
    pub rulesets: Vec<Value>,
    pub labels: Vec<Value>,
    pub actions: Value,
    pub workflow: Value,
    /// Whether a fork started a run of a watched event inside the window.
    pub recent_runs: bool,
    /// Concatenated workflow files, read for originals to find which secrets they use.
    pub workflow_text: String,
    pub wiki_has_content: Option<bool>,
    pub pages: Value,
}

#[derive(Debug, Clone, Default)]
pub struct Live {
    pub org: Value,
    pub org_actions: Value,
    pub org_workflow: Value,
    pub secrets: Vec<Value>,
    pub secret_repos: BTreeMap<String, Vec<u64>>,
    pub teams: Vec<Value>,
    pub installations: Vec<Value>,
    pub runners: Vec<Value>,
    pub configurations: Vec<Value>,
    /// configuration id -> repository name -> attachment status
    pub attachments: BTreeMap<u64, BTreeMap<String, String>>,
    pub defaults: Value,
    pub packages: Vec<Value>,
    /// Why packages cannot be listed (a token without read:packages).
    pub packages_refused: Option<String>,
    pub admins: Vec<Value>,
    pub invitations: Vec<Value>,
    pub outside_collaborators: Vec<Value>,
    pub repos: Vec<RepoLive>,
    /// Names of workflow-less reasons a read was refused, for `check` to report.
    pub refused: BTreeMap<String, String>,
}

impl Live {
    pub fn load(github: &Github, org: &Org) -> Result<Self> {
        let name = &org.org.name;
        let mut live = Self::default();
        live.org = github.get(&format!("/orgs/{name}"))?;
        live.org_actions = github.get(&format!("/orgs/{name}/actions/permissions"))?;
        live.org_workflow = github.get(&format!("/orgs/{name}/actions/permissions/workflow"))?;
        live.secrets = github.paged(&format!("/orgs/{name}/actions/secrets"), Some("secrets"))?;
        for secret in &live.secrets {
            if secret["visibility"] == "selected" {
                let secret_name = secret["name"].as_str().unwrap_or_default();
                let rows = github.paged(&format!("/orgs/{name}/actions/secrets/{secret_name}/repositories"), Some("repositories"))?;
                live.secret_repos.insert(secret_name.to_string(), rows.iter().filter_map(|row| row["id"].as_u64()).collect());
            }
        }
        live.teams = github.paged(&format!("/orgs/{name}/teams"), None)?;
        live.installations = github.paged(&format!("/orgs/{name}/installations"), Some("installations"))?;
        live.runners = github.paged(&format!("/orgs/{name}/actions/runners"), Some("runners"))?;
        live.configurations = github.paged(&format!("/orgs/{name}/code-security/configurations"), None)?;
        for configuration in live.configurations.iter().filter(|row| row["target_type"] == "organization") {
            let Some(id) = configuration["id"].as_u64() else { continue };
            let rows = github.paged(&format!("/orgs/{name}/code-security/configurations/{id}/repositories"), None)?;
            let map = rows.iter().filter_map(|row| Some((row["repository"]["name"].as_str()?.to_string(), row["status"].as_str().unwrap_or_default().to_string()))).collect();
            live.attachments.insert(id, map);
        }
        live.defaults = github.get(&format!("/orgs/{name}/code-security/configurations/defaults"))?;
        live.admins = github.paged(&format!("/orgs/{name}/members?role=admin"), None)?;
        live.invitations = github.paged(&format!("/orgs/{name}/invitations"), None)?;
        live.outside_collaborators = github.paged(&format!("/orgs/{name}/outside_collaborators"), None)?;
        for kind in ["npm", "container", "maven", "nuget", "rubygems"] {
            match github.try_get(&format!("/orgs/{name}/packages?package_type={kind}&per_page=100")) {
                Ok(Value::Array(rows)) => live.packages.extend(rows),
                Ok(_) => {}
                Err(reason) if reason.starts_with("HTTP 400") => live.refused.insert(format!("packages {kind}"), reason).map_or((), |_| ()),
                Err(reason) => {
                    live.packages_refused = Some(reason);
                    break;
                }
            }
        }
        let list = github.paged(&format!("/orgs/{name}/repos?type=all"), None)?;
        let window_start = date_days_ago(org.org.fork_actions_window_days);
        let loaded: Vec<Result<RepoLive>> = parallel(&list, 8, |summary| load_repo(github, org, summary, &window_start));
        for repo in loaded {
            live.repos.push(repo?);
        }
        Ok(live)
    }

    pub fn repo(&self, name: &str) -> Option<&RepoLive> {
        self.repos.iter().find(|repo| repo.name == name)
    }

    pub fn configuration(&self, name: &str) -> Option<&Value> {
        self.configurations.iter().find(|row| row["name"] == name && row["target_type"] == "organization")
    }

    /// The organization configuration a repository is attached to, by name.
    pub fn attached_configuration(&self, repo: &str) -> Option<String> {
        for (id, map) in &self.attachments {
            if map.get(repo).is_some_and(|status| status == "attached" || status == "enforced") {
                return self.configurations.iter().find(|row| row["id"].as_u64() == Some(*id)).and_then(|row| row["name"].as_str().map(str::to_string));
            }
        }
        None
    }
}

fn load_repo(github: &Github, org: &Org, summary: &Value, window_start: &str) -> Result<RepoLive> {
    let owner = &org.org.name;
    let name = summary["name"].as_str().context("repository without a name")?.to_string();
    let is_fork = summary["fork"].as_bool() == Some(true);
    let archived = summary["archived"].as_bool() == Some(true);
    let mut repo = RepoLive { name: name.clone(), id: summary["id"].as_u64().unwrap_or_default(), summary: summary.clone(), ..RepoLive::default() };
    if archived {
        return Ok(repo);
    }
    let base = format!("/repos/{owner}/{name}");
    repo.actions = github.get(&format!("{base}/actions/permissions"))?;
    if !is_fork {
        repo.detail = github.get(&base)?;
        repo.workflow = github.get(&format!("{base}/actions/permissions/workflow"))?;
        repo.labels = github.paged(&format!("{base}/labels"), None)?;
        let summaries = github.try_get(&format!("{base}/rulesets")).unwrap_or(Value::Array(Vec::new()));
        for row in summaries.as_array().into_iter().flatten() {
            if let Some(id) = row["id"].as_u64() {
                repo.rulesets.push(github.get(&format!("{base}/rulesets/{id}"))?);
            }
        }
        if let Some(files) = github.try_get(&format!("{base}/contents/.github/workflows")).ok().and_then(|value| value.as_array().cloned()) {
            for file in files.iter().filter_map(|file| file["path"].as_str()).filter(|path| path.ends_with(".yml") || path.ends_with(".yaml")) {
                if let Some(text) = github.get_text(&format!("{base}/contents/{file}"))? {
                    repo.workflow_text.push_str(&text);
                    repo.workflow_text.push('\n');
                }
            }
        }
        if summary["has_pages"].as_bool() == Some(true) {
            repo.pages = github.try_get(&format!("{base}/pages")).unwrap_or(Value::Null);
        }
    } else if repo.actions["enabled"].as_bool() == Some(true) {
        let runs = github.try_get(&format!("{base}/actions/runs?per_page=30&created=%3E%3D{window_start}")).unwrap_or(Value::Null);
        repo.recent_runs = runs["workflow_runs"].as_array().into_iter().flatten().any(|run| run["event"].as_str().is_some_and(|event| org.org.fork_actions_events.iter().any(|watched| watched == event)));
    }
    if summary["has_wiki"].as_bool() == Some(true) && summary["private"].as_bool() != Some(true) {
        repo.wiki_has_content = wiki_has_content(owner, &name);
    }
    Ok(repo)
}

/// A wiki with pages answers `git ls-remote`; an enabled wiki without pages does not exist as a repository.
fn wiki_has_content(owner: &str, name: &str) -> Option<bool> {
    let output = Command::new("git").args(["ls-remote", &format!("https://github.com/{owner}/{name}.wiki.git")]).env("GIT_TERMINAL_PROMPT", "0").stderr(Stdio::null()).output().ok()?;
    Some(output.status.success() && !output.stdout.is_empty())
}
