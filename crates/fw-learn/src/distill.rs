//! Distillation: turns the corpus into rules for agents and humans. Every rule is a sentence with a number or a quote taken
//! from the store (counts from git history, imperative lines of AGENTS.md, CI failure rates, the code graph's god nodes), and
//! carries its evidence; nothing is written from memory.

use crate::store::{Item, Store};
use anyhow::Result;
use rusqlite::params;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::{SystemTime, UNIX_EPOCH};

const WINDOW_DAYS: i64 = 90;
const COUPLING_DAYS: i64 = 365;
const MAX_AREAS_PER_COMMIT: i64 = 12;

/// Section order and the number of bullets each keeps in the compact form (AGENTS.md blocks).
pub const SECTIONS: &[(&str, &str, usize)] = &[
    ("identity", "What it is", 4),
    ("standing", "Standing rules from the agent contract files", 14),
    ("pitfalls", "Pitfalls: areas with the most fix commits", 6),
    ("hot", "Hot areas in the last 90 days", 5),
    ("coupling", "Files that change together", 5),
    ("architecture", "Architecture", 6),
    ("ci", "CI", 4),
    ("release", "Releases", 4),
    ("issues", "Issues and pull requests", 5),
    ("style", "Commit style", 1),
];

struct Rule {
    section: &'static str,
    text: String,
    evidence: String,
}

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|elapsed| elapsed.as_secs() as i64).unwrap_or(0)
}

pub fn distill(store: &Store) -> Result<usize> {
    store.conn.execute_batch(
        "DROP TABLE IF EXISTS temp.small;
         CREATE TEMP TABLE small AS SELECT repo, sha FROM commit_area GROUP BY repo, sha HAVING COUNT(*) <= 12;
         CREATE INDEX temp.small_key ON small(repo, sha);",
    )?;
    let mut products = store.conn.prepare("SELECT repo, id, display, description, dir FROM product ORDER BY repo, dir")?;
    let products: Vec<(String, String, String, String, String)> = products.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)))?.collect::<std::result::Result<_, _>>()?;
    store.begin()?;
    store.conn.execute("DELETE FROM rule", [])?;
    store.conn.execute("DELETE FROM item WHERE kind = 'rule'", [])?;
    let mut total = 0usize;
    for (repo, id, display, description, dir) in &products {
        let mut rules = Vec::new();
        identity(store, repo, id, display, description, dir, &mut rules)?;
        standing(store, id, &mut rules)?;
        pitfalls(store, id, &mut rules)?;
        hot(store, id, &mut rules)?;
        coupling(store, id, &mut rules)?;
        architecture(store, repo, id, &mut rules)?;
        if dir.is_empty() {
            ci(store, repo, &mut rules)?;
        }
        releases(store, id, &mut rules)?;
        issues(store, id, &mut rules)?;
        style(store, id, &mut rules)?;
        for (order, rule) in rules.iter().enumerate() {
            store.conn.execute("INSERT INTO rule(product, section, ord, text, evidence) VALUES (?1,?2,?3,?4,?5)", params![id, rule.section, order as i64, rule.text, rule.evidence])?;
            let reference = format!("{id}:{}:{order}", rule.section);
            store.upsert(&Item { kind: "rule", product: id, repo: "distilled", reference: &reference, title: &rule.text, body: &rule.evidence, path: rule.section, ts: now() })?;
            total += 1;
        }
    }
    store.commit()?;
    Ok(total)
}

fn identity(store: &Store, repo: &str, id: &str, display: &str, description: &str, dir: &str, rules: &mut Vec<Rule>) -> Result<()> {
    let mut kinds = store.conn.prepare("SELECT kind, COUNT(*) FROM unit WHERE repo = ?1 AND product = ?2 GROUP BY kind ORDER BY COUNT(*) DESC")?;
    let counts: Vec<String> = kinds
        .query_map(params![repo, id], |row| Ok(format!("{} {}", row.get::<_, i64>(1)?, row.get::<_, String>(0)?)))?
        .collect::<std::result::Result<_, _>>()?;
    let commits: i64 = store.conn.query_row("SELECT COUNT(*) FROM item WHERE kind = 'commit' AND product = ?1", [id], |row| row.get(0))?;
    let readme = if dir.is_empty() { "README.md".to_owned() } else { format!("{dir}/README.md") };
    let first_paragraph: String = store
        .conn
        .query_row("SELECT body FROM item WHERE kind = 'doc' AND repo = ?1 AND path = ?2 AND ref LIKE '%#0'", params![repo, readme], |row| row.get::<_, String>(0))
        .ok()
        .and_then(|body| body.split("\n\n").map(|paragraph| paragraph.split_whitespace().collect::<Vec<_>>().join(" ")).find(|paragraph| paragraph.len() >= 30 && !paragraph.starts_with(['>', '<', '[', '!'])))
        .unwrap_or_default();
    let description = if description.is_empty() { first_paragraph.chars().take(320).collect::<String>() } else { description.to_owned() };
    let summary = if description.is_empty() { String::new() } else { format!(": {description}") };
    rules.push(Rule { section: "identity", text: format!("{display} (`{id}`){summary}"), evidence: "product manifest".to_owned() });
    rules.push(Rule { section: "identity", text: format!("{commits} commits of history, units: {}", if counts.is_empty() { "none found".to_owned() } else { counts.join(", ") }), evidence: "git history and unit manifests".to_owned() });
    Ok(())
}

fn standing(store: &Store, id: &str, rules: &mut Vec<Rule>) -> Result<()> {
    let mut statement = store.conn.prepare(
        "SELECT title, path FROM item WHERE kind = 'rule-source' AND product = ?1
         ORDER BY (path LIKE '%AGENTS.md') DESC, (path LIKE '%CLAUDE.md') DESC, length(path), path, ref",
    )?;
    let mut seen = BTreeSet::new();
    for row in statement.query_map([id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))? {
        let (text, path) = row?;
        if seen.insert(text.to_ascii_lowercase()) {
            rules.push(Rule { section: "standing", text, evidence: path });
        }
    }
    Ok(())
}

fn latest_subject(store: &Store, id: &str, area: &str, fix_only: bool) -> Result<String> {
    let sql = format!(
        "SELECT i.title FROM commit_area a JOIN small s ON s.repo = a.repo AND s.sha = a.sha JOIN item i ON i.kind = 'commit' AND i.repo = a.repo AND i.ref = a.sha
         WHERE a.product = ?1 AND a.area = ?2 {} ORDER BY a.ts DESC LIMIT 1",
        if fix_only { "AND a.fix = 1" } else { "" }
    );
    Ok(store.conn.query_row(&sql, params![id, area], |row| row.get(0)).unwrap_or_default())
}

fn pitfalls(store: &Store, id: &str, rules: &mut Vec<Rule>) -> Result<()> {
    let mut statement = store.conn.prepare(
        "SELECT a.area, COUNT(DISTINCT a.sha) AS n FROM commit_area a JOIN small s ON s.repo = a.repo AND s.sha = a.sha
         WHERE a.product = ?1 AND a.area != '(root)' AND a.fix = 1 GROUP BY a.area ORDER BY n DESC, a.area LIMIT 8",
    )?;
    let rows: Vec<(String, i64)> = statement.query_map([id], |row| Ok((row.get(0)?, row.get(1)?)))?.collect::<std::result::Result<_, _>>()?;
    for (area, count) in rows {
        let subject = latest_subject(store, id, &area, true)?;
        rules.push(Rule { section: "pitfalls", text: format!("`{area}` has {count} fix commits in its history; the latest: {subject}"), evidence: "git history, subjects with fix, revert, regress, crash or bug".to_owned() });
    }
    Ok(())
}

fn hot(store: &Store, id: &str, rules: &mut Vec<Rule>) -> Result<()> {
    let since = now() - WINDOW_DAYS * 86_400;
    let mut statement = store.conn.prepare(
        "SELECT a.area, COUNT(DISTINCT a.sha) AS n FROM commit_area a JOIN small s ON s.repo = a.repo AND s.sha = a.sha
         WHERE a.product = ?1 AND a.area != '(root)' AND a.ts > ?2 GROUP BY a.area ORDER BY n DESC, a.area LIMIT 8",
    )?;
    let rows: Vec<(String, i64)> = statement.query_map(params![id, since], |row| Ok((row.get(0)?, row.get(1)?)))?.collect::<std::result::Result<_, _>>()?;
    for (area, count) in rows {
        let subject = latest_subject(store, id, &area, false)?;
        rules.push(Rule { section: "hot", text: format!("`{area}`: {count} commits in {WINDOW_DAYS} days; the latest: {subject}"), evidence: "git history".to_owned() });
    }
    Ok(())
}

fn coupling(store: &Store, id: &str, rules: &mut Vec<Rule>) -> Result<()> {
    let since = now() - COUPLING_DAYS * 86_400;
    let mut statement = store.conn.prepare(
        "SELECT a.sha, a.area FROM commit_area a JOIN small s ON s.repo = a.repo AND s.sha = a.sha WHERE a.product = ?1 AND a.ts > ?2",
    )?;
    let mut by_commit: HashMap<String, Vec<String>> = HashMap::new();
    for row in statement.query_map(params![id, since], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))? {
        let (sha, area) = row?;
        by_commit.entry(sha).or_default().push(area);
    }
    let mut singles: BTreeMap<&str, i64> = BTreeMap::new();
    let mut pairs: BTreeMap<(&str, &str), i64> = BTreeMap::new();
    for areas in by_commit.values().filter(|areas| (2..=MAX_AREAS_PER_COMMIT as usize).contains(&areas.len())) {
        for first in areas {
            *singles.entry(first).or_default() += 1;
            for second in areas.iter().filter(|second| *second != first) {
                *pairs.entry((first, second)).or_default() += 1;
            }
        }
    }
    let mut ranked: Vec<_> = pairs.iter().filter(|((first, _), together)| **together >= 8 && **together * 10 >= singles[first] * 6).collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    let mut used = BTreeSet::new();
    for ((first, second), together) in ranked {
        if !used.insert((*second, *first)) {
            continue;
        }
        rules.push(Rule { section: "coupling", text: format!("A change in `{first}` also touched `{second}` in {together} of {} commits; check both", singles[first]), evidence: format!("co-change count over the last {COUPLING_DAYS} days") });
        if used.len() >= 8 {
            break;
        }
    }
    Ok(())
}

fn architecture(store: &Store, repo: &str, id: &str, rules: &mut Vec<Rule>) -> Result<()> {
    let mut statement = store.conn.prepare("SELECT product, dir, name, deps FROM unit WHERE repo = ?1")?;
    let units: Vec<(String, String, String, String)> = statement.query_map([repo], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))?.collect::<std::result::Result<_, _>>()?;
    let mut names: BTreeMap<&str, &str> = BTreeMap::new();
    for unit in units.iter().filter(|unit| unit.0 == id) {
        let slot = names.entry(unit.2.as_str()).or_insert(unit.1.as_str());
        if unit.1.len() < slot.len() {
            *slot = unit.1.as_str();
        }
    }
    let mut dependents: BTreeMap<&str, i64> = BTreeMap::new();
    for (_, _, _, deps) in &units {
        for dep in deps.split(',').filter(|dep| names.contains_key(dep)) {
            *dependents.entry(dep).or_default() += 1;
        }
    }
    let mut ranked: Vec<_> = dependents.into_iter().filter(|(_, count)| *count >= 3).collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    for (name, count) in ranked.into_iter().take(8) {
        rules.push(Rule { section: "architecture", text: format!("`{name}` (`{}`) is a dependency of {count} units; a change there reaches all of them", names[name]), evidence: "unit manifests".to_owned() });
    }
    let mut report = store.conn.prepare("SELECT body FROM item WHERE kind = 'graph' AND product = ?1 AND title LIKE '%God nodes%' LIMIT 1")?;
    if let Ok(body) = report.query_row([id], |row| row.get::<_, String>(0)) {
        for line in body.lines().filter_map(|line| line.strip_prefix("- ")).take(6) {
            rules.push(Rule { section: "architecture", text: format!("God node of the code graph: {line}"), evidence: "fw-graph report".to_owned() });
        }
    }
    Ok(())
}

fn ci(store: &Store, repo: &str, rules: &mut Vec<Rule>) -> Result<()> {
    let mut statement = store.conn.prepare(
        "SELECT workflow, COUNT(*), SUM(conclusion = 'failure') FROM ci_run WHERE repo = ?1 AND conclusion IN ('success', 'failure') GROUP BY workflow ORDER BY SUM(conclusion = 'failure') DESC, workflow",
    )?;
    let rows: Vec<(String, i64, i64)> = statement.query_map([repo], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?.collect::<std::result::Result<_, _>>()?;
    if rows.is_empty() {
        return Ok(());
    }
    for (workflow, total, failed) in rows.iter().filter(|row| row.2 > 0).take(6) {
        let detail: String = store
            .conn
            .query_row("SELECT body FROM item WHERE kind = 'ci' AND repo = ?1 AND path = ?2 AND title LIKE '% failure:%' AND body != '' ORDER BY ts DESC LIMIT 1", params![repo, workflow], |row| row.get(0))
            .unwrap_or_default();
        let detail = detail.lines().next().map_or(String::new(), |line| format!("; latest failure: {line}"));
        rules.push(Rule { section: "ci", text: format!("Workflow `{workflow}`: {failed} of its last {total} finished runs failed{detail}"), evidence: "GitHub Actions runs".to_owned() });
    }
    let failing = rows.iter().filter(|row| row.2 > 0).count();
    rules.push(Rule { section: "ci", text: format!("{} of {} workflows with recent runs had at least one failure", failing, rows.len()), evidence: "GitHub Actions runs".to_owned() });
    Ok(())
}

fn releases(store: &Store, id: &str, rules: &mut Vec<Rule>) -> Result<()> {
    let mut statement = store.conn.prepare("SELECT path, title, MAX(ts) FROM item WHERE kind = 'release' AND product = ?1 GROUP BY path ORDER BY path")?;
    let rows: Vec<(String, String)> = statement.query_map([id], |row| Ok((row.get(0)?, row.get(1)?)))?.collect::<std::result::Result<_, _>>()?;
    for (channel, version) in rows {
        rules.push(Rule { section: "release", text: format!("Channel `{channel}` serves `{version}`"), evidence: "release manifest".to_owned() });
    }
    Ok(())
}

fn issues(store: &Store, id: &str, rules: &mut Vec<Rule>) -> Result<()> {
    let count = |kind: &str| -> Result<i64> { Ok(store.conn.query_row("SELECT COUNT(*) FROM item WHERE kind = ?1 AND product = ?2 AND body LIKE 'state open%'", params![kind, id], |row| row.get(0))?) };
    let (open_issues, open_pulls) = (count("issue")?, count("pr")?);
    if open_issues + open_pulls == 0 {
        return Ok(());
    }
    rules.push(Rule { section: "issues", text: format!("{open_issues} open issues and {open_pulls} open pull requests"), evidence: "GitHub".to_owned() });
    let mut statement = store.conn.prepare("SELECT ref, title FROM item WHERE kind = 'issue' AND product = ?1 AND body LIKE 'state open labels%bug%' ORDER BY ts DESC LIMIT 5")?;
    for row in statement.query_map([id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))? {
        let (number, title) = row?;
        rules.push(Rule { section: "issues", text: format!("Open bug #{number}: {title}"), evidence: "GitHub issues labelled bug".to_owned() });
    }
    Ok(())
}

/// The conventional-commit prefix of a subject: `fix`, `feat(ui)` and so on; none for free-form subjects.
pub fn prefix_of(subject: &str) -> Option<&str> {
    let head = subject.split_once(':')?.0;
    let kind = head.split('(').next()?;
    (!kind.is_empty() && kind.len() <= 12 && kind.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')).then_some(kind)
}

fn style(store: &Store, id: &str, rules: &mut Vec<Rule>) -> Result<()> {
    let mut statement = store.conn.prepare("SELECT title FROM item WHERE kind = 'commit' AND product = ?1")?;
    let mut counts: BTreeMap<String, i64> = BTreeMap::new();
    let mut total = 0i64;
    for subject in statement.query_map([id], |row| row.get::<_, String>(0))? {
        let subject = subject?;
        total += 1;
        if let Some(kind) = prefix_of(&subject) {
            *counts.entry(kind.to_owned()).or_default() += 1;
        }
    }
    let mut ranked: Vec<_> = counts.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    if total > 0 && !ranked.is_empty() {
        let top: Vec<String> = ranked.iter().take(6).map(|(kind, count)| format!("`{kind}:` {count}")).collect();
        rules.push(Rule { section: "style", text: format!("Commit subjects by prefix, out of {total}: {}", top.join(", ")), evidence: "git history".to_owned() });
    }
    Ok(())
}

/// Markdown of one product's rules. `compact` keeps the first few bullets of each section (for AGENTS.md blocks).
pub fn render(store: &Store, product: &str, compact: bool) -> Result<String> {
    let mut out = String::new();
    for (section, title, keep) in SECTIONS {
        let mut statement = store.conn.prepare("SELECT text, evidence FROM rule WHERE product = ?1 AND section = ?2 ORDER BY ord")?;
        let rows: Vec<(String, String)> = statement.query_map(params![product, section], |row| Ok((row.get(0)?, row.get(1)?)))?.collect::<std::result::Result<_, _>>()?;
        if rows.is_empty() {
            continue;
        }
        out.push_str(&format!("### {title}\n\n"));
        for (text, evidence) in rows.iter().take(if compact { *keep } else { usize::MAX }) {
            if compact || *section == "identity" {
                out.push_str(&format!("- {text}\n"));
            } else {
                out.push_str(&format!("- {text} ({evidence})\n"));
            }
        }
        out.push('\n');
    }
    Ok(out.trim_end().to_owned() + "\n")
}

#[cfg(test)]
mod tests {
    use super::prefix_of;

    #[test]
    fn reads_conventional_prefixes() {
        assert_eq!(prefix_of("fix(ui): button"), Some("fix"));
        assert_eq!(prefix_of("release: 2026.11"), Some("release"));
        assert_eq!(prefix_of("Merge origin/main into local main"), None);
        assert_eq!(prefix_of("Update the thing: now"), None);
    }
}
