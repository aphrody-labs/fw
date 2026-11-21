use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const BASE: &str = "https://api.github.com";
/// GitHub asks for at least one second between mutating requests of one client (secondary rate limits).
const WRITE_SPACING: Duration = Duration::from_millis(1000);

#[derive(Clone)]
pub struct Github {
    agent: ureq::Agent,
    token: String,
    last_write: std::sync::Arc<Mutex<Instant>>,
}

pub struct Reply {
    pub status: u16,
    pub body: Value,
    pub next: Option<String>,
}

impl Github {
    /// The token comes from the named environment variable, else from `gh auth token`; it is never printed or put in an argument list.
    pub fn from_env(token_env: &str) -> Result<Self> {
        let token = match std::env::var(token_env).ok().filter(|value| !value.is_empty()) {
            Some(token) => token,
            None => {
                let output = Command::new("gh").args(["auth", "token"]).stderr(Stdio::null()).output().context("run gh auth token")?;
                let token = String::from_utf8(output.stdout).context("gh auth token output")?.trim().to_string();
                if token.is_empty() {
                    bail!("environment variable {token_env} is not set and `gh auth token` returned nothing");
                }
                token
            }
        };
        let config = ureq::Agent::config_builder().http_status_as_error(false).timeout_global(Some(Duration::from_secs(60))).build();
        Ok(Self { agent: config.into(), token, last_write: std::sync::Arc::new(Mutex::new(Instant::now() - WRITE_SPACING)) })
    }

    pub fn request(&self, method: &str, path: &str, body: Option<&Value>) -> Result<Reply> {
        let url = if path.starts_with("https://") { path.to_string() } else { format!("{BASE}{path}") };
        let authorization = format!("Bearer {}", self.token);
        let mutating = method != "GET";
        for attempt in 0..6u32 {
            if mutating {
                let mut last = self.last_write.lock().map_err(|_| anyhow!("write pacing lock poisoned"))?;
                let wait = WRITE_SPACING.saturating_sub(last.elapsed());
                std::thread::sleep(wait);
                *last = Instant::now();
            }
            let mut response = match (method, body) {
                ("GET", _) => self.agent.get(&url).header("Authorization", &authorization).header("Accept", "application/vnd.github+json").header("X-GitHub-Api-Version", "2022-11-28").call(),
                ("DELETE", None) => self.agent.delete(&url).header("Authorization", &authorization).header("Accept", "application/vnd.github+json").call(),
                ("POST", payload) => self.agent.post(&url).header("Authorization", &authorization).header("Accept", "application/vnd.github+json").content_type("application/json").send(payload.map(Value::to_string).unwrap_or_default()),
                ("PUT", payload) => self.agent.put(&url).header("Authorization", &authorization).header("Accept", "application/vnd.github+json").content_type("application/json").send(payload.map(Value::to_string).unwrap_or_default()),
                ("PATCH", payload) => self.agent.patch(&url).header("Authorization", &authorization).header("Accept", "application/vnd.github+json").content_type("application/json").send(payload.map(Value::to_string).unwrap_or_default()),
                _ => bail!("unsupported call {method} {path}"),
            }
            .with_context(|| format!("{method} {path}"))?;
            let status = response.status().as_u16();
            let next = response.headers().get("link").and_then(|value| value.to_str().ok()).and_then(next_link);
            let retry_after = response.headers().get("retry-after").and_then(|value| value.to_str().ok()).and_then(|value| value.parse::<u64>().ok());
            let text = response.body_mut().read_to_string().with_context(|| format!("read body of {method} {path}"))?;
            let limited = status == 429 || (status == 403 && (text.contains("rate limit") || text.contains("secondary"))) || (status == 409 && text.contains("in progress"));
            if (limited || status >= 500) && attempt < 5 {
                std::thread::sleep(Duration::from_secs(retry_after.unwrap_or(30 * u64::from(attempt + 1)).min(120)));
                continue;
            }
            let body = if text.is_empty() { Value::Null } else { serde_json::from_str(&text).unwrap_or(Value::String(text)) };
            return Ok(Reply { status, body, next });
        }
        Err(anyhow!("{method} {path}: gave up after retries"))
    }

    /// GET that must succeed.
    pub fn get(&self, path: &str) -> Result<Value> {
        let reply = self.request("GET", path, None)?;
        if reply.status >= 300 {
            bail!("GET {path}: HTTP {}: {}", reply.status, message(&reply.body));
        }
        Ok(reply.body)
    }

    /// GET that tolerates a refused endpoint (a scope the token lacks, a plan limit): the status and reason come back as the error.
    pub fn try_get(&self, path: &str) -> Result<Value, String> {
        match self.request("GET", path, None) {
            Ok(reply) if reply.status < 300 => Ok(reply.body),
            Ok(reply) => Err(format!("HTTP {}: {}", reply.status, message(&reply.body))),
            Err(error) => Err(error.to_string()),
        }
    }

    /// Every page; objects such as `{"total_count": n, "secrets": [...]}` yield the array under `key`.
    pub fn paged(&self, path: &str, key: Option<&str>) -> Result<Vec<Value>> {
        let separator = if path.contains('?') { '&' } else { '?' };
        let mut url = format!("{path}{separator}per_page=100");
        let mut rows = Vec::new();
        for _ in 0..500 {
            let reply = self.request("GET", &url, None)?;
            if reply.status >= 300 {
                bail!("GET {url}: HTTP {}: {}", reply.status, message(&reply.body));
            }
            let page = match key {
                Some(key) => reply.body[key].as_array().cloned().unwrap_or_default(),
                None => reply.body.as_array().cloned().unwrap_or_default(),
            };
            rows.extend(page);
            match reply.next {
                Some(next) => url = next,
                None => break,
            }
        }
        Ok(rows)
    }

    pub fn send(&self, method: &str, path: &str, body: Option<&Value>) -> Result<Value> {
        let reply = self.request(method, path, body)?;
        if reply.status >= 300 {
            bail!("{method} {path}: HTTP {}: {}", reply.status, message(&reply.body));
        }
        Ok(reply.body)
    }
}

pub fn message(body: &Value) -> String {
    let text = body["message"].as_str().map(str::to_string).unwrap_or_else(|| body.to_string());
    match body["errors"].as_array().filter(|errors| !errors.is_empty()) {
        Some(errors) => format!("{text} {}", Value::Array(errors.clone())),
        None => text,
    }
}

fn next_link(header: &str) -> Option<String> {
    header.split(',').find(|part| part.contains("rel=\"next\"")).and_then(|part| {
        let start = part.find('<')? + 1;
        let end = part.find('>')?;
        Some(part[start..end].to_string())
    })
}

/// Runs `work` over `items` on `threads` workers and returns the results in item order.
pub fn parallel<T: Sync, R: Send>(items: &[T], threads: usize, work: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let next = std::sync::atomic::AtomicUsize::new(0);
    let results: Mutex<Vec<(usize, R)>> = Mutex::new(Vec::with_capacity(items.len()));
    std::thread::scope(|scope| {
        for _ in 0..threads.max(1) {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(item) = items.get(index) else { break };
                    let result = work(item);
                    if let Ok(mut guard) = results.lock() {
                        guard.push((index, result));
                    }
                }
            });
        }
    });
    let mut collected = results.into_inner().unwrap_or_default();
    collected.sort_by_key(|(index, _)| *index);
    collected.into_iter().map(|(_, result)| result).collect()
}

impl Github {
    /// Raw text of a file through the contents API; `None` when the path does not exist.
    pub fn get_text(&self, path: &str) -> Result<Option<String>> {
        let url = format!("{BASE}{path}");
        let mut response = self.agent.get(&url).header("Authorization", &format!("Bearer {}", self.token)).header("Accept", "application/vnd.github.raw+json").call().with_context(|| format!("GET {path}"))?;
        match response.status().as_u16() {
            200 => Ok(Some(response.body_mut().read_to_string().with_context(|| format!("read body of {path}"))?)),
            404 => Ok(None),
            status => bail!("GET {path}: HTTP {status}"),
        }
    }
}

/// `YYYY-MM-DD` of the day `days` before now, in UTC.
pub fn date_days_ago(days: u64) -> String {
    let seconds = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|elapsed| elapsed.as_secs()).unwrap_or_default();
    let day = i64::try_from(seconds / 86_400).unwrap_or_default() - i64::try_from(days).unwrap_or_default();
    let shifted = day + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let month_day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 { month_index + 3 } else { month_index - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{month_day:02}")
}
