use crate::live::Live;
use crate::model::Org;
use crate::plan::{Identity, Plan, secret_users};

#[derive(Debug, Clone)]
pub struct Finding {
    pub id: &'static str,
    pub subject: String,
    pub message: String,
    pub error: bool,
}

fn finding(id: &'static str, subject: impl Into<String>, message: impl Into<String>, error: bool) -> Finding {
    Finding { id, subject: subject.into(), message: message.into(), error }
}

/// Text that must not appear in a public repository: an IPv4 address or a mail address.
fn private_marker(text: &str) -> Option<&'static str> {
    if text.contains('@') && text.split_whitespace().any(|word| word.contains('@') && word.contains('.')) {
        return Some("a mail address");
    }
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index].is_ascii_digit() && (index == 0 || !bytes[index - 1].is_ascii_alphanumeric()) {
            let end = text[index..].find(|character: char| !(character.is_ascii_digit() || character == '.')).map_or(text.len(), |offset| index + offset);
            let parts: Vec<&str> = text[index..end].trim_end_matches('.').split('.').collect();
            if parts.len() == 4 && parts.iter().all(|part| !part.is_empty() && part.len() <= 3 && part.parse::<u8>().is_ok()) {
                return Some("an IP address");
            }
            index = end.max(index + 1);
        } else {
            index += 1;
        }
    }
    None
}

pub fn run(org: &Org, live: &Live, plan: &Plan, identity: &Identity) -> Vec<Finding> {
    let mut findings = Vec::new();
    if !plan.actions.is_empty() {
        findings.push(finding("drift", org.org.name.clone(), format!("{} planned actions: the live organization differs from org.toml", plan.actions.len()), true));
    }

    if let (Some(blog), Some(root)) = (live.org["blog"].as_str(), identity.domain_root.as_deref())
        && !blog.contains(root)
    {
        findings.push(finding("brand-domain", org.org.name.clone(), format!("profile blog {blog} is not the root domain {root} of the identity file"), true));
    }
    if live.org["two_factor_requirement_enabled"] == false {
        findings.push(finding("two-factor", org.org.name.clone(), "two-factor authentication is not required for members", false));
    }
    if live.org_actions["allowed_actions"] == "all" {
        findings.push(finding("actions-allowed-all", org.org.name.clone(), "every third-party action may run; derive an allow-list from the workflows of the originals", false));
    }
    if live.admins.len() > 2 {
        findings.push(finding("admins", org.org.name.clone(), format!("{} organization owners", live.admins.len()), false));
    }
    for invitation in &live.invitations {
        findings.push(finding("invitation", invitation["login"].as_str().unwrap_or("?"), "pending invitation", false));
    }
    for collaborator in &live.outside_collaborators {
        findings.push(finding("outside-collaborator", collaborator["login"].as_str().unwrap_or("?"), "outside collaborator on a repository of the organization", false));
    }
    for installation in &live.installations {
        if installation["repository_selection"] == "all" {
            findings.push(finding("app-all-repositories", installation["app_slug"].as_str().unwrap_or("?"), "GitHub App installed on every repository, forks included", false));
        }
    }
    for runner in &live.runners {
        if runner["status"] != "online" {
            findings.push(finding("runner-offline", runner["name"].as_str().unwrap_or("?"), "self-hosted runner is not online", false));
        }
    }
    for secret in &live.secrets {
        let name = secret["name"].as_str().unwrap_or("?");
        match org.secret.get(name) {
            None => findings.push(finding("secret-undeclared", name, "organization secret that org.toml does not declare", false)),
            Some(declared) => {
                if declared.visibility == "selected" && secret_users(live, name, &declared.extra_repos).is_empty() {
                    findings.push(finding("secret-unused", name, "no workflow of an original reads this secret", false));
                }
            }
        }
        if secret["visibility"] == "all" {
            findings.push(finding("secret-visibility-all", name, format!("visible to every repository, forks included; the API cannot narrow it without the value: change it in the organization settings to selected ({})", secret_users(live, name, &org.secret.get(name).map(|declared| declared.extra_repos.clone()).unwrap_or_default()).iter().map(|repo| repo.name.as_str()).collect::<Vec<_>>().join(", ")), true));
        }
    }
    for declared in org.team.keys() {
        if !live.teams.iter().any(|team| team["slug"] == declared.as_str()) {
            findings.push(finding("team-missing", declared.clone(), "declared team does not exist", true));
        }
    }
    for team in &live.teams {
        let slug = team["slug"].as_str().unwrap_or("?");
        match org.team.get(slug) {
            None => findings.push(finding("team-undeclared", slug, "team that org.toml does not declare", false)),
            Some(declared) if team["privacy"] != declared.privacy.as_str() || team["description"].as_str().unwrap_or_default() != declared.description => {
                findings.push(finding("team-drift", slug, "privacy or description differs from org.toml", false));
            }
            Some(_) => {}
        }
    }

    for (what, reason) in &live.refused {
        findings.push(finding("read-refused", what.clone(), reason.clone(), false));
    }
    if let Some(reason) = &live.packages_refused {
        findings.push(finding("packages-unreadable", org.org.name.clone(), format!("packages cannot be listed ({reason}); the token needs the read:packages scope"), false));
    }
    for package in &live.packages {
        let name = package["name"].as_str().unwrap_or("?");
        if package["visibility"] != "public" {
            findings.push(finding("package-visibility", name, format!("{} package is {}", package["package_type"].as_str().unwrap_or("?"), package["visibility"].as_str().unwrap_or("?")), false));
        }
        if package["repository"].is_null() {
            findings.push(finding("package-unlinked", name, "package is not linked to a repository", false));
        }
    }

    for repo in &live.repos {
        let summary = &repo.summary;
        if summary["archived"] == true {
            continue;
        }
        let name = repo.name.as_str();
        let (class_name, class) = org.class_of(summary);
        let public = summary["private"] != true;
        if public {
            for (field, text) in [("description", summary["description"].as_str()), ("homepage", summary["homepage"].as_str())] {
                if let Some(marker) = text.and_then(private_marker) {
                    findings.push(finding("private-data", name, format!("the {field} contains {marker}"), true));
                }
            }
        }
        if let Some(reason) = identity.retired.get(name) {
            findings.push(finding("retired-live", name, format!("retired in the identity file and not archived: {reason}"), false));
        }
        if class_name != "fork" {
            let declaration = org.repo.get(name);
            if summary["description"].as_str().unwrap_or_default().is_empty() && declaration.and_then(|row| row.description.as_ref()).is_none() {
                findings.push(finding("description-missing", name, "original without a description; declare [repo.<name>] description", false));
            }
            if summary["license"].is_null() && public {
                findings.push(finding("license-missing", name, "original without a detected LICENSE", false));
            }
            if summary["default_branch"] != "main" {
                findings.push(finding("default-branch", name, format!("default branch is {}, work happens on main", summary["default_branch"].as_str().unwrap_or("?")), false));
            }
            if public && summary["homepage"].as_str().unwrap_or_default().is_empty() && declaration.and_then(|row| row.homepage.as_ref()).is_none() {
                findings.push(finding("homepage-missing", name, "public original without a homepage", false));
            }
        }
        if summary["has_wiki"] == true && class.settings.get("has_wiki").and_then(toml::Value::as_bool) == Some(false) && repo.wiki_has_content == Some(true) {
            findings.push(finding("wiki-content", name, "the wiki has pages: move them to docs/ before switching it off", false));
        }
        if summary["has_pages"] == true && !repo.pages.is_null() && repo.pages["build_type"] != "workflow" {
            findings.push(finding("pages-source", name, format!("Pages build_type is {}, not workflow", repo.pages["build_type"].as_str().unwrap_or("?")), false));
        }
        if let Some(security) = Some(class.security.as_str()).filter(|security| !security.is_empty())
            && live.attachments.values().any(|map| map.get(name).is_some_and(|status| status == "failed"))
        {
            findings.push(finding("security-attach-failed", name, format!("attachment of {security} failed"), true));
        }
    }
    if let Some(value) = live.defaults.as_array() {
        for (key, declared) in &org.security {
            if let Some(scope) = &declared.default_for_new_repos
                && !value.iter().any(|row| row["configuration"]["name"] == key.as_str() && row["default_for_new_repos"] == scope.as_str())
            {
                findings.push(finding("security-default", key.clone(), format!("not the default for new {scope} repositories"), true));
            }
        }
    }
    findings
}
