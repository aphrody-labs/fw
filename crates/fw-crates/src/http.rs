//! HTTP access to crates.io hosts under the crawler policy (https://crates.io/policies#crawlers):
//! a User-Agent that names the client and where to read about it, and at most one request per second.

use std::io::Read;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const USER_AGENT: &str = concat!("fw-crates/", env!("CARGO_PKG_VERSION"), " (generic fw client; +https://github.com/aphrody-labs/fw)");

static LAST: Mutex<Option<Instant>> = Mutex::new(None);

/// Blocks until one second has passed since the previous throttled request.
pub fn throttle() {
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(at) = *last {
        let wait = Duration::from_secs(1).saturating_sub(at.elapsed());
        if !wait.is_zero() {
            std::thread::sleep(wait);
        }
    }
    *last = Some(Instant::now());
}

fn agent(timeout: Option<Duration>) -> ureq::Agent {
    ureq::Agent::config_builder().user_agent(USER_AGENT).timeout_global(timeout).build().into()
}

/// Opens a streaming reader on `url` (no size limit, no global timeout: the dump is gigabytes).
pub fn open(url: &str) -> Result<Box<dyn Read + Send>, String> {
    throttle();
    let resp = agent(None).get(url).call().map_err(|e| format!("GET {url}: {e}"))?;
    Ok(Box::new(resp.into_body().into_reader()))
}

pub fn get_string(url: &str, accept: Option<&str>) -> Result<String, String> {
    throttle();
    let mut req = agent(Some(Duration::from_secs(30))).get(url);
    if let Some(a) = accept {
        req = req.header("Accept", a);
    }
    req.call().map_err(|e| format!("GET {url}: {e}"))?.into_body().read_to_string().map_err(|e| format!("GET {url}: {e}"))
}

/// GET with an optional GitHub token (`GITHUB_TOKEN` or `GH_TOKEN`) for api.github.com, which rate-limits anonymous callers per IP.
pub fn get_github(url: &str, accept: &str) -> Result<String, String> {
    throttle();
    let mut req = agent(Some(Duration::from_secs(30))).get(url).header("Accept", accept);
    if let Some(token) = ["GITHUB_TOKEN", "GH_TOKEN"].iter().find_map(|v| std::env::var(v).ok().filter(|t| !t.is_empty())) {
        req = req.header("Authorization", &format!("Bearer {token}"));
    }
    req.call().map_err(|e| format!("GET {url}: {e}"))?.into_body().read_to_string().map_err(|e| format!("GET {url}: {e}"))
}
