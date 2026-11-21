//! Read-only facts from OVH and Hostinger, plus the one OVH write the estate needs: publishing a DS record.
use crate::check::Finding;
use crate::cloudflare::Live;
use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::time::Duration;

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder().http_status_as_error(false).timeout_global(Some(Duration::from_secs(60))).build().into()
}

pub fn sha1_hex(data: &[u8]) -> String {
    let mut state: [u32; 5] = [0x6745_2301, 0xEFCD_AB89, 0x98BA_DCFE, 0x1032_5476, 0xC3D2_E1F0];
    let mut message = data.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for block in message.chunks(64) {
        let mut words = [0u32; 80];
        for (index, chunk) in block.chunks(4).enumerate() {
            words[index] = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
        for index in 16..80 {
            words[index] = (words[index - 3] ^ words[index - 8] ^ words[index - 14] ^ words[index - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = state;
        for (index, word) in words.iter().enumerate() {
            let (mix, constant) = match index {
                0..=19 => ((b & c) | (!b & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let next = a.rotate_left(5).wrapping_add(mix).wrapping_add(e).wrapping_add(constant).wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = next;
        }
        for (slot, value) in state.iter_mut().zip([a, b, c, d, e]) {
            *slot = slot.wrapping_add(value);
        }
    }
    state.iter().map(|word| format!("{word:08x}")).collect()
}

pub struct Ovh {
    agent: ureq::Agent,
    base: String,
    application_key: String,
    application_secret: String,
    consumer_key: String,
    time_delta: i64,
}

impl Ovh {
    /// Reads `~/.ovh.conf` (or the file named by the environment variable): `[default] endpoint` picks the section holding the three keys.
    pub fn from_config(config_env: &str) -> Result<Self> {
        let path = match std::env::var(config_env) {
            Ok(path) => path,
            Err(_) => {
                let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).context("no home directory for .ovh.conf")?;
                format!("{home}/.ovh.conf")
            }
        };
        let text = std::fs::read_to_string(&path).with_context(|| format!("read {path}"))?;
        let mut sections: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        let mut current = String::new();
        for line in text.lines() {
            let line = line.trim();
            if let Some(name) = line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
                current = name.to_string();
            } else if let Some((key, value)) = line.split_once('=') {
                sections.entry(current.clone()).or_default().insert(key.trim().to_string(), value.trim().to_string());
            }
        }
        let endpoint = sections.get("default").and_then(|section| section.get("endpoint")).cloned().unwrap_or_else(|| "ovh-eu".to_string());
        let keys = sections.get(&endpoint).ok_or_else(|| anyhow!("no [{endpoint}] section in the OVH configuration"))?;
        let get = |name: &str| keys.get(name).cloned().ok_or_else(|| anyhow!("OVH configuration lacks {name}"));
        let base = match endpoint.as_str() {
            "ovh-eu" => "https://eu.api.ovh.com/1.0",
            "ovh-ca" => "https://ca.api.ovh.com/1.0",
            "ovh-us" => "https://api.us.ovhcloud.com/1.0",
            other => bail!("unknown OVH endpoint {other}"),
        };
        let mut ovh = Self { agent: agent(), base: base.to_string(), application_key: get("application_key")?, application_secret: get("application_secret")?, consumer_key: get("consumer_key")?, time_delta: 0 };
        let server_time: i64 = ovh.agent.get(&format!("{base}/auth/time")).call()?.body_mut().read_to_string()?.trim().parse().context("OVH /auth/time")?;
        ovh.time_delta = server_time - now_seconds();
        Ok(ovh)
    }

    pub fn call(&self, method: &str, path: &str, body: Option<&Value>) -> Result<Value> {
        let url = format!("{}{path}", self.base);
        let payload = body.map(Value::to_string).unwrap_or_default();
        let timestamp = now_seconds() + self.time_delta;
        let signature = format!("$1${}", sha1_hex(format!("{}+{}+{method}+{url}+{payload}+{timestamp}", self.application_secret, self.consumer_key).as_bytes()));
        let mut response = match method {
            "GET" => self.agent.get(&url).header("X-Ovh-Application", &self.application_key).header("X-Ovh-Consumer", &self.consumer_key).header("X-Ovh-Timestamp", &timestamp.to_string()).header("X-Ovh-Signature", &signature).call(),
            "POST" => self.agent.post(&url).header("X-Ovh-Application", &self.application_key).header("X-Ovh-Consumer", &self.consumer_key).header("X-Ovh-Timestamp", &timestamp.to_string()).header("X-Ovh-Signature", &signature).content_type("application/json").send(payload),
            other => bail!("OVH method {other} is not supported"),
        }
        .with_context(|| format!("OVH {method} {path}"))?;
        let status = response.status();
        let text = response.body_mut().read_to_string()?;
        let value: Value = if text.is_empty() { Value::Null } else { serde_json::from_str(&text).unwrap_or(Value::String(text)) };
        if !status.is_success() {
            bail!("OVH {method} {path}: HTTP {status}: {value}");
        }
        Ok(value)
    }

    /// Publishes the DS record Cloudflare generated for `zone` at the registrar, which finishes DNSSEC.
    pub fn publish_ds(&self, zone: &str, key_tag: u64, algorithm: u64, flags: u64, public_key: &str) -> Result<()> {
        let keys = json!({ "keys": [{ "tag": key_tag, "algorithm": algorithm, "flags": flags, "publicKey": public_key }] });
        self.call("POST", &format!("/domain/{zone}/dsRecord"), Some(&keys))?;
        Ok(())
    }
}

fn now_seconds() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|elapsed| elapsed.as_secs() as i64).unwrap_or(0)
}

/// Facts that tie the providers to the Cloudflare zones: registrar DNSSEC against Cloudflare DNSSEC, renewals, suspended services.
pub fn findings(live: &Live, ovh: Option<&Ovh>, hostinger_token: Option<&str>) -> Vec<Finding> {
    let mut found = Vec::new();
    let mut push = |id: &'static str, error: bool, subject: String, message: String| found.push(Finding { id, error, subject, message });
    match ovh {
        None => push("provider.ovh", true, "ovh".into(), "no OVH credentials found (~/.ovh.conf): registrar state cannot be read".into()),
        Some(ovh) => match ovh.call("GET", "/domain", None) {
            Err(error) => push("provider.ovh", true, "ovh".into(), error.to_string()),
            Ok(domains) => {
                for domain in domains.as_array().into_iter().flatten().filter_map(Value::as_str) {
                    let Ok(info) = ovh.call("GET", &format!("/domain/{domain}"), None) else { continue };
                    let cloudflare = live.zones.iter().find(|zone| zone.name == domain);
                    let registrar_dnssec = info["dnssecState"].as_str().unwrap_or("unknown");
                    match cloudflare {
                        Some(zone) if zone.dnssec != "active" && registrar_dnssec != "enabled" => push("registrar.ds-missing", true, domain.into(), format!("Cloudflare DNSSEC {}, OVH registrar DNSSEC {registrar_dnssec}: publish the DS record at OVH", zone.dnssec)),
                        None => push("registrar.no-zone", false, domain.into(), "OVH domain without a Cloudflare zone".into()),
                        _ => {}
                    }
                    if info["state"].as_str() != Some("ok") || info["suspensionState"].as_str() != Some("not_suspended") {
                        push("registrar.state", true, domain.into(), format!("state {} / {}", info["state"], info["suspensionState"]));
                    }
                }
            }
        },
    }
    match hostinger_token {
        None => push("provider.hostinger", true, "hostinger".into(), "no Hostinger token in the environment".into()),
        Some(token) => {
            let get = |path: &str| -> Result<Value> {
                let mut response = agent().get(&format!("https://developers.hostinger.com{path}")).header("Authorization", &format!("Bearer {token}")).call()?;
                Ok(serde_json::from_str(&response.body_mut().read_to_string()?)?)
            };
            match get("/api/domains/v1/portfolio") {
                Err(error) => push("provider.hostinger", true, "hostinger".into(), error.to_string()),
                Ok(domains) => {
                    for domain in domains.as_array().into_iter().flatten() {
                        if domain["status"].as_str() != Some("active") {
                            push("registrar.state", true, domain["domain"].as_str().unwrap_or("?").into(), format!("Hostinger domain status {}", domain["status"]));
                        }
                    }
                }
            }
            if let Ok(machines) = get("/api/vps/v1/virtual-machines") {
                for machine in machines.as_array().into_iter().flatten() {
                    if machine["firewall_group_id"].is_null() {
                        push("vps.firewall", false, machine["hostname"].as_str().unwrap_or("?").into(), "no Hostinger firewall group is attached to the VPS".into());
                    }
                }
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::sha1_hex;

    #[test]
    fn sha1_matches_known_vectors() {
        assert_eq!(sha1_hex(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(sha1_hex(b""), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert_eq!(sha1_hex(b"The quick brown fox jumps over the lazy dog"), "2fd4e1c67a2d28fced849ee1bb76e7391b93eb12");
    }
}
