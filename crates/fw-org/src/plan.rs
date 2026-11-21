use crate::github::Github;
use crate::live::{Live, RepoLive};
use crate::model::{Org, RepoDeclaration, changed_fields, describe, table_to_json};
use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Action {
    pub op: String,
    pub kind: String,
    pub scope: String,
    pub target: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub schema: u32,
    pub org_sha256: String,
    pub actions: Vec<Action>,
}

/// One API request. `capture` stores the `id` of the response under that key; `{key}` in a later path is replaced by it.
#[derive(Debug, Clone)]
pub struct Call {
    pub method: &'static str,
    pub path: String,
    pub body: Option<Value>,
    pub capture: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Step {
    pub action: Action,
    pub calls: Vec<Call>,
}

/// What the plan reads from the identity file of the stack (`ay/ay.toml`): kinds of the product repositories, repositories to retire and the root domain.
#[derive(Debug, Clone, Default)]
pub struct Identity {
    pub domain_root: Option<String>,
    pub kinds: BTreeMap<String, String>,
    pub retired: BTreeMap<String, String>,
}

impl Identity {
    pub fn load(path: &Path, owner: &str) -> Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        let value: toml::Value = toml::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
        let split = |slug: &str| slug.split_once('/').map(|(slug_owner, name)| (slug_owner.to_string(), name.to_string()));
        let mut identity = Self { domain_root: value.get("domains").and_then(|domains| domains.get("root")).and_then(toml::Value::as_str).map(str::to_string), ..Self::default() };
        for (_, entry) in value.get("repos").and_then(toml::Value::as_table).into_iter().flatten() {
            if let (Some((slug_owner, name)), Some(kind)) = (entry.get("slug").and_then(toml::Value::as_str).and_then(split), entry.get("kind").and_then(toml::Value::as_str))
                && slug_owner == owner
            {
                identity.kinds.insert(name, kind.to_string());
            }
        }
        for (_, entry) in value.get("retired").and_then(toml::Value::as_table).into_iter().flatten() {
            if let Some((slug_owner, name)) = entry.get("slug").and_then(toml::Value::as_str).and_then(split)
                && slug_owner == owner
            {
                identity.retired.insert(name, entry.get("reason").and_then(toml::Value::as_str).unwrap_or_default().to_string());
            }
        }
        Ok(identity)
    }
}

pub fn org_sha256(text: &str) -> String {
    Sha256::digest(text.as_bytes()).iter().map(|byte| format!("{byte:02x}")).collect()
}

fn step(op: &str, kind: &str, scope: &str, target: &str, detail: String, calls: Vec<Call>) -> Step {
    Step { action: Action { op: op.into(), kind: kind.into(), scope: scope.into(), target: target.into(), detail }, calls }
}

fn call(method: &'static str, path: String, body: Option<Value>) -> Call {
    Call { method, path, body, capture: None }
}

pub fn compute(org: &Org, org_text: &str, live: &Live, identity: &Identity) -> Result<(Plan, Vec<Step>)> {
    let owner = org.org.name.as_str();
    let mut steps = Vec::new();

    let profile = table_to_json(&org.profile);
    let changed = changed_fields(&profile, &live.org);
    if !changed.is_empty() {
        steps.push(step("update", "org-profile", owner, owner, describe(&changed, &live.org), vec![call("PATCH", format!("/orgs/{owner}"), Some(Value::Object(changed)))]));
    }
    let workflow = table_to_json(&org.actions.workflow);
    let changed = changed_fields(&workflow, &live.org_workflow);
    if !changed.is_empty() {
        steps.push(step("update", "org-actions", owner, "workflow permissions", describe(&changed, &live.org_workflow), vec![call("PUT", format!("/orgs/{owner}/actions/permissions/workflow"), Some(Value::Object(changed)))]));
    }

    security_configurations(org, live, &mut steps)?;
    secrets(org, live, &mut steps)?;
    archive(org, live, identity, &mut steps);

    for repo in &live.repos {
        if repo.summary["archived"].as_bool() == Some(true) || identity.retired.contains_key(&repo.name) {
            continue;
        }
        repo_steps(org, live, identity, repo, &mut steps)?;
    }

    let rank = |op: &str| match op {
        "archive" | "delete" => 0,
        "update" => 1,
        _ => 2,
    };
    steps.sort_by(|left, right| {
        let (left, right) = (&left.action, &right.action);
        (rank(&left.op), &left.kind, &left.scope, &left.target, &left.detail).cmp(&(rank(&right.op), &right.kind, &right.scope, &right.target, &right.detail))
    });
    let plan = Plan { schema: 1, org_sha256: org_sha256(org_text), actions: steps.iter().map(|step| step.action.clone()).collect() };
    Ok((plan, steps))
}

fn security_configurations(org: &Org, live: &Live, steps: &mut Vec<Step>) -> Result<()> {
    let owner = org.org.name.as_str();
    for (key, declared) in &org.security {
        let mut want = table_to_json(&declared.settings);
        want.insert("name".into(), json!(key));
        want.insert("description".into(), json!(declared.description));
        match live.configuration(key) {
            None => {
                let mut create = call("POST", format!("/orgs/{owner}/code-security/configurations"), Some(Value::Object(want)));
                create.capture = Some(key.clone());
                let mut calls = vec![create];
                if let Some(scope) = &declared.default_for_new_repos {
                    calls.push(call("PUT", format!("/orgs/{owner}/code-security/configurations/{{{key}}}/defaults"), Some(json!({ "default_for_new_repos": scope }))));
                }
                steps.push(step("create", "security-configuration", owner, key, declared.description.clone(), calls));
            }
            Some(existing) => {
                let id = existing["id"].as_u64().unwrap_or_default();
                let changed = changed_fields(&want, existing);
                if !changed.is_empty() {
                    steps.push(step("update", "security-configuration", owner, key, describe(&changed, existing), vec![call("PATCH", format!("/orgs/{owner}/code-security/configurations/{id}"), Some(Value::Object(changed)))]));
                }
                if let Some(scope) = &declared.default_for_new_repos {
                    let current = live.defaults.as_array().into_iter().flatten().any(|row| row["configuration"]["id"].as_u64() == Some(id) && row["default_for_new_repos"] == scope.as_str());
                    if !current {
                        steps.push(step("update", "security-default", owner, key, format!("default for new {scope} repositories"), vec![call("PUT", format!("/orgs/{owner}/code-security/configurations/{id}/defaults"), Some(json!({ "default_for_new_repos": scope })))]));
                    }
                }
            }
        }
    }
    Ok(())
}

/// Repositories that use a secret: those whose workflows read `secrets.NAME`, pass `secrets: inherit`, or are listed by hand.
pub fn secret_users<'a>(live: &'a Live, name: &str, extra: &[String]) -> Vec<&'a RepoLive> {
    let needle = format!("secrets.{name}");
    let mut users: Vec<&RepoLive> = live.repos.iter().filter(|repo| repo.summary["archived"].as_bool() != Some(true)).filter(|repo| repo.summary["fork"].as_bool() != Some(true)).filter(|repo| repo.summary["private"].as_bool() != Some(true)).filter(|repo| repo.workflow_text.contains(&needle) || repo.workflow_text.contains("secrets: inherit") || extra.contains(&repo.name)).collect();
    users.sort_by(|left, right| left.name.cmp(&right.name));
    users
}

fn secrets(org: &Org, live: &Live, steps: &mut Vec<Step>) -> Result<()> {
    let owner = org.org.name.as_str();
    for (name, declared) in &org.secret {
        let Some(existing) = live.secrets.iter().find(|row| row["name"] == name.as_str()) else { continue };
        if declared.visibility != "selected" {
            if existing["visibility"] != declared.visibility.as_str() {
                bail!("secret {name}: only visibility selected can be applied without its value");
            }
            continue;
        }
        let users = secret_users(live, name, &declared.extra_repos);
        let ids: BTreeSet<u64> = users.iter().map(|repo| repo.id).collect();
        let names = users.iter().map(|repo| repo.name.as_str()).collect::<Vec<_>>().join(", ");
        if existing["visibility"] != "selected" {
            let body = json!({ "visibility": "selected", "selected_repository_ids": ids });
            steps.push(step("update", "secret-visibility", owner, name, format!("visibility {} -> selected: {names}", existing["visibility"].as_str().unwrap_or("?")), vec![call("PUT", format!("/orgs/{owner}/actions/secrets/{name}"), Some(body))]));
        } else if live.secret_repos.get(name).map(|list| list.iter().copied().collect::<BTreeSet<u64>>()).as_ref() != Some(&ids) {
            steps.push(step("update", "secret-repositories", owner, name, format!("repositories: {names}"), vec![call("PUT", format!("/orgs/{owner}/actions/secrets/{name}/repositories"), Some(json!({ "selected_repository_ids": ids })))]));
        }
    }
    Ok(())
}

fn archive(org: &Org, live: &Live, identity: &Identity, steps: &mut Vec<Step>) {
    let owner = org.org.name.as_str();
    for (name, reason) in &identity.retired {
        let Some(repo) = live.repo(name) else { continue };
        if repo.summary["archived"].as_bool() != Some(true) {
            steps.push(step("archive", "repository", owner, name, reason.clone(), vec![call("PATCH", format!("/repos/{owner}/{name}"), Some(json!({ "archived": true })))]));
        }
    }
}

fn repo_steps(org: &Org, live: &Live, identity: &Identity, repo: &RepoLive, steps: &mut Vec<Step>) -> Result<()> {
    let owner = org.org.name.as_str();
    let name = repo.name.as_str();
    let base = format!("/repos/{owner}/{name}");
    let (class_name, class) = org.class_of(&repo.summary);
    let is_fork = class_name == "fork";
    let declaration = org.repo.get(name).cloned().unwrap_or_default();
    let current = if is_fork { &repo.summary } else { &repo.detail };

    let mut want = table_to_json(&class.settings);
    want.extend(table_to_json(&declaration.settings));
    if declaration.discussions {
        want.insert("has_discussions".into(), json!(true));
    }
    if let Some(description) = &declaration.description {
        want.insert("description".into(), json!(description));
    }
    if let Some(homepage) = &declaration.homepage {
        want.insert("homepage".into(), json!(homepage));
    }
    if is_fork {
        want.retain(|key, _| current.get(key.as_str()).is_some());
    }
    let mut changed = changed_fields(&want, current);
    if changed.get("has_issues") == Some(&json!(false)) && current["open_issues_count"].as_u64().unwrap_or_default() > 0 {
        changed.remove("has_issues");
    }
    if changed.get("has_wiki") == Some(&json!(false)) && repo.wiki_has_content != Some(false) {
        changed.remove("has_wiki");
    }
    if !changed.is_empty() {
        steps.push(step("update", "repository", class_name, name, describe(&changed, current), vec![call("PATCH", base.clone(), Some(Value::Object(changed)))]));
    }

    let mut topics: BTreeSet<String> = repo.summary["topics"].as_array().into_iter().flatten().filter_map(|topic| topic.as_str().map(str::to_string)).collect();
    let before = topics.clone();
    topics.extend(declaration.topics.iter().cloned());
    if let Some(topic) = identity.kinds.get(name).and_then(|kind| org.topic_by_kind.get(kind)) {
        topics.insert(topic.clone());
    }
    if topics != before {
        let added: Vec<&String> = topics.difference(&before).collect();
        steps.push(step("update", "topics", class_name, name, format!("add {}", added.iter().map(|topic| topic.as_str()).collect::<Vec<_>>().join(", ")), vec![call("PUT", format!("{base}/topics"), Some(json!({ "names": topics })))]));
    }

    let security = declaration_security(&declaration, &class.security);
    if !security.is_empty() && live.attached_configuration(name).as_deref() != Some(security) {
        let config_ref = match live.configuration(security).and_then(|row| row["id"].as_u64()) {
            Some(id) => id.to_string(),
            None => format!("{{{security}}}"),
        };
        steps.push(step("update", "security-attach", class_name, name, format!("attach {security}"), vec![call("POST", format!("/orgs/{owner}/code-security/configurations/{config_ref}/attach"), Some(json!({ "scope": "selected", "selected_repository_ids": [repo.id] })))]));
    }

    let enabled_now = repo.actions["enabled"].as_bool().unwrap_or(true);
    match class.actions_enabled {
        Some(false) if is_fork => {
            let keep = declaration.actions == Some(true) || repo.recent_runs;
            if enabled_now && !keep {
                steps.push(step("update", "actions", class_name, name, "enabled true -> false (no run of a watched event in the window)".into(), vec![call("PUT", format!("{base}/actions/permissions"), Some(json!({ "enabled": false })))]));
            }
        }
        Some(true) if !enabled_now => {
            steps.push(step("update", "actions", class_name, name, "enabled false -> true".into(), vec![call("PUT", format!("{base}/actions/permissions"), Some(json!({ "enabled": true })))]));
        }
        _ => {}
    }
    if !is_fork && enabled_now {
        let wanted = table_to_json(&class.workflow);
        let changed = changed_fields(&wanted, &repo.workflow);
        if !changed.is_empty() {
            steps.push(step("update", "actions-workflow", class_name, name, describe(&changed, &repo.workflow), vec![call("PUT", format!("{base}/actions/permissions/workflow"), Some(Value::Object(changed)))]));
        }
    }

    if class.labels {
        label_steps(org, repo, class_name, &base, steps);
    }
    if !is_fork {
        ruleset_steps(org, repo, class_name, &base, declaration.rulesets.as_deref().unwrap_or(&class.rulesets), steps);
    }
    Ok(())
}

fn declaration_security<'a>(declaration: &'a RepoDeclaration, class: &'a str) -> &'a str {
    declaration.security.as_deref().unwrap_or(class)
}

fn encode(segment: &str) -> String {
    segment.bytes().map(|byte| if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) { (byte as char).to_string() } else { format!("%{byte:02X}") }).collect()
}

fn label_steps(org: &Org, repo: &RepoLive, class_name: &str, base: &str, steps: &mut Vec<Step>) {
    let name = repo.name.as_str();
    let find = |wanted: &str| repo.labels.iter().find(|label| label["name"].as_str().is_some_and(|live| live.eq_ignore_ascii_case(wanted)));
    for (old, new) in &org.labels_rename {
        if let (Some(existing), None) = (find(old), find(new))
            && let Some(live_name) = existing["name"].as_str()
        {
            steps.push(step("update", "label", class_name, name, format!("rename {live_name} -> {new}"), vec![call("PATCH", format!("{base}/labels/{}", encode(live_name)), Some(json!({ "new_name": new })))]));
        }
    }
    for label in &org.labels {
        match find(&label.name) {
            None if org.labels_rename.iter().any(|(old, new)| *new == label.name && find(old).is_some()) => {}
            None => steps.push(step("create", "label", class_name, name, label.name.clone(), vec![call("POST", format!("{base}/labels"), Some(json!({ "name": label.name, "color": label.color, "description": label.description })))])),
            Some(existing) => {
                let color_matches = existing["color"].as_str().is_some_and(|color| color.eq_ignore_ascii_case(&label.color));
                let description_matches = existing["description"].as_str().unwrap_or_default() == label.description;
                if !color_matches || !description_matches {
                    steps.push(step("update", "label", class_name, name, label.name.clone(), vec![call("PATCH", format!("{base}/labels/{}", encode(existing["name"].as_str().unwrap_or(&label.name))), Some(json!({ "color": label.color, "description": label.description })))]));
                }
            }
        }
    }
}

/// The part of a ruleset that the file controls, in a form that compares equal for the declaration and for what the API returns.
pub fn ruleset_projection(ruleset: &Value) -> Value {
    let mut rules: Vec<Value> = ruleset["rules"].as_array().into_iter().flatten().map(|rule| json!({ "type": rule["type"], "parameters": rule.get("parameters").cloned().unwrap_or(Value::Null) })).collect();
    rules.sort_by_key(|rule| rule["type"].as_str().unwrap_or_default().to_string());
    json!({
        "target": ruleset["target"],
        "enforcement": ruleset["enforcement"],
        "bypass_actors": ruleset["bypass_actors"].as_array().map(Vec::len).unwrap_or(0),
        "include": ruleset["conditions"]["ref_name"]["include"],
        "exclude": ruleset["conditions"]["ref_name"]["exclude"],
        "rules": rules,
    })
}

fn ruleset_body(org: &Org, key: &str) -> Value {
    let declared = &org.ruleset[key];
    let rules: Vec<Value> = declared.rules.iter().map(|rule| Value::Object(table_to_json(rule))).collect();
    json!({
        "name": key,
        "target": declared.target,
        "enforcement": declared.enforcement,
        "bypass_actors": [],
        "conditions": { "ref_name": { "include": declared.include, "exclude": declared.exclude } },
        "rules": rules,
    })
}

fn ruleset_steps(org: &Org, repo: &RepoLive, class_name: &str, base: &str, wanted: &[String], steps: &mut Vec<Step>) {
    let name = repo.name.as_str();
    for key in wanted {
        let body = ruleset_body(org, key);
        match repo.rulesets.iter().find(|row| row["name"] == key.as_str()) {
            None => steps.push(step("create", "ruleset", class_name, name, key.clone(), vec![call("POST", format!("{base}/rulesets"), Some(body))])),
            Some(existing) if ruleset_projection(existing) != ruleset_projection(&body) => {
                let id = existing["id"].as_u64().unwrap_or_default();
                steps.push(step("update", "ruleset", class_name, name, format!("{key}: {} -> {}", rule_types(existing), rule_types(&body)), vec![call("PUT", format!("{base}/rulesets/{id}"), Some(body))]));
            }
            Some(_) => {}
        }
    }
    for existing in &repo.rulesets {
        let live_name = existing["name"].as_str().unwrap_or_default();
        if !wanted.iter().any(|key| key == live_name) {
            let id = existing["id"].as_u64().unwrap_or_default();
            steps.push(step("delete", "ruleset", class_name, name, format!("{live_name} is not declared ({})", rule_types(existing)), vec![call("DELETE", format!("{base}/rulesets/{id}"), None)]));
        }
    }
}

fn rule_types(ruleset: &Value) -> String {
    let mut types: Vec<&str> = ruleset["rules"].as_array().into_iter().flatten().filter_map(|rule| rule["type"].as_str()).collect();
    types.sort_unstable();
    types.join("+")
}

/// A committed plan is the only thing `apply` executes: it must be tracked and unmodified, and equal to a fresh plan.
pub fn require_committed(plan_path: &Path, committed: &Plan, fresh: &Plan) -> Result<()> {
    let directory = plan_path.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    let file_name = plan_path.file_name().ok_or_else(|| anyhow!("plan path has no file name"))?;
    let tracked = Command::new("git").arg("-C").arg(directory).args(["ls-files", "--error-unmatch"]).arg(file_name).output().context("run git ls-files")?;
    if !tracked.status.success() {
        bail!("{} is not tracked by git: commit the plan before applying it", plan_path.display());
    }
    let unchanged = Command::new("git").arg("-C").arg(directory).args(["diff", "--quiet", "HEAD", "--"]).arg(file_name).status().context("run git diff")?;
    if !unchanged.success() {
        bail!("{} differs from HEAD: commit the plan before applying it", plan_path.display());
    }
    if committed != fresh {
        bail!("the committed plan no longer matches org.toml and the live state: run `fw org plan`, commit it, then apply");
    }
    Ok(())
}

pub fn execute(github: &Github, steps: &[Step]) -> Result<()> {
    let mut captured: BTreeMap<String, String> = BTreeMap::new();
    for item in steps {
        for request in &item.calls {
            let mut path = request.path.clone();
            for (key, value) in &captured {
                path = path.replace(&format!("{{{key}}}"), value);
            }
            let body = github.send(request.method, &path, request.body.as_ref()).with_context(|| format!("{} {} {} {}", item.action.op, item.action.kind, item.action.scope, item.action.target))?;
            if let Some(key) = &request.capture {
                let id = body["id"].as_u64().ok_or_else(|| anyhow!("response of {path} has no id"))?;
                captured.insert(key.clone(), id.to_string());
            }
        }
        println!("applied {} {} {} {}", item.action.op, item.action.kind, item.action.scope, item.action.target);
    }
    Ok(())
}

