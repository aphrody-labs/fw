use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Estate {
    pub estate: Meta,
    #[serde(default)]
    pub practice: BTreeMap<String, Practice>,
    #[serde(default)]
    pub zone_defaults: BTreeMap<String, toml::Value>,
    #[serde(default)]
    pub zone: BTreeMap<String, Zone>,
    #[serde(default)]
    pub worker: BTreeMap<String, Worker>,
    #[serde(default)]
    pub bucket: BTreeMap<String, Bucket>,
    #[serde(default)]
    pub service: BTreeMap<String, Service>,
    #[serde(default)]
    pub retired: Retired,
    #[serde(default)]
    pub backup: Option<crate::backup::Backup>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Meta {
    pub schema: u32,
    pub account_env: String,
    pub token_env: String,
    pub cluster_file: String,
    pub plan_file: String,
    pub compat_max_age_days: u32,
    #[serde(default)]
    pub hostinger_token_env: Option<String>,
    #[serde(default)]
    pub ovh_config_env: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Practice {
    pub rule: String,
    pub source: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Zone {
    #[serde(default)]
    pub dnssec: bool,
    #[serde(default)]
    pub settings: BTreeMap<String, toml::Value>,
    #[serde(default)]
    pub record: Vec<String>,
    #[serde(default)]
    pub route: Vec<String>,
    #[serde(default)]
    pub domain: Vec<String>,
    #[serde(default)]
    pub header_rule: Vec<HeaderRule>,
}

/// A response-header transform rule: `set` headers on the responses whose request matches `expression`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeaderRule {
    pub description: String,
    pub expression: String,
    pub set: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Worker {
    pub source: String,
    #[serde(default)]
    pub sampling: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Bucket {
    pub tier: String,
    pub owner: String,
    /// R2 location (WEUR, EEUR, ...): sent as the location hint on creation, compared on every check.
    pub location: String,
    #[serde(default)]
    pub abort_multipart_days: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Service {
    pub worker: String,
    #[serde(default)]
    pub hostnames: Vec<String>,
    #[serde(default)]
    pub bindings: Vec<String>,
    pub modes: Vec<String>,
    #[serde(default)]
    pub selfhosted_hosts: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Retired {
    #[serde(default)]
    pub worker: BTreeMap<String, String>,
}

impl Estate {
    pub fn load(path: &Path) -> Result<(Self, String)> {
        let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        let estate: Estate = toml::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
        if estate.estate.schema != 1 {
            bail!("estate schema {} is not supported (expected 1)", estate.estate.schema);
        }
        Ok((estate, text))
    }

    pub fn zone_settings(&self, zone: &str) -> BTreeMap<String, toml::Value> {
        let mut merged = self.zone_defaults.clone();
        if let Some(declared) = self.zone.get(zone) {
            merged.extend(declared.settings.clone());
        }
        merged
    }
}

/// A DNS record as declared (`TYPE NAME [flag...] | CONTENT`) or as read back from the API.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Record {
    pub kind: String,
    pub name: String,
    pub content: String,
    pub proxied: bool,
    pub ttl: u32,
    pub priority: Option<u16>,
}

impl Record {
    pub fn parse(zone: &str, line: &str) -> Result<Self> {
        let (head, content) = line.split_once('|').ok_or_else(|| anyhow!("record `{line}` has no `|` before the content"))?;
        let mut words = head.split_whitespace();
        let kind = words.next().ok_or_else(|| anyhow!("record `{line}` has no type"))?.to_ascii_uppercase();
        let relative = words.next().ok_or_else(|| anyhow!("record `{line}` has no name"))?;
        let mut record = Record { kind, name: fqdn(zone, relative), content: content.trim().to_string(), proxied: false, ttl: 1, priority: None };
        for flag in words {
            match flag.split_once('=') {
                None if flag == "proxied" => record.proxied = true,
                Some(("prio", value)) => record.priority = Some(value.parse().with_context(|| format!("prio in `{line}`"))?),
                Some(("ttl", value)) => record.ttl = value.parse().with_context(|| format!("ttl in `{line}`"))?,
                _ => bail!("unknown flag `{flag}` in record `{line}`"),
            }
        }
        Ok(record)
    }

    pub fn to_line(&self, zone: &str) -> String {
        let mut head = format!("{} {}", self.kind, relative(zone, &self.name));
        if self.proxied {
            head.push_str(" proxied");
        }
        if let Some(priority) = self.priority {
            head.push_str(&format!(" prio={priority}"));
        }
        if self.ttl != 1 {
            head.push_str(&format!(" ttl={}", self.ttl));
        }
        format!("{head} | {}", self.content)
    }

    /// Same identity as far as the API is concerned: one value of a multi-valued set, or the single value of a name.
    pub fn identity(&self) -> (String, String, String, Option<u16>) {
        (self.kind.clone(), self.name.clone(), normalize_content(&self.kind, &self.content), self.priority)
    }

    pub fn single_valued(&self) -> bool {
        matches!(self.kind.as_str(), "A" | "AAAA" | "CNAME")
    }
}

pub fn fqdn(zone: &str, relative: &str) -> String {
    match relative {
        "@" => zone.to_string(),
        name if name == zone || name.ends_with(&format!(".{zone}")) => name.to_string(),
        name => format!("{name}.{zone}"),
    }
}

pub fn relative(zone: &str, name: &str) -> String {
    if name == zone {
        "@".to_string()
    } else {
        name.strip_suffix(&format!(".{zone}")).unwrap_or(name).to_string()
    }
}

/// TXT values come back quoted and split in 255-byte chunks; hostnames come back with or without a trailing dot.
pub fn normalize_content(kind: &str, content: &str) -> String {
    let trimmed = content.trim();
    match kind {
        "TXT" => trimmed.replace("\" \"", "").trim_matches('"').to_string(),
        "CNAME" | "MX" | "NS" => trimmed.trim_end_matches('.').to_ascii_lowercase(),
        "SRV" => trimmed.trim_end_matches('.').to_string(),
        _ => trimmed.to_string(),
    }
}

/// `host/path => script` of a Worker route.
pub fn parse_route(line: &str) -> Result<(String, String)> {
    let (pattern, script) = line.split_once("=>").ok_or_else(|| anyhow!("route `{line}` has no `=>`"))?;
    Ok((pattern.trim().to_string(), script.trim().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_prints_records() {
        let record = Record::parse("aphrody.com", "MX @ prio=10 | mx1.mail.ovh.net").expect("parse");
        assert_eq!(record.name, "aphrody.com");
        assert_eq!(record.priority, Some(10));
        assert_eq!(record.to_line("aphrody.com"), "MX @ prio=10 | mx1.mail.ovh.net");
        let proxied = Record::parse("aphrody.com", "CNAME admin proxied | tunnel:aphrody").expect("parse");
        assert!(proxied.proxied);
        assert_eq!(proxied.name, "admin.aphrody.com");
    }

    #[test]
    fn rejects_unknown_flags_and_missing_content() {
        assert!(Record::parse("a.com", "A x ttl=1 bogus | 1.2.3.4").is_err());
        assert!(Record::parse("a.com", "A x").is_err());
    }

    #[test]
    fn normalizes_txt_chunks_and_trailing_dots() {
        assert_eq!(normalize_content("TXT", "\"v=DKIM1; p=abc\" \"def\""), "v=DKIM1; p=abcdef");
        assert_eq!(normalize_content("CNAME", "Target.Example.com."), "target.example.com");
    }

    #[test]
    fn splits_routes() {
        assert_eq!(parse_route("a.com/* => w").expect("route"), ("a.com/*".to_string(), "w".to_string()));
    }
}
