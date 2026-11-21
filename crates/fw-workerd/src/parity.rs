//! Differential parity: the same request list goes to the cloud URL and to the self-hosted URL; status, a fixed set of headers and the body hash
//! are compared. A side effect (an object written, a row inserted) is checked by a later request of the same list that reads it back.

use crate::{read_text, Error, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant};

/// Headers whose values must be equal on both sides unless a request lists allowed values for them.
const COMPARED: [&str; 13] = [
    "content-type",
    "cache-control",
    "etag",
    "content-length",
    "content-range",
    "accept-ranges",
    "content-security-policy",
    "content-location",
    "content-encoding",
    "vary",
    "location",
    "access-control-allow-origin",
    "x-content-type-options",
];

/// Headers that differ by construction: Cloudflare adds them at its edge, or they carry time.
pub const ALLOWED_DIFFERENCES: [&str; 15] = [
    "server",
    "cf-ray",
    "cf-cache-status",
    "date",
    "alt-svc",
    "nel",
    "report-to",
    "age",
    "server-timing",
    "cf-placement",
    "set-cookie",
    "strict-transport-security",
    "expect-ct",
    "connection",
    // The seeded copy has the time it was copied; an object written through the self-hosted mode has its own write time on both sides of a later run.
    "last-modified",
];

#[derive(Debug, Deserialize)]
pub struct Spec {
    pub name: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(rename = "request")]
    pub requests: Vec<RequestSpec>,
}

#[derive(Debug, Deserialize)]
pub struct RequestSpec {
    pub id: String,
    #[serde(default = "default_method")]
    pub method: String,
    pub path: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub body: Option<String>,
    /// For headers that legitimately take several values across modes (a cache layer answering for the same bytes).
    #[serde(default)]
    pub allow_values: BTreeMap<String, Vec<String>>,
    /// Headers the cloud edge adds or removes for this response (named with the reason in the spec), skipped for this request only.
    #[serde(default)]
    pub ignore_headers: Vec<String>,
    /// Time this request `--repeat` more times on each side (latency per request, not over the whole list).
    #[serde(default)]
    pub bench: bool,
    /// `ignore` skips the body comparison for responses that carry time or randomness.
    #[serde(default)]
    pub body_compare: Option<String>,
}

fn default_method() -> String {
    "GET".to_owned()
}

#[derive(Debug, Clone, Serialize)]
pub struct Observed {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body_sha256: String,
    pub body_bytes: usize,
    pub milliseconds: f64,
}

#[derive(Debug, Serialize)]
pub struct Verdict {
    pub id: String,
    pub passed: bool,
    pub differences: Vec<String>,
    pub cloud: Observed,
    pub selfhosted: Observed,
}

#[derive(Debug, Serialize)]
pub struct Latency {
    pub samples: usize,
    pub min_ms: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
}

#[derive(Debug, Serialize)]
pub struct RequestLatency {
    pub id: String,
    pub cloud: Latency,
    pub selfhosted: Latency,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub name: String,
    pub passed: bool,
    pub verdicts: Vec<Verdict>,
    pub latency: Vec<RequestLatency>,
    pub allowed_differences: Vec<&'static str>,
}

pub fn load_spec(path: &Path) -> Result<Spec> {
    let text = read_text(path)?;
    toml::from_str(&text).map_err(|error| Error::Parse { path: path.display().to_string(), message: error.to_string() })
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .max_redirects(0)
        .timeout_global(Some(Duration::from_secs(60)))
        .build()
        .into()
}

pub fn fetch(client: &ureq::Agent, base: &str, spec: &Spec, request: &RequestSpec) -> Result<Observed> {
    let url = format!("{}{}", base.trim_end_matches('/'), request.path);
    let mut builder = ureq::http::Request::builder().method(request.method.as_str()).uri(&url);
    let mut sent: BTreeMap<String, String> = spec.headers.clone();
    sent.extend(request.headers.clone());
    for (name, value) in &sent {
        builder = builder.header(name.as_str(), value.as_str());
    }
    let body = request.body.clone().unwrap_or_default().into_bytes();
    let built = builder.body(body).map_err(|error| Error::Http(format!("{url}: {error}")))?;
    let started = Instant::now();
    let mut response = client.run(built).map_err(|error| Error::Http(format!("{url}: {error}")))?;
    let bytes = response.body_mut().read_to_vec().map_err(|error| Error::Http(format!("{url}: {error}")))?;
    let milliseconds = started.elapsed().as_secs_f64() * 1000.0;
    let mut headers = BTreeMap::new();
    for (name, value) in response.headers() {
        if let Ok(text) = value.to_str() {
            headers.insert(name.as_str().to_ascii_lowercase(), text.to_owned());
        }
    }
    Ok(Observed {
        status: response.status().as_u16(),
        headers,
        body_sha256: hex::encode(Sha256::digest(&bytes)),
        body_bytes: bytes.len(),
        milliseconds,
    })
}

pub fn compare(request: &RequestSpec, cloud: &Observed, selfhosted: &Observed) -> Vec<String> {
    let mut differences = Vec::new();
    if cloud.status != selfhosted.status {
        differences.push(format!("status: cloud {} / self-hosted {}", cloud.status, selfhosted.status));
    }
    for name in COMPARED.into_iter().filter(|name| !request.ignore_headers.iter().any(|ignored| ignored.as_str() == *name)) {
        let (left, right) = (cloud.headers.get(name), selfhosted.headers.get(name));
        if let Some(allowed) = request.allow_values.get(name) {
            let inside = |value: Option<&String>| value.is_some_and(|text| allowed.iter().any(|candidate| candidate == text));
            if !(inside(left) && inside(right)) {
                differences.push(format!("{name}: cloud {left:?} / self-hosted {right:?} not both in {allowed:?}"));
            }
        } else if left != right {
            differences.push(format!("{name}: cloud {left:?} / self-hosted {right:?}"));
        }
    }
    for (name, allowed) in &request.allow_values {
        if COMPARED.contains(&name.as_str()) || request.ignore_headers.contains(name) {
            continue;
        }
        let (left, right) = (cloud.headers.get(name), selfhosted.headers.get(name));
        let inside = |value: Option<&String>| value.is_some_and(|text| allowed.iter().any(|candidate| candidate == text));
        if !(inside(left) && inside(right)) {
            differences.push(format!("{name}: cloud {left:?} / self-hosted {right:?} not both in {allowed:?}"));
        }
    }
    if request.body_compare.as_deref() != Some("ignore") && (cloud.body_sha256 != selfhosted.body_sha256 || cloud.body_bytes != selfhosted.body_bytes) {
        differences.push(format!(
            "body: cloud {} bytes sha256 {} / self-hosted {} bytes sha256 {}",
            cloud.body_bytes,
            &cloud.body_sha256[..16],
            selfhosted.body_bytes,
            &selfhosted.body_sha256[..16]
        ));
    }
    differences
}

fn latency(mut samples: Vec<f64>) -> Latency {
    samples.sort_by(f64::total_cmp);
    let at = |fraction: f64| samples.get(((samples.len() as f64 - 1.0) * fraction).round() as usize).copied().unwrap_or(0.0);
    Latency { samples: samples.len(), min_ms: at(0.0), p50_ms: at(0.5), p95_ms: at(0.95) }
}

/// Runs the request list once per side for the verdicts, then every request marked `bench` `repeat` more times per side for the latency figures.
pub fn run(spec: &Spec, cloud_base: &str, selfhosted_base: &str, repeat: usize) -> Result<Report> {
    let client = agent();
    let mut verdicts = Vec::new();
    for request in &spec.requests {
        let cloud = fetch(&client, cloud_base, spec, request)?;
        let selfhosted = fetch(&client, selfhosted_base, spec, request)?;
        let differences = compare(request, &cloud, &selfhosted);
        verdicts.push(Verdict { id: request.id.clone(), passed: differences.is_empty(), differences, cloud, selfhosted });
    }
    let mut measured = Vec::new();
    for request in spec.requests.iter().filter(|request| request.bench && repeat > 0) {
        let mut cloud_times = Vec::new();
        let mut selfhosted_times = Vec::new();
        for _ in 0..repeat {
            cloud_times.push(fetch(&client, cloud_base, spec, request)?.milliseconds);
            selfhosted_times.push(fetch(&client, selfhosted_base, spec, request)?.milliseconds);
        }
        measured.push(RequestLatency { id: request.id.clone(), cloud: latency(cloud_times), selfhosted: latency(selfhosted_times) });
    }
    Ok(Report {
        name: spec.name.clone(),
        passed: verdicts.iter().all(|verdict| verdict.passed),
        verdicts,
        latency: measured,
        allowed_differences: ALLOWED_DIFFERENCES.to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observed(status: u16, headers: &[(&str, &str)], body: &str) -> Observed {
        Observed {
            status,
            headers: headers.iter().map(|(name, value)| ((*name).to_owned(), (*value).to_owned())).collect(),
            body_sha256: hex::encode(Sha256::digest(body.as_bytes())),
            body_bytes: body.len(),
            milliseconds: 1.0,
        }
    }

    fn request(allow: &[(&str, &[&str])]) -> RequestSpec {
        RequestSpec {
            id: "r".into(),
            method: "GET".into(),
            path: "/".into(),
            headers: BTreeMap::new(),
            body: None,
            ignore_headers: Vec::new(),
            bench: false,
            allow_values: allow.iter().map(|(name, values)| ((*name).to_owned(), values.iter().map(|value| (*value).to_owned()).collect())).collect(),
            body_compare: None,
        }
    }

    #[test]
    fn edge_only_headers_do_not_count() {
        let cloud = observed(200, &[("content-type", "a"), ("cf-ray", "x"), ("server", "cloudflare")], "body");
        let local = observed(200, &[("content-type", "a")], "body");
        assert!(compare(&request(&[]), &cloud, &local).is_empty());
    }

    #[test]
    fn a_different_body_or_compared_header_is_reported() {
        let cloud = observed(200, &[("etag", "\"1\"")], "one");
        let local = observed(200, &[("etag", "\"2\"")], "two");
        let found = compare(&request(&[]), &cloud, &local);
        assert_eq!(found.len(), 2);
        assert!(found[0].starts_with("etag"));
        assert!(found[1].starts_with("body"));
    }

    #[test]
    fn an_ignored_header_is_skipped_for_that_request_only() {
        let cloud = observed(206, &[("accept-ranges", "bytes")], "b");
        let local = observed(206, &[], "b");
        let mut with_ignore = request(&[]);
        with_ignore.ignore_headers = vec!["accept-ranges".to_owned()];
        assert!(compare(&with_ignore, &cloud, &local).is_empty());
        assert_eq!(compare(&request(&[]), &cloud, &local).len(), 1);
    }

    #[test]
    fn allowed_values_cover_a_cache_layer_answering_for_the_same_bytes() {
        let cloud = observed(200, &[("x-material-source", "edge-cache")], "b");
        let local = observed(200, &[("x-material-source", "r2")], "b");
        assert!(compare(&request(&[("x-material-source", &["r2", "edge-cache", "built"])]), &cloud, &local).is_empty());
        assert!(!compare(&request(&[("x-material-source", &["built"])]), &cloud, &local).is_empty());
    }
}
