//! pin, sync and status against local git remotes that stand in for the forks.

use std::path::Path;
use std::process::Command;

use fw_linux::cli::run_with;
use fw_linux::pins::Pins;
use fw_linux::repo::{Remotes, git};

fn run(directory: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git").arg("-C").arg(directory).args(arguments).output().expect("git runs");
    assert!(output.status.success(), "git {arguments:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    std::fs::write(path, text).expect("write");
}

/// Two releases of a tiny "kernel": v1.0.0 and v1.0.1 (annotated), served as owner/<name>.git.
fn serve(base: &Path, name: &str) -> (String, String) {
    let work = base.join(format!("work-{name}"));
    std::fs::create_dir_all(&work).expect("mkdir");
    run(&work, &["init", "-q", "-b", "main"]);
    run(&work, &["config", "core.autocrlf", "false"]);
    run(&work, &["config", "user.name", "test"]);
    run(&work, &["config", "user.email", "test@example.com"]);
    write(&work.join("Makefile"), "all:\n");
    write(&work.join("include/uapi/linux/a.h"), "#define A 1\n");
    write(&work.join("drivers/big.c"), "int big;\n");
    run(&work, &["add", "-A"]);
    run(&work, &["commit", "-q", "-m", "one"]);
    run(&work, &["tag", "-a", "v1.0.0", "-m", "v1.0.0"]);
    let first = run(&work, &["rev-parse", "HEAD"]);
    write(&work.join("include/uapi/linux/a.h"), "#define A 2\n");
    run(&work, &["commit", "-q", "-a", "-m", "two"]);
    run(&work, &["tag", "-a", "v1.0.1", "-m", "v1.0.1"]);
    let second = run(&work, &["rev-parse", "HEAD"]);
    let bare = base.join("owner").join(format!("{name}.git"));
    std::fs::create_dir_all(bare.parent().expect("parent")).expect("mkdir");
    run(base, &["clone", "-q", "--bare", &work.display().to_string(), &bare.display().to_string()]);
    run(&bare, &["config", "uploadpack.allowFilter", "true"]);
    run(&bare, &["config", "uploadpack.allowAnySHA1InWant", "true"]);
    (first, second)
}

/// A `file://` URL of a local directory (`file:///C:/...` on Windows), so that git honours depth and filters.
fn file_url(directory: &Path) -> String {
    let path = directory.display().to_string().replace('\\', "/");
    if path.starts_with('/') { format!("file://{path}") } else { format!("file:///{path}") }
}

fn arguments(root: &Path, rest: &[&str]) -> Vec<String> {
    let mut all = vec![rest[0].to_owned(), "--root".to_owned(), root.display().to_string()];
    all.extend(rest[1..].iter().map(|argument| (*argument).to_owned()));
    all
}

#[test]
fn pin_sync_and_status_follow_the_tags() {
    let temporary = tempfile::tempdir().expect("tempdir");
    let base = temporary.path().join("remotes");
    let (first, second) = serve(&base, "fork");
    let remotes = Remotes::new(&file_url(&base));

    let root = temporary.path().join("fw");
    std::fs::create_dir_all(&root).expect("mkdir");
    run(&root, &["init", "-q", "-b", "main"]);
    let feed = temporary.path().join("releases.json");
    write(&feed, r#"{"latest_stable":{"version":"1.0.1"},"releases":[{"moniker":"stable","version":"1.0.1","iseol":false}]}"#);
    write(
        &root.join("kernel.toml"),
        &format!(
            "schema = 1\nreleases = \"{}\"\n\n[profile.uapi]\nfilter = \"blob:none\"\nsparse = [\"/*\", \"!/*/\", \"/include/uapi/\"]\n\n[profile.full]\n\n[[tree]]\nid = \"stable\"\nrole = \"test\"\npath = \"third_party/linux-stable\"\nrepo = \"owner/fork\"\nupstream = \"owner/fork\"\nmoniker = \"stable\"\nprofile = \"uapi\"\ntag = \"v1.0.0\"\ncommit = \"{first}\"\n",
            feed.display().to_string().replace('\\', "/")
        ),
    );

    assert_eq!(run_with(&arguments(&root, &["pin", "1.0.1"]), &remotes).expect("pin"), 0);
    let pins = Pins::load(&root.join("kernel.toml")).expect("pins");
    let tree = pins.tree("stable").expect("tree");
    assert_eq!(tree.tag.as_deref(), Some("v1.0.1"));
    assert_eq!(tree.commit, second, "the annotated tag is peeled to its commit");
    assert_eq!(fw_linux::repo::gitlink(&root, "third_party/linux-stable").as_deref(), Some(second.as_str()));

    assert_eq!(run_with(&arguments(&root, &["sync"]), &remotes).expect("sync"), 0);
    let checkout = root.join("third_party/linux-stable");
    assert_eq!(run(&checkout, &["rev-parse", "HEAD"]), second);
    assert!(checkout.join("include/uapi/linux/a.h").is_file(), "the sparse profile keeps include/uapi");
    assert!(!checkout.join("drivers/big.c").exists(), "the sparse profile leaves the drivers out");
    assert_eq!(std::fs::read_to_string(checkout.join("include/uapi/linux/a.h")).expect("read"), "#define A 2\n");

    let status = |extra: &[&str]| {
        let mut rest = vec!["status", "--check", "--strict"];
        rest.extend_from_slice(extra);
        run_with(&arguments(&root, &rest), &remotes).expect("status")
    };
    assert_eq!(status(&[]), 0, "pins match kernel.org, the fork and the gitlink");

    // Move back one release: the pin is now behind the feed, which only --strict reports.
    assert_eq!(run_with(&arguments(&root, &["pin", "v1.0.0"]), &remotes).expect("pin back"), 0);
    assert_eq!(status(&[]), 1);
    let lenient = run_with(&arguments(&root, &["status", "--check"]), &remotes).expect("status");
    assert_eq!(lenient, 0);

    // Syncing again moves the checkout back and re-fetches nothing it already has.
    assert_eq!(run_with(&arguments(&root, &["sync", "stable", "--profile", "full"]), &remotes).expect("sync full"), 0);
    assert!(checkout.join("drivers/big.c").is_file(), "the full profile checks everything out");
    assert_eq!(run(&checkout, &["rev-parse", "HEAD"]), first);
    assert!(git(Some(&checkout), &["status", "--porcelain"]).expect("status").trim().is_empty());

    // And back: the sparse profile removes what the full one checked out.
    assert_eq!(run_with(&arguments(&root, &["sync", "stable", "--profile", "uapi"]), &remotes).expect("sync uapi"), 0);
    assert!(!checkout.join("drivers/big.c").exists(), "going back to the sparse profile drops the drivers again");
    assert!(checkout.join("include/uapi/linux/a.h").is_file());
}

#[test]
fn a_tag_missing_from_the_fork_is_a_problem() {
    let temporary = tempfile::tempdir().expect("tempdir");
    let base = temporary.path().join("remotes");
    let (first, _) = serve(&base, "fork");
    let remotes = Remotes::new(&file_url(&base));
    let root = temporary.path().join("fw");
    std::fs::create_dir_all(&root).expect("mkdir");
    write(
        &root.join("kernel.toml"),
        &format!("schema = 1\nreleases = \"/nonexistent/releases.json\"\n\n[profile.full]\n\n[[tree]]\nid = \"stable\"\nrole = \"test\"\npath = \"third_party/x\"\nrepo = \"owner/fork\"\nupstream = \"owner/fork\"\nmoniker = \"stable\"\nprofile = \"full\"\ntag = \"v9.9.9\"\ncommit = \"{first}\"\n"),
    );
    let code = run_with(&arguments(&root, &["status", "--check", "--json"]), &remotes).expect("status");
    assert_eq!(code, 1, "a missing tag and an unreadable feed both fail --check");
}
