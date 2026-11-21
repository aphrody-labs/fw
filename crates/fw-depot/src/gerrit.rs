//! A Gerrit REST client: the calls `git cl` makes (query changes, get a change, submit, abandon, set a topic or reviewers) with the three ways a
//! depot_tools user authenticates: a bearer token (`FW_GERRIT_TOKEN`, or `gcloud auth print-access-token` when `FW_GERRIT_AUTH=gcloud`), the
//! cookies of `~/.gitcookies` (or git's `http.cookiefile`), and none for a public read.
//!
//! Every mutating call goes through `Client::send`, which does nothing but describe the request when `dry_run` is set.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::error::{Error, Result};
use crate::http;

/// How a request proves who sends it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Auth {
    None,
    Bearer(String),
    Cookie(String),
}

/// The cookie header for `host` from a Netscape cookie file (`.gitcookies`).
pub fn cookie_for(text: &str, host: &str) -> Option<String> {
    let mut found = Vec::new();
    for line in text.lines() {
        let line = line.strip_prefix("#HttpOnly_").unwrap_or(line);
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        let [domain, _, _, _, _, name, value] = fields.as_slice() else {
            continue;
        };
        let domain = domain.trim_start_matches('.');
        if host == domain || host.ends_with(&format!(".{domain}")) {
            found.push(format!("{name}={value}"));
        }
    }
    (!found.is_empty()).then(|| found.join("; "))
}

fn cookie_file() -> Option<PathBuf> {
    let configured = crate::git::run(Path::new("."), &["config", "--get", "http.cookiefile"]).ok();
    let path = configured.filter(|path| !path.is_empty()).map(PathBuf::from);
    path.or_else(|| {
        std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(|home| PathBuf::from(home).join(".gitcookies"))
    })
    .filter(|path| path.is_file())
}

/// The authentication to use for a Gerrit host.
pub fn authenticate(host: &str) -> Auth {
    if let Ok(token) = std::env::var("FW_GERRIT_TOKEN")
        && !token.is_empty()
    {
        return Auth::Bearer(token);
    }
    if std::env::var("FW_GERRIT_AUTH").is_ok_and(|value| value == "gcloud")
        && let Ok(output) = std::process::Command::new("gcloud")
            .args(["auth", "print-access-token"])
            .output()
        && output.status.success()
    {
        return Auth::Bearer(String::from_utf8_lossy(&output.stdout).trim().to_owned());
    }
    if let Some(file) = cookie_file()
        && let Ok(text) = std::fs::read_to_string(&file)
        && let Some(cookie) = cookie_for(&text, host)
    {
        return Auth::Cookie(cookie);
    }
    Auth::None
}

#[derive(Debug, Clone)]
pub struct Client {
    /// `https://chromium-review.googlesource.com`
    pub base: String,
    pub auth: Auth,
    pub dry_run: bool,
}

/// What a dry run reports instead of sending.
#[derive(Debug, Clone, PartialEq)]
pub struct Described {
    pub method: String,
    pub url: String,
    pub body: Option<Value>,
}

pub enum Outcome {
    Sent(Value),
    Skipped(Described),
}

impl Client {
    pub fn new(base: &str) -> Client {
        let base = base.trim_end_matches('/').to_owned();
        let host = base
            .split("://")
            .nth(1)
            .unwrap_or(&base)
            .split('/')
            .next()
            .unwrap_or_default()
            .to_owned();
        Client {
            auth: authenticate(&host),
            base,
            dry_run: false,
        }
    }

    fn headers(&self) -> Vec<(&'static str, String)> {
        match &self.auth {
            Auth::None => Vec::new(),
            Auth::Bearer(token) => vec![("Authorization", format!("Bearer {token}"))],
            Auth::Cookie(cookie) => vec![("Cookie", cookie.clone())],
        }
    }

    /// The path prefix of an authenticated request: `/a` when credentials are present.
    fn prefix(&self) -> &'static str {
        if self.auth == Auth::None { "" } else { "/a" }
    }

    pub fn get(&self, path: &str) -> Result<Value> {
        let url = format!("{}{}{path}", self.base, self.prefix());
        let response = http::get(&url, &self.headers())?;
        http::parse_json(&url, &response)
    }

    fn send(&self, method: &str, path: &str, body: Option<Value>) -> Result<Outcome> {
        let url = format!("{}{}{path}", self.base, self.prefix());
        if self.dry_run {
            return Ok(Outcome::Skipped(Described {
                method: method.to_owned(),
                url,
                body,
            }));
        }
        let payload = body.clone().unwrap_or_else(|| json!({}));
        let value = http::send_json(method, &url, &self.headers(), &payload)?;
        Ok(Outcome::Sent(value))
    }

    /// `GET /changes/?q=...` with the options git cl asks for.
    pub fn query(&self, query: &str, limit: usize) -> Result<Vec<Value>> {
        let encoded = percent_encode(query);
        let reply = self.get(&format!(
            "/changes/?q={encoded}&n={limit}&o=CURRENT_REVISION&o=LABELS&o=DETAILED_ACCOUNTS&o=CURRENT_COMMIT"
        ))?;
        Ok(reply.as_array().cloned().unwrap_or_default())
    }

    /// `GET /changes/{id}` where `id` is a number or `project~number`.
    pub fn change(&self, id: &str) -> Result<Value> {
        self.get(&format!(
            "/changes/{}?o=CURRENT_REVISION&o=LABELS&o=DETAILED_ACCOUNTS&o=CURRENT_COMMIT&o=ALL_REVISIONS",
            percent_encode(id)
        ))
    }

    /// `POST /changes/{id}/submit`: lands the change.
    pub fn submit(&self, id: &str) -> Result<Outcome> {
        self.send("POST", &format!("/changes/{}/submit", percent_encode(id)), Some(json!({})))
    }

    pub fn abandon(&self, id: &str, message: &str) -> Result<Outcome> {
        self.send(
            "POST",
            &format!("/changes/{}/abandon", percent_encode(id)),
            Some(json!({ "message": message })),
        )
    }

    /// `POST /changes/{id}/revisions/current/review`: a message and labels (`Commit-Queue: 2` starts a CQ run).
    pub fn review(&self, id: &str, message: &str, labels: &[(String, i32)]) -> Result<Outcome> {
        let labels: serde_json::Map<String, Value> = labels
            .iter()
            .map(|(name, vote)| (name.clone(), json!(vote)))
            .collect();
        self.send(
            "POST",
            &format!("/changes/{}/revisions/current/review", percent_encode(id)),
            Some(json!({ "message": message, "labels": labels })),
        )
    }

    pub fn set_topic(&self, id: &str, topic: &str) -> Result<Outcome> {
        self.send(
            "PUT",
            &format!("/changes/{}/topic", percent_encode(id)),
            Some(json!({ "topic": topic })),
        )
    }

    pub fn add_reviewer(&self, id: &str, account: &str, state: &str) -> Result<Outcome> {
        self.send(
            "POST",
            &format!("/changes/{}/reviewers", percent_encode(id)),
            Some(json!({ "reviewer": account, "state": state })),
        )
    }
}

pub fn percent_encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => out.push(byte as char),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// `refs/changes/NN/CHANGE/PATCHSET`: where Gerrit keeps a patch set.
pub fn change_ref(change: u64, patchset: u64) -> String {
    format!("refs/changes/{:02}/{change}/{patchset}", change % 100)
}

/// The ref and push options `git cl upload` pushes to: `refs/for/BRANCH%topic=...,r=...`.
pub fn upload_ref(target: &str, options: &UploadOptions) -> String {
    let mut parts = Vec::new();
    for reviewer in &options.reviewers {
        parts.push(format!("r={reviewer}"));
    }
    for cc in &options.cc {
        parts.push(format!("cc={cc}"));
    }
    if let Some(topic) = &options.topic {
        parts.push(format!("topic={topic}"));
    }
    if options.wip {
        parts.push("wip".to_owned());
    }
    if options.ready {
        parts.push("ready".to_owned());
    }
    if let Some(message) = &options.message {
        parts.push(format!("m={}", push_option_encode(message)));
    }
    for hashtag in &options.hashtags {
        parts.push(format!("hashtag={hashtag}"));
    }
    if options.cq_dry_run {
        parts.push("l=Commit-Queue+1".to_owned());
    }
    if parts.is_empty() {
        format!("refs/for/{target}")
    } else {
        format!("refs/for/{target}%{}", parts.join(","))
    }
}

fn push_option_encode(message: &str) -> String {
    message
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (byte as char).to_string(),
            b' ' => "_".to_owned(),
            other => format!("%{other:02x}"),
        })
        .collect()
}

#[derive(Debug, Clone, Default)]
pub struct UploadOptions {
    pub reviewers: Vec<String>,
    pub cc: Vec<String>,
    pub topic: Option<String>,
    pub wip: bool,
    pub ready: bool,
    pub message: Option<String>,
    pub hashtags: Vec<String>,
    pub cq_dry_run: bool,
}

/// Error for an HTTP failure that names the host.
pub fn unreachable(base: &str, message: &str) -> Error {
    Error::message(format!("{base}: {message}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gitcookies_match_the_host_and_its_parent_domain() {
        let file = "# comment\n.googlesource.com\tTRUE\t/\tTRUE\t2147483647\to\tgit-user=1/secret\n#HttpOnly_chromium-review.googlesource.com\tFALSE\t/\tTRUE\t0\tSID\tabc\n";
        assert_eq!(
            cookie_for(file, "chromium-review.googlesource.com").as_deref(),
            Some("o=git-user=1/secret; SID=abc")
        );
        assert_eq!(cookie_for(file, "example.com"), None);
    }

    #[test]
    fn upload_refs_carry_the_push_options() {
        let options = UploadOptions {
            reviewers: vec!["a@x.org".into()],
            topic: Some("t".into()),
            wip: true,
            message: Some("fix it".into()),
            ..UploadOptions::default()
        };
        assert_eq!(
            upload_ref("main", &options),
            "refs/for/main%r=a@x.org,topic=t,wip,m=fix_it"
        );
        assert_eq!(change_ref(12345, 3), "refs/changes/45/12345/3");
        assert_eq!(upload_ref("main", &UploadOptions::default()), "refs/for/main");
    }
}
