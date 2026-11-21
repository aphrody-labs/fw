//! End to end on a throwaway repository with a bare origin: harvest, distill, search, publish, and a second publish that
//! must change nothing.

use fw_learn::products::{Repo, discover_products};
use fw_learn::store::Store;
use fw_learn::{HarvestOptions, distill, publish, search};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git").arg("-C").arg(dir).args(args).output().expect("run git");
    assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn write(dir: &Path, path: &str, text: &str) {
    let full = dir.join(path);
    std::fs::create_dir_all(full.parent().expect("parent")).expect("create dirs");
    std::fs::write(full, text).expect("write file");
}

fn commit(dir: &Path, message: &str) {
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", message]);
}

fn fixture() -> (PathBuf, PathBuf) {
    let base = std::env::temp_dir().join(format!("fw-learn-engine-{}", std::process::id()));
    std::fs::remove_dir_all(&base).ok();
    let (origin, work) = (base.join("origin.git"), base.join("work"));
    std::fs::create_dir_all(&work).expect("create work");
    git(&base, &["init", "-q", "--bare", "-b", "main", origin.to_str().expect("utf8")]);
    git(&work, &["init", "-q", "-b", "main"]);
    git(&work, &["config", "user.name", "Test"]);
    git(&work, &["config", "user.email", "test@example.invalid"]);
    git(&work, &["remote", "add", "origin", origin.to_str().expect("utf8")]);
    write(&work, "product.toml", "name = \"demo\"\ndescription = \"A demo product for the engine test.\"\n");
    write(&work, "AGENTS.md", "# Contract\n\n- Never edit generated files by hand, because the generator overwrites them.\n- plain description line\n");
    write(&work, "README.md", "# demo\n\nA demo product for the engine test, with enough words to be a paragraph.\n");
    for name in ["alpha", "beta", "gamma"] {
        write(&work, &format!("crates/{name}/Cargo.toml"), &format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\ndescription = \"crate {name}\"\n[dependencies]\nalpha = {{ path = \"../alpha\" }}\n"));
        write(&work, &format!("crates/{name}/src/lib.rs"), "pub fn run() {}\n");
    }
    commit(&work, "feat(alpha): first version");
    write(&work, "crates/alpha/src/lib.rs", "pub fn run() { /* fixed */ }\n");
    commit(&work, "fix(alpha): crash in run");
    write(&work, "crates/beta/src/lib.rs", "pub fn run() { /* changed */ }\n");
    commit(&work, "feat(beta): more");
    git(&work, &["push", "-q", "origin", "main"]);
    git(&work, &["fetch", "-q", "origin", "main"]);
    (base, work)
}

#[test]
fn harvests_distills_searches_and_publishes_idempotently() {
    let (base, work) = fixture();
    let store = Store::open(&base.join("store")).expect("open store");
    let options = HarvestOptions {
        repos: vec![work.clone()],
        rev: None,
        max_commits: 1000,
        max_issues: 0,
        releases_url: String::new(),
        graphs: false,
        remote: false,
        graph_limit: Duration::from_secs(1),
        graph_interval: 3600,
    };
    let report = fw_learn::harvest(&store, &options).expect("harvest");
    assert_eq!(report["work"]["commits"]["stored"], 3, "{report}");
    assert_eq!(report["work"]["units"]["stored"], 3, "{report}");

    assert!(distill::distill(&store).expect("distill") > 0);
    let found = search(&store, "crash", 5, "", "").expect("search");
    assert!(found["hits"].as_array().is_some_and(|hits| hits.iter().any(|hit| hit["kind"] == "commit" && hit["title"] == "fix(alpha): crash in run")), "{found}");
    let rules = distill::render(&store, "demo", false).expect("render");
    assert!(rules.contains("Never edit generated files by hand"), "{rules}");
    assert!(rules.contains("`crates/alpha` has 1 fix commits"), "{rules}");
    assert!(rules.contains("`alpha` (`crates/alpha`) is a dependency of 3 units"), "{rules}");

    let repo = Repo::open(&work, Some("origin/main")).expect("open repo");
    let products = discover_products(&repo, &repo.tree().expect("tree")).expect("products");
    let build = || publish::plan(&store, &repo, &products);
    let first = publish::publish(&repo, &build, "learn: refresh the generated rules, llms.txt and learned rules (fw learn publish)", true).expect("publish");
    assert!(first.commit.is_some());
    assert!(first.changed.contains(&"AGENTS.md".to_owned()) && first.changed.contains(&"llms.txt".to_owned()), "{:?}", first.changed);
    let agents = git(&work, &["show", "origin/main:AGENTS.md"]);
    assert!(agents.contains("<!-- learn:begin demo ") && agents.contains("Never edit generated files by hand"));

    fw_learn::harvest(&store, &options).expect("second harvest");
    distill::distill(&store).expect("second distill");
    let second = publish::publish(&repo, &build, "learn: refresh the generated rules, llms.txt and learned rules (fw learn publish)", true).expect("second publish");
    assert!(second.changed.is_empty(), "a second publish must change nothing: {:?}\n{}", second.changed, git(&work, &["diff", first.commit.as_deref().unwrap_or("HEAD"), second.commit.as_deref().unwrap_or("HEAD"), "--", "llms.txt", "docs/knowledge/learned/demo.md"]));
    std::fs::remove_dir_all(base).ok();
}
