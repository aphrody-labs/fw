//! git cl against a local Gerrit-like server: nothing here talks to a real Gerrit.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::Command;
use std::sync::mpsc;

use fw_depot::gerrit::{Auth, Client, Outcome, UploadOptions};
use fw_depot::git_cl;

/// Serves a few Gerrit endpoints on 127.0.0.1 until the test ends; every request line and `Cookie` header is sent to the channel.
fn gerrit() -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = format!("http://{}", listener.local_addr().expect("address"));
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut request_line = String::new();
            if reader.read_line(&mut request_line).is_err() {
                continue;
            }
            let mut cookie = String::new();
            let mut length = 0usize;
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).is_err() || header.trim().is_empty() {
                    break;
                }
                let lower = header.to_ascii_lowercase();
                if let Some(value) = lower.strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap_or(0);
                }
                if lower.starts_with("cookie:") {
                    cookie = header.trim()["cookie:".len()..].trim().to_owned();
                }
            }
            let mut body = vec![0u8; length];
            reader.read_exact(&mut body).ok();
            let line = request_line.trim().to_owned();
            sender
                .send(format!("{line} cookie={cookie} body={}", String::from_utf8_lossy(&body)))
                .ok();
            let reply = if line.contains("/submit") {
                r#"{"id":"proj~123","status":"MERGED"}"#
            } else if line.contains("GET /a/changes/123") || line.contains("GET /changes/123") {
                r#"{"_number":123,"status":"NEW","subject":"fix the thing","current_revision":"abc","revisions":{"abc":{"_number":2,"commit":{"message":"fix the thing\n\nChange-Id: I1\n"}}}}"#
            } else if line.contains("/changes/?q=") {
                r#"[{"_number":123,"status":"NEW","subject":"fix the thing"}]"#
            } else {
                "{}"
            };
            let payload = format!(")]}}'\n{reply}");
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
                payload.len()
            )
            .ok();
        }
    });
    (address, receiver)
}

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "protocol.file.allow=always"])
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git runs");
    assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

#[test]
fn queries_and_changes_are_read_with_the_cookie() {
    let (base, requests) = gerrit();
    let mut client = Client::new(&base);
    client.auth = Auth::Cookie("o=git-user=1/secret".to_owned());
    let found = client.query("status:open owner:self", 5).expect("query");
    assert_eq!(found[0]["_number"], 123);
    let change = client.change("123").expect("change");
    assert_eq!(change["subject"], "fix the thing");
    let first = requests.recv().expect("request");
    assert!(first.starts_with("GET /a/changes/?q=status%3Aopen%20owner%3Aself"), "{first}");
    assert!(first.contains("cookie=o=git-user=1/secret"), "{first}");
}

#[test]
fn a_dry_run_sends_nothing_and_a_real_submit_does() {
    let (base, requests) = gerrit();
    let mut client = Client::new(&base);
    client.auth = Auth::Bearer("token".to_owned());
    client.dry_run = true;
    match client.submit("123").expect("dry run") {
        Outcome::Skipped(described) => {
            assert_eq!(described.method, "POST");
            assert!(described.url.ends_with("/a/changes/123/submit"), "{}", described.url);
        }
        Outcome::Sent(_) => panic!("a dry run must not send"),
    }
    assert!(requests.try_recv().is_err(), "the dry run reached the server");
    client.dry_run = false;
    let Outcome::Sent(reply) = client.submit("123").expect("submit") else {
        panic!("not sent");
    };
    assert_eq!(reply["status"], "MERGED");
    assert!(requests.recv().expect("request").starts_with("POST /a/changes/123/submit"));
}

#[test]
fn patch_fetches_the_change_ref_into_a_branch_and_remembers_the_issue() {
    let (base, _requests) = gerrit();
    let directory = tempfile::tempdir().expect("tempdir");
    let remote = directory.path().join("remote.git");
    let work = directory.path().join("work");
    let clone = directory.path().join("clone");
    std::fs::create_dir_all(&work).expect("mkdir");
    git(&work, &["init", "-q", "-b", "main"]);
    std::fs::write(work.join("a.txt"), "one\n").expect("write");
    git(&work, &["add", "-A"]);
    git(&work, &["commit", "-q", "-m", "base"]);
    std::fs::write(work.join("a.txt"), "two\n").expect("write");
    git(&work, &["commit", "-q", "-am", "patch set 2"]);
    let patch_set = git(&work, &["rev-parse", "HEAD"]);
    git(&work, &["clone", "-q", "--bare", ".", remote.to_str().expect("utf8")]);
    git(&remote, &["update-ref", "refs/changes/23/123/2", &patch_set]);
    git(directory.path(), &["clone", "-q", remote.to_str().expect("utf8"), clone.to_str().expect("utf8")]);

    let mut client = Client::new(&base);
    client.auth = Auth::None;
    let message = git_cl::patch(&clone, &client, "123", None).expect("patch");
    assert!(message.contains("patch set 2"), "{message}");
    assert_eq!(git(&clone, &["rev-parse", "HEAD"]), patch_set);
    assert_eq!(git_cl::current_branch(&clone).expect("branch"), "change-123");
    let found = git_cl::association(&clone, "change-123").expect("association");
    assert_eq!((found.issue, found.patchset), (123, Some(2)));
}

#[test]
fn upload_dry_run_prints_the_push_and_pushes_nothing() {
    let directory = tempfile::tempdir().expect("tempdir");
    git(directory.path(), &["init", "-q", "-b", "main"]);
    let options = UploadOptions {
        reviewers: vec!["a@example.org".to_owned()],
        topic: Some("t".to_owned()),
        ..UploadOptions::default()
    };
    let described = git_cl::upload(directory.path(), "main", &options, true).expect("dry run");
    assert_eq!(described, "git push origin HEAD:refs/for/main%r=a@example.org,topic=t");
}

#[test]
fn a_change_id_is_added_once() {
    let directory = tempfile::tempdir().expect("tempdir");
    git(directory.path(), &["init", "-q", "-b", "main"]);
    // ensure_change_id amends with the system git, which has no identity on CI runners: give the repo one.
    git(directory.path(), &["config", "user.name", "t"]);
    git(directory.path(), &["config", "user.email", "t@t"]);
    std::fs::write(directory.path().join("f"), "x").expect("write");
    git(directory.path(), &["add", "-A"]);
    git(directory.path(), &["commit", "-q", "-m", "subject"]);
    assert!(git_cl::ensure_change_id(directory.path()).expect("first"));
    let message = git(directory.path(), &["log", "-1", "--format=%B"]);
    assert!(message.contains("Change-Id: I"), "{message}");
    assert!(!git_cl::ensure_change_id(directory.path()).expect("second"));
}
