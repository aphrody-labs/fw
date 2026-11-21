//! End-to-end tests of the `fw-files` binary: paths in, JSON lines out, no network.

use serde_json::Value;
use std::process::{Command, Output, Stdio};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fw-files")).args(args).output().unwrap()
}

fn lines(out: &Output) -> Vec<Value> {
    String::from_utf8_lossy(&out.stdout).lines().map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("{l}: {e}"))).collect()
}

#[test]
fn ls_sorts_naturally_with_dirs_first_and_mime() {
    let d = tempfile::tempdir().unwrap();
    for n in ["f10.txt", "f2.txt", ".hidden"] {
        std::fs::write(d.path().join(n), "x").unwrap();
    }
    std::fs::create_dir(d.path().join("zdir")).unwrap();
    let p = d.path().to_str().unwrap();

    let out = run(&["ls", p, "--mime"]);
    assert!(out.status.success());
    let rows = lines(&out);
    let names: Vec<_> = rows.iter().map(|r| r["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["zdir", "f2.txt", "f10.txt"]);
    assert_eq!(rows[0]["mime"], "inode/directory");
    assert_eq!(rows[1]["mime"], "text/plain");

    let all = lines(&run(&["ls", p, "-a", "--sort", "size", "-r", "--no-dirs-first"]));
    assert_eq!(all.len(), 4);
    let bare = String::from_utf8(run(&["ls", p, "--names"]).stdout).unwrap();
    assert_eq!(bare.lines().count(), 3);
}

#[test]
fn mime_reads_paths_from_stdin_and_names_the_previewer() {
    let d = tempfile::tempdir().unwrap();
    let json = d.path().join("a.json");
    std::fs::write(&json, "{}").unwrap();
    let missing = d.path().join("gone");
    let mut child = Command::new(env!("CARGO_BIN_EXE_fw-files")).arg("mime").stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    {
        use std::io::Write;
        let stdin = child.stdin.as_mut().unwrap();
        writeln!(stdin, "{}\n{}\n{}", json.display(), d.path().display(), missing.display()).unwrap();
    }
    let out = child.wait_with_output().unwrap();
    assert!(!out.status.success(), "a missing path must fail the run");
    let rows = lines(&out);
    assert_eq!((rows[0]["mime"].as_str(), rows[0]["previewer"].as_str()), (Some("application/json"), Some("json")));
    assert_eq!((rows[1]["mime"].as_str(), rows[1]["previewer"].as_str()), (Some("inode/directory"), Some("folder")));
    assert!(rows[2]["error"].is_string());
}

#[test]
fn preview_returns_one_object() {
    let d = tempfile::tempdir().unwrap();
    let f = d.path().join("n.txt");
    std::fs::write(&f, "one\ntwo\nthree\n").unwrap();
    let rows = lines(&run(&["preview", f.to_str().unwrap(), "--lines", "2"]));
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["lines"], serde_json::json!(["one", "two"]));
    assert_eq!((rows[0]["kind"].as_str(), rows[0]["truncated"].as_bool()), (Some("text"), Some(true)));
}

#[test]
fn cp_mv_rm_stream_events_and_change_the_disk() {
    let d = tempfile::tempdir().unwrap();
    let (a, b) = (d.path().join("a"), d.path().join("b"));
    std::fs::write(&a, "payload").unwrap();
    std::fs::write(&b, "second").unwrap();
    let out_dir = d.path().join("out");
    std::fs::create_dir(&out_dir).unwrap();

    let cp = run(&["cp", a.to_str().unwrap(), b.to_str().unwrap(), out_dir.to_str().unwrap(), "--jobs", "2"]);
    assert!(cp.status.success(), "{}", String::from_utf8_lossy(&cp.stderr));
    let events = lines(&cp);
    let finished: Vec<_> = events.iter().filter(|e| e["event"] == "finished").collect();
    assert_eq!(finished.len(), 2);
    assert!(finished.iter().all(|e| e["status"] == "done" && e["files_done"] == 1));
    assert_eq!(std::fs::read(out_dir.join("a")).unwrap(), b"payload");

    // A second copy collides: default policy fails, --on-conflict overwrite succeeds.
    let again = run(&["cp", a.to_str().unwrap(), out_dir.to_str().unwrap()]);
    assert!(!again.status.success());
    assert!(lines(&again).iter().any(|e| e["event"] == "error"));
    assert!(run(&["cp", a.to_str().unwrap(), out_dir.to_str().unwrap(), "--on-conflict", "overwrite"]).status.success());

    let moved = d.path().join("moved");
    assert!(run(&["mv", out_dir.join("a").to_str().unwrap(), moved.to_str().unwrap()]).status.success());
    assert!(moved.exists() && !out_dir.join("a").exists());

    assert!(run(&["rm", moved.to_str().unwrap(), out_dir.to_str().unwrap()]).status.success());
    assert!(!moved.exists() && !out_dir.exists());
}

#[test]
fn rules_dump_yazi_previewers() {
    let rows = lines(&run(&["rules"]));
    assert!(rows.iter().any(|r| r["mime"] == "image/*" && r["run"] == "image"));
    assert_eq!(lines(&run(&["rules", "fetchers"])).len(), 4);
    assert!(!run(&["rules", "bogus"]).status.success());
}

#[test]
fn bad_arguments_fail_with_a_message() {
    let out = run(&["ls", "/definitely/not/here"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).starts_with("fw-files:"));
    assert!(!run(&["ls", ".", "--sort", "sideways"]).status.success());
}
