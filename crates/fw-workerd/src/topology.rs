//! Hosts come from `cluster/build.toml` (ssh alias, user, work directory); what runs on them comes from `cluster/dual-mode.toml`.

use crate::{read_text, Error, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct Host {
    /// The key of `[hosts.<name>]`, filled in by `load_hosts`.
    #[serde(default)]
    pub name: String,
    /// Older names of the node (the build file keeps them while hosts are renamed).
    #[serde(default)]
    pub aliases: Vec<String>,
    pub ssh: String,
    pub address: String,
    pub user: String,
    pub work: String,
}

#[derive(Debug, Deserialize)]
struct BuildFile {
    hosts: BTreeMap<String, Host>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkerdPackage {
    pub package: String,
    pub version: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Garage {
    pub binary: String,
    pub region: String,
    pub s3_port: u16,
    pub rpc_port: u16,
    pub admin_port: u16,
    /// The one Garage bucket that holds the objects of every R2 bucket, as `<r2 bucket>/<key>`; services that bind the same R2 bucket share them.
    pub r2_bucket: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EdgeService {
    pub name: String,
    pub wrangler: String,
    #[serde(default)]
    pub env: Option<String>,
    pub port: u16,
    pub hosts: Vec<String>,
    #[serde(default)]
    pub cloud_url: Option<String>,
    #[serde(default)]
    pub secrets: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DualMode {
    pub workerd: WorkerdPackage,
    pub garage: Garage,
    #[serde(rename = "service")]
    pub services: Vec<EdgeService>,
}

pub fn load_hosts(build_toml: &Path) -> Result<BTreeMap<String, Host>> {
    let text = read_text(build_toml)?;
    let file: BuildFile = toml::from_str(&text).map_err(|error| Error::Parse { path: build_toml.display().to_string(), message: error.to_string() })?;
    let mut hosts = file.hosts;
    for (name, host) in &mut hosts {
        host.name.clone_from(name);
    }
    Ok(hosts)
}

/// A host by its name or one of its aliases.
pub fn find_host<'a>(hosts: &'a BTreeMap<String, Host>, name: &str) -> Result<&'a Host> {
    hosts.values().find(|host| host.name == name || host.aliases.iter().any(|alias| alias == name)).ok_or_else(|| Error::Invalid(format!("no host `{name}` in cluster/build.toml")))
}

pub fn load_dual_mode(path: &Path) -> Result<DualMode> {
    let text = read_text(path)?;
    toml::from_str(&text).map_err(|error| Error::Parse { path: path.display().to_string(), message: error.to_string() })
}

impl Host {
    pub fn serves(&self, service: &EdgeService) -> bool {
        service.hosts.iter().any(|name| *name == self.name || self.aliases.contains(name))
    }

    /// Everything the self-hosted mode writes lives under one directory of the host's work area.
    pub fn edge_root(&self) -> String {
        format!("{}/edge", self.work.trim_end_matches('/'))
    }
}

impl DualMode {
    pub fn service(&self, name: &str) -> Result<&EdgeService> {
        self.services.iter().find(|service| service.name == name).ok_or_else(|| Error::Invalid(format!("no service `{name}` in dual-mode.toml")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_and_services_parse() {
        let hosts: BuildFile = toml::from_str("[hosts.fr1]\naliases=[\"old\"]\nssh=\"fr1\"\naddress=\"10.0.0.1\"\nuser=\"u\"\nwork=\"/srv/x/\"\n").expect("hosts");
        assert_eq!(hosts.hosts["fr1"].edge_root(), "/srv/x/edge");
        assert_eq!(hosts.hosts["fr1"].aliases, vec!["old".to_owned()]);
        let mode: DualMode = toml::from_str(
            "[workerd]\npackage=\"p\"\nversion=\"1\"\n[garage]\nbinary=\"/b\"\nregion=\"garage\"\ns3_port=1\nrpc_port=2\nadmin_port=3\nr2_bucket=\"s\"\n[[service]]\nname=\"a\"\nwrangler=\"w\"\nport=4\nhosts=[\"fr1\"]\ncloud_url=\"https://a\"\n",
        )
        .expect("mode");
        assert_eq!(mode.service("a").expect("service").port, 4);
        assert!(mode.service("b").is_err());
    }
}
