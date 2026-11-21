//! Polite HTTP: per-host pacing, retry with backoff and Retry-After, robots.txt, conditional GET.

use anyhow::{Result, anyhow};
use std::collections::{HashMap, VecDeque};
use std::sync::mpsc;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

pub const USER_AGENT: &str = "fw-corpus/4.2 (+https://aphrody.com; local search index, polite crawler)";
const MAX_BODY: u64 = 64 << 20;

pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

impl Response {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// Validator string for the next conditional request.
    pub fn validator(&self) -> Option<String> {
        self.etag
            .as_ref()
            .map(|etag| format!("etag:{etag}"))
            .or_else(|| self.last_modified.as_ref().map(|date| format!("lm:{date}")))
    }
}

pub struct Client {
    agent: ureq::Agent,
    interval: Duration,
    next_slot: Mutex<HashMap<String, Instant>>,
    robots: Mutex<HashMap<String, Robots>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn host_of(url: &str) -> &str {
    url.split("://").nth(1).unwrap_or(url).split('/').next().unwrap_or("")
}

impl Client {
    /// `interval_ms` is the minimum spacing between two requests to the same host.
    pub fn new(interval_ms: u64) -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(60)))
            .http_status_as_error(false)
            .user_agent(USER_AGENT)
            .build()
            .into();
        Self { agent, interval: Duration::from_millis(interval_ms), next_slot: Mutex::default(), robots: Mutex::default() }
    }

    fn pace(&self, host: &str) {
        let wait = {
            let mut slots = lock(&self.next_slot);
            let now = Instant::now();
            let slot = slots.get(host).copied().filter(|slot| *slot > now).unwrap_or(now);
            slots.insert(host.to_owned(), slot + self.interval);
            slot.saturating_duration_since(now)
        };
        if !wait.is_zero() {
            std::thread::sleep(wait);
        }
    }

    pub fn get(&self, url: &str) -> Result<Response> {
        self.request(url, &[], None)
    }

    pub fn get_with(&self, url: &str, headers: &[(&str, &str)]) -> Result<Response> {
        self.request(url, headers, None)
    }

    /// Conditional GET: `validator` is `etag:..` or `lm:..` from [`Response::validator`]; 304 means unchanged.
    pub fn get_conditional(&self, url: &str, validator: Option<&str>) -> Result<Response> {
        self.request(url, &[], validator)
    }

    fn request(&self, url: &str, headers: &[(&str, &str)], validator: Option<&str>) -> Result<Response> {
        let host = host_of(url).to_owned();
        let mut last_error = anyhow!("no attempt");
        for attempt in 0..5u32 {
            self.pace(&host);
            let mut request = self.agent.get(url);
            for (name, value) in headers {
                request = request.header(*name, *value);
            }
            match validator.and_then(|value| value.split_once(':')) {
                Some(("etag", etag)) => request = request.header("If-None-Match", etag),
                Some(("lm", date)) => request = request.header("If-Modified-Since", date),
                _ => {}
            }
            match request.call() {
                Ok(mut response) => {
                    let status = response.status().as_u16();
                    let header = |name: &str| {
                        response.headers().get(name).and_then(|value| value.to_str().ok()).map(str::to_owned)
                    };
                    let retry_after = header("retry-after").and_then(|value| value.trim().parse::<u64>().ok());
                    let (etag, last_modified) = (header("etag"), header("last-modified"));
                    if status == 429 || status >= 500 {
                        last_error = anyhow!("{url}: HTTP {status}");
                        let backoff = retry_after.unwrap_or(5u64 << attempt).min(120);
                        std::thread::sleep(Duration::from_secs(backoff));
                        continue;
                    }
                    let body = if status == 304 {
                        Vec::new()
                    } else {
                        response.body_mut().with_config().limit(MAX_BODY).read_to_vec()?
                    };
                    return Ok(Response { status, body, etag, last_modified });
                }
                Err(error) => {
                    last_error = anyhow!("{url}: {error}");
                    std::thread::sleep(Duration::from_secs(2u64 << attempt));
                }
            }
        }
        Err(last_error)
    }

    /// robots.txt check for `url` (group for `fw-corpus`, else `*`); unreachable robots allows.
    pub fn allowed(&self, url: &str) -> bool {
        let Some((scheme, rest)) = url.split_once("://") else { return true };
        let (host, path) = rest.split_once('/').map_or((rest, String::new()), |(host, path)| (host, format!("/{path}")));
        let origin = format!("{scheme}://{host}");
        if let Some(robots) = lock(&self.robots).get(&origin) {
            return robots.allows(&path);
        }
        let robots = match self.get(&format!("{origin}/robots.txt")) {
            Ok(response) if response.status == 200 => Robots::parse(&response.text()),
            _ => Robots::default(),
        };
        let allowed = robots.allows(&path);
        lock(&self.robots).insert(origin, robots);
        allowed
    }
}

#[derive(Default)]
pub struct Robots {
    rules: Vec<(bool, String)>,
}

/// User-agent names and the (allow, path) rules that apply to them.
type RobotsGroup = (Vec<String>, Vec<(bool, String)>);

impl Robots {
    pub fn parse(text: &str) -> Self {
        let mut groups: Vec<RobotsGroup> = Vec::new();
        let mut collecting_agents = false;
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            let Some((field, value)) = line.split_once(':') else { continue };
            let (field, value) = (field.trim().to_ascii_lowercase(), value.trim());
            match field.as_str() {
                "user-agent" => {
                    if !collecting_agents {
                        groups.push((Vec::new(), Vec::new()));
                    }
                    collecting_agents = true;
                    if let Some(group) = groups.last_mut() {
                        group.0.push(value.to_ascii_lowercase());
                    }
                }
                "allow" | "disallow" => {
                    collecting_agents = false;
                    if let Some(group) = groups.last_mut()
                        && !value.is_empty()
                    {
                        group.1.push((field == "allow", value.to_owned()));
                    }
                }
                _ => {}
            }
        }
        let pick = |name: &str| groups.iter().find(|group| group.0.iter().any(|agent| agent == name));
        let rules = pick("fw-corpus").or_else(|| pick("*")).map(|group| group.1.clone()).unwrap_or_default();
        Self { rules }
    }

    /// Longest matching rule wins; allow wins ties.
    pub fn allows(&self, path: &str) -> bool {
        let mut best: Option<(usize, bool)> = None;
        for (allow, pattern) in &self.rules {
            if glob(pattern, path) && best.is_none_or(|(length, was_allow)| pattern.len() > length || (pattern.len() == length && *allow && !was_allow)) {
                best = Some((pattern.len(), *allow));
            }
        }
        best.is_none_or(|(_, allow)| allow)
    }
}

/// robots pattern match: `*` wildcard, trailing `$` anchor, otherwise prefix.
fn glob(pattern: &str, text: &str) -> bool {
    let (pattern, anchored) = pattern.strip_suffix('$').map_or((pattern, false), |stripped| (stripped, true));
    fn walk(pattern: &[u8], text: &[u8], anchored: bool) -> bool {
        match pattern.split_first() {
            None => !anchored || text.is_empty(),
            Some((b'*', rest)) => (0..=text.len()).any(|skip| walk(rest, &text[skip..], anchored)),
            Some((byte, rest)) => text.first() == Some(byte) && walk(rest, &text[1..], anchored),
        }
    }
    walk(pattern.as_bytes(), text.as_bytes(), anchored)
}

/// Run `work` over `items` on `threads` workers; `sink` receives results on the calling thread
/// (so a single SQLite connection can write them).
pub fn pipeline<T: Send, R: Send>(
    items: Vec<T>,
    threads: usize,
    work: impl Fn(T) -> Option<R> + Sync,
    mut sink: impl FnMut(R),
) {
    let queue = Mutex::new(items.into_iter().collect::<VecDeque<_>>());
    let (sender, receiver) = mpsc::sync_channel(64);
    std::thread::scope(|scope| {
        for _ in 0..threads.max(1) {
            let sender = sender.clone();
            let (queue, work) = (&queue, &work);
            scope.spawn(move || {
                while let Some(item) = lock(queue).pop_front() {
                    if let Some(result) = work(item)
                        && sender.send(result).is_err()
                    {
                        break;
                    }
                }
            });
        }
        drop(sender);
        for result in receiver {
            sink(result);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn robots_longest_match() {
        let robots = Robots::parse("User-agent: *\nDisallow: /*?*q=\nDisallow: /archive\nAllow: /archive/ok\n");
        assert!(!robots.allows("/a?b=1&q=x"));
        assert!(!robots.allows("/archive/x"));
        assert!(robots.allows("/archive/ok"));
        assert!(robots.allows("/components"));
    }

    #[test]
    fn gitiles_root_listing_is_disallowed_but_files_are_not() {
        let robots = Robots::parse("User-agent: *\nDisallow: /?format=TEXT\nDisallow: /*/+log/*\n");
        assert!(!robots.allows("/?format=TEXT"));
        assert!(robots.allows("/chromium/src/+/HEAD/docs/a.md?format=TEXT"));
        assert!(!robots.allows("/chromium/src/+log/main"));
    }
}
