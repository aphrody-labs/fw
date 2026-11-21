//! Stack generator: a goal in words becomes a role list, each role is filled with the best-ranked crate of its pool,
//! and the result is a Cargo project skeleton. Pools, profiles and per-crate hints live in `stack.toml`, never in code;
//! the choice among candidates is always the ranking.

use crate::model::Record;
use crate::store::{self, Dataset};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const CONFIG: &str = include_str!("../stack.toml");

#[derive(Debug, Deserialize)]
struct Config {
    role: Vec<Role>,
    profile: Vec<Profile>,
    #[serde(default)]
    addon: Vec<Addon>,
    #[serde(default, rename = "crate")]
    hints: BTreeMap<String, Hint>,
}

#[derive(Debug, Deserialize)]
struct Role {
    id: String,
    title: String,
    /// Crates considered first by name.
    #[serde(default)]
    pool: Vec<String>,
    /// Crates carrying any of these crates.io keywords or categories are candidates too.
    #[serde(default)]
    keywords: Vec<String>,
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    exclude: Vec<String>,
    /// Dynamic candidates need at least this many dependents.
    #[serde(default = "default_min_dependents")]
    min_dependents: u32,
}

fn default_min_dependents() -> u32 {
    50
}

#[derive(Debug, Deserialize)]
struct Profile {
    id: String,
    title: String,
    #[serde(rename = "match")]
    terms: Vec<String>,
    roles: Vec<String>,
    #[serde(default)]
    default: bool,
    /// Target the skeleton is checked for (`wasm32-unknown-unknown` for workers).
    target: Option<String>,
    /// `lib` skeletons get `src/lib.rs` with `crate-type = ["cdylib", "rlib"]`.
    #[serde(default)]
    lib: bool,
}

#[derive(Debug, Deserialize)]
struct Addon {
    #[serde(rename = "match")]
    terms: Vec<String>,
    roles: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct Hint {
    #[serde(default)]
    features: Vec<String>,
    /// Other crates this one needs next to it in `[dependencies]`.
    #[serde(default)]
    deps: Vec<String>,
    /// Top-level items for `src/main.rs` (or `src/lib.rs`).
    #[serde(default)]
    items: String,
    /// Statements inside `async fn main` (or `fn main`).
    #[serde(default)]
    main: String,
    /// The statements run after every other crate's (a server that never returns).
    #[serde(default)]
    last: bool,
    /// The crate needs an async `main` (tokio).
    #[serde(default)]
    tokio_main: bool,
}

fn config() -> Result<Config, String> {
    toml::from_str(CONFIG).map_err(|e| format!("stack.toml: {e}"))
}

fn words(goal: &str) -> BTreeSet<String> {
    goal.to_ascii_lowercase().split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_owned).collect()
}

fn term_hits(terms: &[String], goal: &str, ws: &BTreeSet<String>) -> usize {
    let g = goal.to_ascii_lowercase();
    terms.iter().filter(|t| if t.contains(' ') { g.contains(t.as_str()) } else { ws.contains(t.as_str()) }).count()
}

fn candidates<'a>(ds: &'a Dataset, role: &Role) -> Vec<&'a Record> {
    let mut seen = BTreeSet::new();
    let mut out: Vec<&Record> = Vec::new();
    let excluded = |r: &Record| role.exclude.iter().any(|e| store::key(e) == store::key(&r.name));
    for n in &role.pool {
        if let Some(r) = ds.get(n).filter(|r| !r.yanked && !excluded(r))
            && seen.insert(r.name.clone())
        {
            out.push(r);
        }
    }
    if !role.keywords.is_empty() || !role.categories.is_empty() {
        for r in &ds.records {
            if r.yanked || r.dependents < role.min_dependents || excluded(r) || seen.contains(&r.name) {
                continue;
            }
            if r.keywords.iter().any(|k| role.keywords.contains(k)) || r.categories.iter().any(|c| role.categories.contains(c)) {
                seen.insert(r.name.clone());
                out.push(r);
            }
        }
    }
    out.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.name.cmp(&b.name)));
    out
}

fn reason(r: &Record) -> String {
    format!(
        "{} downloads in 90 days, {} dependents, released {}, {}, MSRV {}{}",
        crate::awesome::thousands(r.recent_downloads),
        crate::awesome::thousands(u64::from(r.dependents)),
        r.last_release,
        if r.license.is_empty() { "no licence" } else { &r.license },
        if r.msrv.is_empty() { "undeclared" } else { &r.msrv },
        if r.advisories.open.is_empty() { String::new() } else { format!(", OPEN ADVISORY {}", r.advisories.open.join(" ")) }
    )
}

fn ident(name: &str) -> String {
    name.replace('-', "_")
}

fn msrv_key(v: &str) -> (u32, u32, u32) {
    let mut p = v.split('.').map(|x| x.parse::<u32>().unwrap_or(0));
    (p.next().unwrap_or(0), p.next().unwrap_or(0), p.next().unwrap_or(0))
}

/// Picks crates for `goal` and renders the project files (path to content).
pub fn generate(ds: &Dataset, goal: &str) -> Result<Value, String> {
    let cfg = config()?;
    let ws = words(goal);
    let profile = cfg
        .profile
        .iter()
        .map(|p| (term_hits(&p.terms, goal, &ws), p))
        .filter(|(h, _)| *h > 0)
        .max_by_key(|(h, _)| *h)
        .map(|(_, p)| p)
        .or_else(|| cfg.profile.iter().find(|p| p.default))
        .ok_or_else(|| "stack.toml defines no default profile".to_owned())?;

    let mut role_ids: Vec<&str> = profile.roles.iter().map(String::as_str).collect();
    for a in cfg.addon.iter().filter(|a| term_hits(&a.terms, goal, &ws) > 0) {
        for r in &a.roles {
            if !role_ids.contains(&r.as_str()) {
                role_ids.push(r);
            }
        }
    }

    let mut roles_out = Vec::new();
    let mut chosen: Vec<&Record> = Vec::new();
    for id in role_ids {
        let role = cfg.role.iter().find(|r| r.id == id).ok_or_else(|| format!("stack.toml: unknown role `{id}`"))?;
        let cands = candidates(ds, role);
        let Some(best) = cands.first() else {
            roles_out.push(json!({ "role": role.id, "title": role.title, "crate": null, "reason": "no candidate in the dataset" }));
            continue;
        };
        roles_out.push(json!({
            "role": role.id, "title": role.title, "crate": best.name, "version": best.version, "score": best.score,
            "reason": reason(best), "alternatives": cands.iter().skip(1).take(3).map(|r| r.name.as_str()).collect::<Vec<_>>(),
        }));
        if !chosen.iter().any(|c| c.name == best.name) {
            chosen.push(best);
        }
    }

    // Dependencies: the chosen crates plus what their hints ask for, features from the hints.
    let mut deps: BTreeMap<String, (String, Vec<String>)> = BTreeMap::new();
    let mut extra: Vec<&Record> = Vec::new();
    for r in &chosen {
        extra.push(r);
        for d in cfg.hints.get(&r.name).map(|h| h.deps.as_slice()).unwrap_or_default() {
            if let Some(dr) = ds.get(d) {
                extra.push(dr);
            }
        }
    }
    for r in extra {
        let feats = cfg.hints.get(&r.name).map(|h| h.features.clone()).unwrap_or_default();
        deps.entry(r.name.clone()).or_insert((r.version.clone(), feats));
    }

    let msrv = chosen.iter().filter(|r| !r.msrv.is_empty()).map(|r| r.msrv.as_str()).max_by_key(|m| msrv_key(m)).map(str::to_owned);
    let mut toml = format!("[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n", slug(goal));
    if let Some(m) = &msrv {
        toml.push_str(&format!("rust-version = \"{m}\"\n"));
    }
    if profile.lib {
        toml.push_str("\n[lib]\ncrate-type = [\"cdylib\", \"rlib\"]\n");
    }
    toml.push_str("\n[dependencies]\n");
    for (name, (ver, feats)) in &deps {
        if feats.is_empty() {
            toml.push_str(&format!("{name} = \"{ver}\"\n"));
        } else {
            let f: Vec<String> = feats.iter().map(|x| format!("\"{x}\"")).collect();
            toml.push_str(&format!("{name} = {{ version = \"{ver}\", features = [{}] }}\n", f.join(", ")));
        }
    }

    let mut items = String::new();
    let mut main_body = String::new();
    let mut main_last = String::new();
    let mut uses = String::new();
    let mut tokio_main = false;
    for r in &chosen {
        match cfg.hints.get(&r.name) {
            Some(h) if !h.items.is_empty() || !h.main.is_empty() => {
                items.push_str(&h.items);
                if h.last { main_last.push_str(&h.main) } else { main_body.push_str(&h.main) }
                tokio_main |= h.tokio_main;
            }
            _ => uses.push_str(&format!("use {} as _;\n", ident(&r.name))),
        }
    }
    main_body.push_str(&main_last);
    let src = if profile.lib {
        format!("{uses}{items}")
    } else if tokio_main {
        format!("{uses}{items}\n#[tokio::main]\nasync fn main() -> Result<(), Box<dyn std::error::Error>> {{\n{main_body}    Ok(())\n}}\n")
    } else {
        format!("{uses}{items}\nfn main() -> Result<(), Box<dyn std::error::Error>> {{\n{main_body}    Ok(())\n}}\n")
    };

    let mut files = serde_json::Map::new();
    files.insert("Cargo.toml".into(), json!(toml));
    files.insert(if profile.lib { "src/lib.rs" } else { "src/main.rs" }.into(), json!(src));
    if let Some(t) = &profile.target {
        files.insert(".cargo/config.toml".into(), json!(format!("[build]\ntarget = \"{t}\"\n")));
    }
    files.insert(".gitignore".into(), json!("target\nCargo.lock\n"));

    Ok(json!({
        "op": "stack", "goal": goal, "profile": profile.id, "profile_title": profile.title, "dump_date": ds.meta.dump_date,
        "rust_version": msrv, "target": profile.target, "roles": roles_out, "files": files,
    }))
}

fn slug(goal: &str) -> String {
    let s: String = goal.to_ascii_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let parts: Vec<&str> = s.split('-').filter(|p| !p.is_empty()).take(4).collect();
    let name = parts.join("-");
    if name.is_empty() || name.starts_with(|c: char| c.is_ascii_digit()) { format!("app-{name}").trim_end_matches('-').to_owned() } else { name }
}

/// Writes the project files of a `generate` result under `dir`.
pub fn write(v: &Value, dir: &Path) -> Result<Vec<String>, String> {
    let mut written = Vec::new();
    for (rel, content) in v["files"].as_object().ok_or("no files in the stack result")? {
        store::write_text(&dir.join(rel), content.as_str().unwrap_or_default())?;
        written.push(rel.clone());
    }
    Ok(written)
}

/// Runs `cargo check` in a generated project and returns whether it compiled, with the output tail.
pub fn check(dir: &Path) -> Value {
    let out = std::process::Command::new("cargo").arg("check").arg("--quiet").current_dir(dir).output();
    match out {
        Ok(o) => {
            let mut text = String::from_utf8_lossy(&o.stderr).into_owned();
            text.push_str(&String::from_utf8_lossy(&o.stdout));
            let tail: Vec<&str> = text.lines().rev().take(40).collect::<Vec<_>>().into_iter().rev().collect();
            json!({ "ok": o.status.success(), "exit": o.status.code(), "output": tail.join("\n") })
        }
        Err(e) => json!({ "ok": false, "error": format!("cannot run cargo: {e}") }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_parses_and_roles_resolve() {
        let cfg = config().unwrap();
        for p in &cfg.profile {
            for r in &p.roles {
                assert!(cfg.role.iter().any(|x| x.id == *r), "profile {} role {r}", p.id);
            }
        }
        for a in &cfg.addon {
            for r in &a.roles {
                assert!(cfg.role.iter().any(|x| x.id == *r), "addon role {r}");
            }
        }
        assert_eq!(cfg.profile.iter().filter(|p| p.default).count(), 1);
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("web API with Postgres and auth"), "web-api-with-postgres");
        assert_eq!(slug("3d game"), "app-3d-game");
        assert_eq!(msrv_key("1.100"), (1, 100, 0));
    }
}
