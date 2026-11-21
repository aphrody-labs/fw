//! Live snapshot of the DioxusLabs organisation.
//!
//! Sources: the GitHub GraphQL and REST APIs through `gh` (authentication stays in gh, never in argv) and the crates.io API.
//! Output: `docs/research/dioxuslabs-ecosystem.json` and `docs/research/dioxuslabs-ecosystem.md`.
//! A field that cannot be read is dropped, never filled. Usage: `dioxus-snapshot [--root DIR] [--check]`.

use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{env, fs, thread};

type Failure = Box<dyn std::error::Error>;

const VOLATILE_KEYS: [&str; 8] = [
    "date", "stars", "forks", "open_issues", "open_prs", "discussions", "downloads", "recent_downloads",
];

fn main() -> Result<(), Failure> {
    let mut root: Option<PathBuf> = None;
    let mut check = false;
    let mut arguments = env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--root" => root = arguments.next().map(PathBuf::from),
            "--check" => check = true,
            other => return Err(format!("unknown argument {other}").into()),
        }
    }
    let root = match root {
        Some(path) => path,
        None => find_root(&env::current_dir()?)?,
    };
    let config: toml::Table = fs::read_to_string(root.join("tools/dioxus-snapshot/snapshot.toml"))?.parse()?;
    let org = text(&config, "org")?;
    let since = text(&config, "since")?;
    let architecture_repo = text(&config, "architecture_repo")?;
    let crate_names: Vec<String> = config
        .get("crates")
        .and_then(toml::Value::as_array)
        .map(|list| list.iter().filter_map(|item| item.as_str().map(str::to_owned)).collect())
        .unwrap_or_default();

    let upstreams: toml::Table = fs::read_to_string(root.join("upstreams.toml"))?.parse()?;
    let tracked: Vec<(String, String)> = upstreams
        .get("upstream")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let repo = entry.get("repo")?.as_str()?;
            let (owner, name) = repo.split_once('/')?;
            if owner != org {
                return None;
            }
            Some((name.to_owned(), entry.get("id")?.as_str()?.to_owned()))
        })
        .collect();

    let repositories = read_repositories(&org, &since)?;
    let mut tracked_detail = Map::new();
    for (name, id) in &tracked {
        match read_tracked(&org, name, &since) {
            Ok(mut detail) => {
                detail["upstream_id"] = json!(id);
                tracked_detail.insert(name.clone(), detail);
            }
            Err(error) => eprintln!("tracked {name}: dropped ({error})"),
        }
    }
    let mut compared = Map::new();
    for (repository, path) in config.get("compare").and_then(toml::Value::as_table).into_iter().flatten() {
        let Some((owner, name)) = repository.split_once('/') else { continue };
        match read_compared(owner, name, path.as_str().unwrap_or_default(), &since) {
            Ok(detail) => {
                compared.insert(repository.clone(), detail);
            }
            Err(error) => eprintln!("compare {repository}: dropped ({error})"),
        }
    }
    let architecture = read_architecture(&org, &architecture_repo).unwrap_or_else(|error| {
        eprintln!("architecture notes: dropped ({error})");
        Value::Null
    });
    let mut crates = Map::new();
    for name in &crate_names {
        match read_crate(name) {
            Ok(Some(entry)) => {
                crates.insert(name.clone(), entry);
            }
            Ok(None) => eprintln!("crate {name}: not on crates.io, dropped"),
            Err(error) => eprintln!("crate {name}: dropped ({error})"),
        }
        thread::sleep(Duration::from_millis(1100));
    }
    let issues = read_cited_issues(&root, &config)?;

    let snapshot = compact(json!({
        "schema": 1,
        "date": today(),
        "since": since,
        "organisation": org,
        "repositories": repositories,
        "tracked": Value::Object(tracked_detail),
        "compared": Value::Object(compared),
        "architecture_notes": architecture,
        "crates": Value::Object(crates),
        "cited_issues": issues,
    }));

    let json_path = root.join("docs/research/dioxuslabs-ecosystem.json");
    let markdown_path = root.join("docs/research/dioxuslabs-ecosystem.md");
    let before: Value = fs::read_to_string(&json_path).ok().and_then(|raw| serde_json::from_str(&raw).ok()).unwrap_or(Value::Null);
    let changed = strip_volatile(&before) != strip_volatile(&snapshot);
    println!("{}", json!({ "ok": true, "repositories": snapshot["repositories"]["count"], "tracked": tracked.len(), "crates": snapshot["crates"].as_object().map_or(0, Map::len), "changed": changed }));
    if check {
        if changed {
            std::process::exit(1);
        }
        return Ok(());
    }
    if changed || !json_path.exists() {
        fs::write(&json_path, serde_json::to_string_pretty(&snapshot)? + "\n")?;
        fs::write(&markdown_path, summary(&snapshot))?;
    }
    Ok(())
}

fn text(config: &toml::Table, key: &str) -> Result<String, Failure> {
    config.get(key).and_then(toml::Value::as_str).map(str::to_owned).ok_or_else(|| format!("snapshot.toml: missing {key}").into())
}

fn find_root(start: &Path) -> Result<PathBuf, Failure> {
    start
        .ancestors()
        .find(|path| path.join("upstreams.toml").is_file() && path.join("tools/dioxus-snapshot").is_dir())
        .map(Path::to_path_buf)
        .ok_or_else(|| "run from inside the fw checkout or pass --root".into())
}

/// Retries transient failures (HTTP 5xx, secondary rate limit) three times before giving up.
fn gh(arguments: &[&str]) -> Result<Value, Failure> {
    let mut last_error = String::new();
    for attempt in 1..=3_u64 {
        let output = Command::new("gh").args(arguments).output()?;
        if output.status.success() {
            return Ok(serde_json::from_slice(&output.stdout)?);
        }
        last_error = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        thread::sleep(Duration::from_secs(2 * attempt));
    }
    Err(format!("gh {}: {last_error}", arguments.first().copied().unwrap_or("")).into())
}

fn graphql(query: &str, variables: &[(&str, &str)]) -> Result<Value, Failure> {
    let mut arguments = vec!["api", "graphql", "-f"];
    let query_field = format!("query={query}");
    arguments.push(&query_field);
    let fields: Vec<String> = variables.iter().map(|(key, value)| format!("{key}={value}")).collect();
    for field in &fields {
        arguments.push("-F");
        arguments.push(field);
    }
    let response = gh(&arguments)?;
    if let Some(errors) = response.get("errors") {
        return Err(format!("graphql: {errors}").into());
    }
    Ok(response["data"].clone())
}

fn read_repositories(org: &str, since: &str) -> Result<Value, Failure> {
    let query = r#"query($org:String!,$cursor:String,$since:GitTimestamp!){organization(login:$org){login name createdAt
      repositories(first:100,after:$cursor,orderBy:{field:STARGAZERS,direction:DESC}){totalCount pageInfo{hasNextPage endCursor}
      nodes{name description isArchived isFork stargazerCount forkCount pushedAt licenseInfo{spdxId} primaryLanguage{name}
      issues(states:OPEN){totalCount} pullRequests(states:OPEN){totalCount} discussions{totalCount}
      agents:object(expression:"HEAD:AGENTS.md"){... on Blob{byteSize}}
      defaultBranchRef{name target{... on Commit{history(since:$since){totalCount}}}}}}}}"#;
    let since_timestamp = format!("{since}T00:00:00Z");
    let mut cursor = String::new();
    let mut rows: Vec<Value> = Vec::new();
    let organisation = loop {
        let mut variables = vec![("org", org), ("since", since_timestamp.as_str())];
        if !cursor.is_empty() {
            variables.push(("cursor", cursor.as_str()));
        }
        let data = graphql(query, &variables)?;
        let page = &data["organization"]["repositories"];
        for node in page["nodes"].as_array().into_iter().flatten() {
            rows.push(json!({
                "name": node["name"], "description": node["description"], "archived": node["isArchived"], "fork": node["isFork"],
                "stars": node["stargazerCount"], "forks": node["forkCount"],
                "pushed": node["pushedAt"].as_str().map(|day| &day[..10]),
                "license": node["licenseInfo"]["spdxId"], "language": node["primaryLanguage"]["name"],
                "open_issues": node["issues"]["totalCount"], "open_prs": node["pullRequests"]["totalCount"], "discussions": node["discussions"]["totalCount"],
                "agents_md_bytes": node["agents"]["byteSize"],
                "commits_since": node["defaultBranchRef"]["target"]["history"]["totalCount"],
            }));
        }
        if page["pageInfo"]["hasNextPage"].as_bool() != Some(true) {
            break json!({ "name": data["organization"]["name"], "created": data["organization"]["createdAt"].as_str().map(|day| &day[..10]), "total": page["totalCount"] });
        }
        cursor = page["pageInfo"]["endCursor"].as_str().unwrap_or_default().to_owned();
    };
    Ok(json!({ "count": rows.len(), "organisation": organisation, "list": rows }))
}

fn read_tracked(org: &str, name: &str, since: &str) -> Result<Value, Failure> {
    let query = r#"query($org:String!,$name:String!,$merged:String!,$opened:String!,$prs:String!){
      repository(owner:$org,name:$name){defaultBranchRef{name target{... on Commit{oid committedDate}}}
        releases(first:6,orderBy:{field:CREATED_AT,direction:DESC}){nodes{tagName publishedAt isPrerelease}}
        agents:object(expression:"HEAD:AGENTS.md"){... on Blob{byteSize text}}}
      merged:search(query:$merged,type:ISSUE){issueCount}
      opened:search(query:$opened,type:ISSUE){issueCount}
      pulls:search(query:$prs,type:ISSUE){issueCount}}"#;
    let merged = format!("repo:{org}/{name} is:pr is:merged merged:>={since}");
    let opened = format!("repo:{org}/{name} is:issue created:>={since}");
    let pulls = format!("repo:{org}/{name} is:pr created:>={since}");
    let data = graphql(query, &[("org", org), ("name", name), ("merged", &merged), ("opened", &opened), ("prs", &pulls)])?;
    let repository = &data["repository"];
    let releases: Vec<Value> = repository["releases"]["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|release| json!({ "tag": release["tagName"], "date": release["publishedAt"].as_str().map(|day| &day[..10]), "prerelease": release["isPrerelease"] }))
        .collect();
    Ok(json!({
        "branch": repository["defaultBranchRef"]["name"],
        "head": repository["defaultBranchRef"]["target"]["oid"],
        "head_date": repository["defaultBranchRef"]["target"]["committedDate"].as_str().map(|day| &day[..10]),
        "releases": releases,
        "merged_prs_since": data["merged"]["issueCount"],
        "opened_issues_since": data["opened"]["issueCount"],
        "opened_prs_since": data["pulls"]["issueCount"],
        "agents_md": { "bytes": repository["agents"]["byteSize"], "text": repository["agents"]["text"] },
    }))
}

/// Activity of a repository that is not a DioxusLabs upstream but the other side of a comparison (GPUI lives in `path` of zed-industries/zed).
fn read_compared(owner: &str, name: &str, path: &str, since: &str) -> Result<Value, Failure> {
    let query = r#"query($owner:String!,$name:String!,$since:GitTimestamp!,$path:String!,$merged:String!){
      repository(owner:$owner,name:$name){stargazerCount forkCount licenseInfo{spdxId} defaultBranchRef{name target{... on Commit{oid
        all:history(since:$since){totalCount} scoped:history(since:$since,path:$path){totalCount}}}}
        releases(first:1,orderBy:{field:CREATED_AT,direction:DESC}){nodes{tagName publishedAt}}}
      merged:search(query:$merged,type:ISSUE){issueCount}}"#;
    let merged = format!("repo:{owner}/{name} is:pr is:merged merged:>={since}");
    let since_timestamp = format!("{since}T00:00:00Z");
    let data = graphql(query, &[("owner", owner), ("name", name), ("since", &since_timestamp), ("path", path), ("merged", &merged)])?;
    let repository = &data["repository"];
    let target = &repository["defaultBranchRef"]["target"];
    Ok(json!({
        "path": path, "stars": repository["stargazerCount"], "forks": repository["forkCount"], "license": repository["licenseInfo"]["spdxId"],
        "head": target["oid"], "commits_since": target["all"]["totalCount"], "path_commits_since": target["scoped"]["totalCount"],
        "merged_prs_since": data["merged"]["issueCount"],
        "latest_release": repository["releases"]["nodes"].get(0).map(|release| json!({ "tag": release["tagName"], "date": release["publishedAt"].as_str().map(|day| &day[..10]) })),
    }))
}

fn read_architecture(org: &str, repo: &str) -> Result<Value, Failure> {
    let tree = gh(&["api", &format!("repos/{org}/{repo}/git/trees/HEAD?recursive=1")])?;
    let notes: Vec<Value> = tree["tree"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entry| entry["path"].as_str().is_some_and(|path| path.starts_with("notes/architecture/") && path.ends_with(".md")))
        .map(|entry| json!({ "path": entry["path"], "bytes": entry["size"], "blob": entry["sha"] }))
        .collect();
    Ok(Value::Array(notes))
}

fn read_crate(name: &str) -> Result<Option<Value>, Failure> {
    let url = format!("https://crates.io/api/v1/crates/{name}");
    let mut response = match ureq::get(&url).header("User-Agent", "fw-dioxus-snapshot").call() {
        Ok(response) => response,
        Err(ureq::Error::StatusCode(404)) => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let body: Value = response.body_mut().read_json()?;
    let entry = &body["crate"];
    Ok(Some(json!({
        "max_version": entry["max_version"], "max_stable_version": entry["max_stable_version"], "newest_version": entry["newest_version"],
        "updated": entry["updated_at"].as_str().map(|day| &day[..10]), "downloads": entry["downloads"], "recent_downloads": entry["recent_downloads"],
        "repository": entry["repository"],
    })))
}

/// Finds `dioxus#6058`, `DioxusLabs/blitz#12` and `blitz#12` in the skill and re-reads each live.
fn read_cited_issues(root: &Path, config: &toml::Table) -> Result<Value, Failure> {
    let org = text(config, "org")?;
    let Ok(skill) = fs::read_to_string(root.join(text(config, "skill")?)) else {
        return Ok(Value::Null);
    };
    let mut cited: BTreeSet<(String, u64)> = BTreeSet::new();
    for (index, _) in skill.match_indices('#') {
        let number: String = skill[index + 1..].chars().take_while(char::is_ascii_digit).collect();
        let token: String = skill[..index].chars().rev().take_while(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '/')).collect::<Vec<_>>().into_iter().rev().collect();
        let repo = token.rsplit('/').next().unwrap_or_default();
        if let (Ok(number), false) = (number.parse::<u64>(), repo.is_empty()) {
            cited.insert((repo.to_owned(), number));
        }
    }
    let mut rows = Vec::new();
    for (repo, number) in cited {
        let Ok(issue) = gh(&["api", &format!("repos/{org}/{repo}/issues/{number}")]) else { continue };
        rows.push(json!({
            "ref": format!("{repo}#{number}"), "title": issue["title"], "state": issue["state"],
            "kind": if issue["pull_request"].is_null() { "issue" } else { "pull_request" },
            "created": issue["created_at"].as_str().map(|day| &day[..10]),
        }));
    }
    Ok(Value::Array(rows))
}

fn today() -> String {
    let seconds = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |elapsed| elapsed.as_secs());
    let days = i64::try_from(seconds / 86_400).unwrap_or(0) + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_part = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_part + 2) / 5 + 1;
    let month = if month_part < 10 { month_part + 3 } else { month_part - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Drops null fields and empty containers so a value that could not be sourced is absent, not a placeholder.
fn compact(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let kept: Map<String, Value> = map.into_iter().map(|(key, inner)| (key, compact(inner))).filter(|(_, inner)| !is_empty(inner)).collect();
            Value::Object(kept)
        }
        Value::Array(list) => Value::Array(list.into_iter().map(compact).filter(|inner| !is_empty(inner)).collect()),
        other => other,
    }
}

fn is_empty(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Object(map) => map.is_empty(),
        Value::Array(list) => list.is_empty(),
        _ => false,
    }
}

fn strip_volatile(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(map.iter().filter(|(key, _)| !VOLATILE_KEYS.contains(&key.as_str())).map(|(key, inner)| (key.clone(), strip_volatile(inner))).collect()),
        Value::Array(list) => Value::Array(list.iter().map(strip_volatile).collect()),
        other => other.clone(),
    }
}

fn summary(snapshot: &Value) -> String {
    let field = |value: &Value| value.as_str().map_or_else(|| value.to_string(), str::to_owned);
    let mut out = format!(
        "# DioxusLabs ecosystem snapshot\n\nGenerated by `tools/dioxus-snapshot` (live GitHub GraphQL and crates.io reads, date {}); do not edit. Full data: [dioxuslabs-ecosystem.json](dioxuslabs-ecosystem.json).\n\n",
        field(&snapshot["date"])
    );
    out += &format!("Organisation {}: {} public repositories.\n\n", field(&snapshot["organisation"]), field(&snapshot["repositories"]["count"]));
    out += "## Top repositories by stars\n\n| repository | stars | forks | open issues | open PRs | discussions | commits since | AGENTS.md bytes | pushed |\n|---|---:|---:|---:|---:|---:|---:|---:|---|\n";
    for row in snapshot["repositories"]["list"].as_array().into_iter().flatten().take(20) {
        out += &format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            field(&row["name"]), field(&row["stars"]), field(&row["forks"]), field(&row["open_issues"]), field(&row["open_prs"]),
            field(&row["discussions"]), field(&row["commits_since"]), row["agents_md_bytes"].as_u64().map_or("-".to_owned(), |bytes| bytes.to_string()), field(&row["pushed"])
        );
    }
    out += "\n## Tracked in upstreams.toml\n\n| repository | upstream id | latest release | head | merged PRs since | issues opened since |\n|---|---|---|---|---:|---:|\n";
    for (name, detail) in snapshot["tracked"].as_object().into_iter().flatten() {
        let release = detail["releases"].get(0).map_or("-".to_owned(), |latest| format!("{} ({})", field(&latest["tag"]), field(&latest["date"])));
        let head = detail["head"].as_str().map_or("-", |oid| &oid[..oid.len().min(10)]);
        out += &format!("| {name} | {} | {release} | {head} | {} | {} |\n", field(&detail["upstream_id"]), field(&detail["merged_prs_since"]), field(&detail["opened_issues_since"]));
    }
    if let Some(compared) = snapshot["compared"].as_object() {
        out += "\n## Compared repositories\n\n| repository | path | stars | commits since (all) | commits since (path) | merged PRs since | head |\n|---|---|---:|---:|---:|---:|---|\n";
        for (name, detail) in compared {
            let head = detail["head"].as_str().map_or("-", |oid| &oid[..oid.len().min(10)]);
            out += &format!("| {name} | {} | {} | {} | {} | {} | {head} |\n", field(&detail["path"]), field(&detail["stars"]), field(&detail["commits_since"]), field(&detail["path_commits_since"]), field(&detail["merged_prs_since"]));
        }
    }
    out += "\n## crates.io\n\n| crate | max version | max stable | updated | downloads |\n|---|---|---|---|---:|\n";
    for (name, entry) in snapshot["crates"].as_object().into_iter().flatten() {
        out += &format!("| {name} | {} | {} | {} | {} |\n", field(&entry["max_version"]), field(&entry["max_stable_version"]), field(&entry["updated"]), field(&entry["downloads"]));
    }
    if let Some(issues) = snapshot["cited_issues"].as_array() {
        out += "\n## Issues cited by the dioxus skill (re-read live)\n\n| reference | kind | state | title |\n|---|---|---|---|\n";
        for issue in issues {
            out += &format!("| {} | {} | {} | {} |\n", field(&issue["ref"]), field(&issue["kind"]), field(&issue["state"]), field(&issue["title"]));
        }
    }
    if let Some(notes) = snapshot["architecture_notes"].as_array() {
        out += "\n## Architecture notes (dioxus notes/architecture)\n\n";
        for note in notes {
            out += &format!("- `{}` ({} bytes)\n", field(&note["path"]), field(&note["bytes"]));
        }
    }
    out
}
