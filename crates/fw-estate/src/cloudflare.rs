use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::time::Duration;

const BASE: &str = "https://api.cloudflare.com/client/v4";

pub struct Cloudflare {
    agent: ureq::Agent,
    token: String,
    pub account: String,
}

impl Cloudflare {
    /// Reads the token and the account id from the environment variables named in the estate file; values are never printed.
    pub fn from_env(token_env: &str, account_env: &str) -> Result<Self> {
        let token = std::env::var(token_env).with_context(|| format!("environment variable {token_env} is not set"))?;
        let account = std::env::var(account_env).with_context(|| format!("environment variable {account_env} is not set"))?;
        let config = ureq::Agent::config_builder().http_status_as_error(false).timeout_global(Some(Duration::from_secs(60))).build();
        Ok(Self { agent: config.into(), token, account })
    }

    /// One call; returns the whole envelope and fails when `success` is false.
    pub fn call(&self, method: &str, path: &str, body: Option<&Value>) -> Result<Value> {
        let url = format!("{BASE}{path}");
        let authorization = format!("Bearer {}", self.token);
        let mut response = match (method, body) {
            ("GET", _) => self.agent.get(&url).header("Authorization", &authorization).call(),
            ("DELETE", _) => self.agent.delete(&url).header("Authorization", &authorization).call(),
            ("POST", Some(payload)) => self.agent.post(&url).header("Authorization", &authorization).content_type("application/json").send(payload.to_string()),
            ("PUT", Some(payload)) => self.agent.put(&url).header("Authorization", &authorization).content_type("application/json").send(payload.to_string()),
            ("PATCH", Some(payload)) => self.agent.patch(&url).header("Authorization", &authorization).content_type("application/json").send(payload.to_string()),
            _ => bail!("unsupported call {method} {path}"),
        }
        .with_context(|| format!("{method} {path}"))?;
        let status = response.status();
        let text = response.body_mut().read_to_string().with_context(|| format!("read body of {method} {path}"))?;
        let envelope: Value = serde_json::from_str(&text).map_err(|_| anyhow!("{method} {path}: HTTP {status}, body is not JSON"))?;
        if envelope["success"].as_bool() != Some(true) {
            bail!("{method} {path}: HTTP {status}: {}", envelope["errors"]);
        }
        Ok(envelope)
    }

    pub fn get(&self, path: &str) -> Result<Value> {
        Ok(self.call("GET", path, None)?["result"].clone())
    }

    /// GET that tolerates a refused endpoint (a token without the scope, a product not enabled): the reason is returned instead.
    pub fn try_get(&self, path: &str) -> Result<Value, String> {
        self.get(path).map_err(|error| error.to_string())
    }

    pub fn paged(&self, path: &str) -> Result<Vec<Value>> {
        let mut rows = Vec::new();
        for page in 1..=200 {
            let separator = if path.contains('?') { '&' } else { '?' };
            let envelope = self.call("GET", &format!("{path}{separator}per_page=100&page={page}"), None)?;
            rows.extend(envelope["result"].as_array().cloned().unwrap_or_default());
            let total = envelope["result_info"]["total_pages"].as_u64().unwrap_or(1);
            if page >= total {
                break;
            }
        }
        Ok(rows)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ZoneLive {
    pub id: String,
    pub name: String,
    pub status: String,
    pub dnssec: String,
    pub dnssec_ds: Option<String>,
    pub settings: BTreeMap<String, Value>,
    pub records: Vec<Value>,
    pub routes: Vec<Value>,
    pub header_rules: Vec<Value>,
}

#[derive(Debug, Clone, Default)]
pub struct Live {
    pub zones: Vec<ZoneLive>,
    pub worker_domains: Vec<Value>,
    pub workers: Vec<Value>,
    pub worker_settings: BTreeMap<String, Value>,
    pub buckets: Vec<Value>,
    pub bucket_lifecycle: BTreeMap<String, Value>,
    pub tunnels: Vec<Value>,
    pub tunnel_ingress: BTreeMap<String, Vec<Value>>,
    pub access_apps: Vec<Value>,
    pub d1: Vec<Value>,
    pub kv: Vec<Value>,
    pub queues: Vec<Value>,
    pub tokens: Vec<Value>,
    pub requests_14d: BTreeMap<String, (u64, u64)>,
}

impl Live {
    pub fn load(cloudflare: &Cloudflare) -> Result<Self> {
        let account = &cloudflare.account;
        let mut live = Live::default();
        for zone in cloudflare.paged("/zones")? {
            let id = zone["id"].as_str().unwrap_or_default().to_string();
            let settings = cloudflare.get(&format!("/zones/{id}/settings"))?;
            let dnssec = cloudflare.get(&format!("/zones/{id}/dnssec")).unwrap_or(Value::Null);
            live.zones.push(ZoneLive {
                name: zone["name"].as_str().unwrap_or_default().to_string(),
                status: zone["status"].as_str().unwrap_or_default().to_string(),
                dnssec: dnssec["status"].as_str().unwrap_or("unknown").to_string(),
                dnssec_ds: dnssec["ds"].as_str().map(str::to_string),
                settings: settings.as_array().into_iter().flatten().filter_map(|entry| Some((entry["id"].as_str()?.to_string(), entry["value"].clone()))).collect(),
                records: cloudflare.paged(&format!("/zones/{id}/dns_records"))?,
                routes: cloudflare.paged(&format!("/zones/{id}/workers/routes"))?,
                header_rules: cloudflare.try_get(&format!("/zones/{id}/rulesets/phases/http_response_headers_transform/entrypoint")).map(|ruleset| ruleset["rules"].as_array().cloned().unwrap_or_default()).unwrap_or_default(),
                id,
            });
        }
        live.worker_domains = cloudflare.paged(&format!("/accounts/{account}/workers/domains"))?;
        live.workers = cloudflare.paged(&format!("/accounts/{account}/workers/scripts"))?;
        for worker in &live.workers {
            let name = worker["id"].as_str().unwrap_or_default();
            let mut settings = cloudflare.get(&format!("/accounts/{account}/workers/scripts/{name}/settings"))?;
            settings["schedules"] = cloudflare.get(&format!("/accounts/{account}/workers/scripts/{name}/schedules")).map(|result| result["schedules"].clone()).unwrap_or(Value::Null);
            live.worker_settings.insert(name.to_string(), settings);
        }
        live.buckets = cloudflare.get(&format!("/accounts/{account}/r2/buckets"))?["buckets"].as_array().cloned().unwrap_or_default();
        for bucket in &mut live.buckets {
            let name = bucket["name"].as_str().unwrap_or_default().to_string();
            if let Ok(detail) = cloudflare.get(&format!("/accounts/{account}/r2/buckets/{name}")) {
                bucket["location"] = detail["location"].clone();
            }
            let lifecycle = cloudflare.get(&format!("/accounts/{account}/r2/buckets/{name}/lifecycle")).unwrap_or(Value::Null);
            live.bucket_lifecycle.insert(name, lifecycle);
        }
        live.tunnels = cloudflare.paged(&format!("/accounts/{account}/cfd_tunnel?is_deleted=false"))?;
        for tunnel in &live.tunnels {
            let id = tunnel["id"].as_str().unwrap_or_default();
            let configuration = cloudflare.get(&format!("/accounts/{account}/cfd_tunnel/{id}/configurations")).unwrap_or(Value::Null);
            live.tunnel_ingress.insert(id.to_string(), configuration["config"]["ingress"].as_array().cloned().unwrap_or_default());
        }
        live.access_apps = cloudflare.paged(&format!("/accounts/{account}/access/apps"))?;
        live.d1 = cloudflare.paged(&format!("/accounts/{account}/d1/database"))?;
        live.kv = cloudflare.paged(&format!("/accounts/{account}/storage/kv/namespaces"))?;
        live.queues = cloudflare.paged(&format!("/accounts/{account}/queues"))?;
        live.tokens = cloudflare.paged(&format!("/accounts/{account}/tokens")).unwrap_or_default();
        live.requests_14d = requests_14d(cloudflare).unwrap_or_default();
        Ok(live)
    }

    pub fn load_zone_id(cloudflare: &Cloudflare, name: &str) -> Result<String> {
        let zones = cloudflare.get(&format!("/zones?name={name}"))?;
        zones[0]["id"].as_str().map(str::to_string).ok_or_else(|| anyhow!("zone {name} not found"))
    }

    pub fn tunnel_name(&self, id: &str) -> Option<String> {
        self.tunnels.iter().find(|tunnel| tunnel["id"].as_str() == Some(id)).and_then(|tunnel| tunnel["name"].as_str().map(str::to_string))
    }

    pub fn tunnel_id(&self, name: &str) -> Option<String> {
        self.tunnels.iter().find(|tunnel| tunnel["name"].as_str() == Some(name)).and_then(|tunnel| tunnel["id"].as_str().map(str::to_string))
    }
}

/// Requests and errors per Worker over the last 14 days, from the GraphQL analytics API.
fn requests_14d(cloudflare: &Cloudflare) -> Result<BTreeMap<String, (u64, u64)>> {
    let query = "query($account:String!,$since:Date!,$until:Date!){viewer{accounts(filter:{accountTag:$account}){workersInvocationsAdaptive(limit:1000,filter:{date_geq:$since,date_leq:$until}){sum{requests errors} dimensions{scriptName}}}}}";
    let (since, until) = date_window(14);
    let payload = json!({ "query": query, "variables": { "account": cloudflare.account, "since": since, "until": until } });
    let envelope = cloudflare.post_graphql(&payload)?;
    let mut totals = BTreeMap::new();
    for row in envelope["data"]["viewer"]["accounts"][0]["workersInvocationsAdaptive"].as_array().into_iter().flatten() {
        let name = row["dimensions"]["scriptName"].as_str().unwrap_or_default().to_string();
        let entry: &mut (u64, u64) = totals.entry(name).or_default();
        entry.0 += row["sum"]["requests"].as_u64().unwrap_or(0);
        entry.1 += row["sum"]["errors"].as_u64().unwrap_or(0);
    }
    Ok(totals)
}

fn date_window(days: u64) -> (String, String) {
    let seconds = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|elapsed| elapsed.as_secs()).unwrap_or(0);
    (civil_date(seconds.saturating_sub(days * 86_400)), civil_date(seconds))
}

/// Days since 1970-01-01 to a calendar date (Howard Hinnant's algorithm).
pub fn civil_date(epoch_seconds: u64) -> String {
    let days = (epoch_seconds / 86_400) as i64 + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_part = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_part + 2) / 5 + 1;
    let month = if month_part < 10 { month_part + 3 } else { month_part - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

impl Cloudflare {
    pub fn post_graphql(&self, payload: &Value) -> Result<Value> {
        let mut response = self
            .agent
            .post("https://api.cloudflare.com/client/v4/graphql")
            .header("Authorization", &format!("Bearer {}", self.token))
            .content_type("application/json")
            .send(payload.to_string())
            .context("POST graphql")?;
        let text = response.body_mut().read_to_string().context("read graphql body")?;
        let envelope: Value = serde_json::from_str(&text).context("graphql body is not JSON")?;
        if !envelope["errors"].is_null() {
            bail!("graphql: {}", envelope["errors"]);
        }
        Ok(envelope)
    }
}

#[cfg(test)]
mod tests {
    use super::civil_date;

    #[test]
    fn converts_epoch_to_calendar_date() {
        assert_eq!(civil_date(0), "1970-01-01");
        assert_eq!(civil_date(1_763_683_200), "2025-11-21");
    }
}
