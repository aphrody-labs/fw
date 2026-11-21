//! systemd units, Garage config and the idempotent bootstrap script of one node. Nothing here names a host: hosts come from `cluster/build.toml`.

use crate::compile::{state_directories, Settings, Storage};
use crate::topology::{DualMode, EdgeService, Host};
use crate::wrangler;
use crate::Result;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::path::Path;

pub const GARAGE_UNIT: &str = "fw-edge-garage.service";

pub struct HostPlan {
    /// File name (relative to the output directory) and content.
    pub files: Vec<(String, String)>,
    pub buckets: Vec<String>,
}

pub fn workerd_binary(host: &Host, mode: &DualMode) -> String {
    format!("{}/workerd/node_modules/{}/bin/workerd", host.edge_root(), mode.workerd.package)
}

pub fn garage_config(host: &Host, mode: &DualMode) -> String {
    let root = host.edge_root();
    let garage = &mode.garage;
    format!(
        "metadata_dir = \"{root}/garage/meta\"\ndata_dir = \"{root}/garage/data\"\ndb_engine = \"lmdb\"\nreplication_factor = 1\n\nrpc_bind_addr = \"127.0.0.1:{rpc}\"\nrpc_public_addr = \"127.0.0.1:{rpc}\"\nrpc_secret_file = \"{root}/secrets/garage-rpc.secret\"\n\n[s3_api]\ns3_region = \"{region}\"\napi_bind_addr = \"127.0.0.1:{s3}\"\n\n[admin]\napi_bind_addr = \"127.0.0.1:{admin}\"\nadmin_token_file = \"{root}/secrets/garage-admin.secret\"\n",
        rpc = garage.rpc_port,
        s3 = garage.s3_port,
        admin = garage.admin_port,
        region = garage.region,
    )
}

pub fn garage_unit(host: &Host, mode: &DualMode) -> String {
    let root = host.edge_root();
    format!(
        "[Unit]\nDescription=Garage S3 for the self-hosted Cloudflare mode (separate from the shenron-cdn Garage)\nAfter=network-online.target\nWants=network-online.target\n\n[Service]\nType=simple\nUser={user}\nExecStart={binary} -c {root}/garage/garage.toml server\nRestart=on-failure\nRestartSec=3\nLimitNOFILE=65536\nNoNewPrivileges=true\nProtectSystem=strict\nProtectHome=true\nReadWritePaths={root}\nPrivateTmp=true\nMemoryMax=2G\nCPUWeight=50\n\n[Install]\nWantedBy=multi-user.target\n",
        user = host.user,
        binary = mode.garage.binary,
    )
}

/// Compile settings of one service on one host: everything derives from build.toml and dual-mode.toml.
pub fn settings(host: &Host, mode: &DualMode, service: &EdgeService, project_directory: PathBuf) -> Result<Settings> {
    let root = host.edge_root();
    let mut linked = BTreeMap::new();
    for other in mode.services.iter().filter(|other| other.name != service.name) {
        linked.insert(other.name.clone(), format!("127.0.0.1:{}", other.port));
    }
    Ok(Settings {
        listen_address: format!("127.0.0.1:{}", service.port),
        state_directory: format!("{root}/state/{}", service.name),
        storage: Storage::S3Bridge { bucket: mode.garage.r2_bucket.clone(), endpoint: format!("127.0.0.1:{}", mode.garage.s3_port), region: mode.garage.region.clone() },
        linked_services: linked,
        secrets: service.secrets.clone(),
        project_directory,
    })
}

pub fn workerd_unit(host: &Host, mode: &DualMode, service: &EdgeService, directories: &[String]) -> String {
    let root = host.edge_root();
    let name = &service.name;
    let prepare = format!("ExecStartPre=/usr/bin/mkdir -p {}\n", directories.join(" "));
    format!(
        "[Unit]\nDescription=workerd: {name} (self-hosted Cloudflare mode)\nAfter={garage} network-online.target\nWants={garage}\n\n[Service]\nType=simple\nUser={user}\nEnvironmentFile={root}/secrets/aws.env\nEnvironmentFile=-{root}/secrets/{name}.env\nWorkingDirectory={root}/services/{name}\n{prepare}ExecStart={binary} serve --experimental {root}/services/{name}/{name}.capnp\nRestart=on-failure\nRestartSec=3\nLimitNOFILE=65536\nNoNewPrivileges=true\nProtectSystem=strict\nProtectHome=true\nReadWritePaths={root}/state\nPrivateTmp=true\nMemoryMax=2G\nCPUWeight=50\n\n[Install]\nWantedBy=multi-user.target\n",
        garage = GARAGE_UNIT,
        user = host.user,
        binary = workerd_binary(host, mode),
    )
}

pub fn bootstrap_script(host: &Host, mode: &DualMode) -> String {
    let root = host.edge_root();
    let config = format!("{root}/garage/garage.toml");
    let package = &mode.workerd.package;
    let version = &mode.workerd.version;
    let mut script = String::new();
    script.push_str("#!/usr/bin/env bash\n# Idempotent: safe to run again after a new bucket or service is declared.\nset -euo pipefail\numask 077\n");
    script.push_str(&format!("root={root}\nmkdir -p \"$root\"/{{garage/meta,garage/data,secrets,services,state,workerd}}\n"));
    script.push_str("for secret in garage-rpc garage-admin; do\n  [ -s \"$root/secrets/$secret.secret\" ] || openssl rand -hex 32 | tr -d \x27\\n\x27 > \"$root/secrets/$secret.secret\"\ndone\n");
    script.push_str(&format!(
        "if [ ! -x \"$root/workerd/node_modules/{package}/bin/workerd\" ]; then\n  (cd \"$root/workerd\" && echo '{{}}' > package.json && bun add workerd@{version})\nfi\n"
    ));
    script.push_str("here=$(cd \"$(dirname \"$0\")\" && pwd)\ninstall -m 600 \"$here/garage.toml\" \"$root/garage/garage.toml\"\nfor unit in \"$here\"/*.service; do sudo install -m 644 \"$unit\" /etc/systemd/system/; done\nsudo systemctl daemon-reload\nsudo systemctl enable --now fw-edge-garage.service\n");
    script.push_str(&format!("garage=\"{binary} -c {config}\"\n", binary = mode.garage.binary));
    script.push_str("until $garage status >/dev/null 2>&1; do sleep 1; done\n");
    script.push_str("if ! $garage layout show | grep -qi 'zone'; then\n  node=$($garage node id -q | cut -d@ -f1)\n  $garage layout assign -z local -c 200G \"$node\"\n  $garage layout apply --version 1\nfi\n");
    script.push_str("$garage key info fw-edge >/dev/null 2>&1 || $garage key create fw-edge >/dev/null\n");
    let bucket = &mode.garage.r2_bucket;
    script.push_str(&format!("$garage bucket info {bucket} >/dev/null 2>&1 || $garage bucket create {bucket}\n$garage bucket allow --read --write --owner {bucket} --key fw-edge >/dev/null\n"));
    script.push_str("info=$($garage key info fw-edge --show-secret)\nid=$(printf '%s\\n' \"$info\" | sed -n 's/^Key ID: *//p')\nsecret=$(printf '%s\\n' \"$info\" | sed -n 's/^Secret key: *//p')\n");
    script.push_str("printf 'AWS_ACCESS_KEY_ID=%s\\nAWS_SECRET_ACCESS_KEY=%s\\n' \"$id\" \"$secret\" > \"$root/secrets/aws.env\"\n");
    script
}

pub fn plan(host_name: &str, host: &Host, mode: &DualMode, repository_root: &Path) -> Result<HostPlan> {
    let mut files = vec![
        (format!("{host_name}/garage.toml"), garage_config(host, mode)),
        (format!("{host_name}/{GARAGE_UNIT}"), garage_unit(host, mode)),
    ];
    let mut buckets: Vec<String> = Vec::new();
    for service in mode.services.iter().filter(|service| service.hosts.iter().any(|name| name == host_name)) {
        let wrangler_path = repository_root.join(&service.wrangler);
        let loaded = wrangler::load(&wrangler_path, service.env.as_deref())?;
        for entry in &loaded.r2 {
            if !buckets.contains(&entry.bucket) {
                buckets.push(entry.bucket.clone());
            }
        }
        let directories = state_directories(&loaded, &settings_without_key(host, service));
        files.push((format!("{host_name}/fw-workerd-{}.service", service.name), workerd_unit(host, mode, service, &directories)));
    }
    buckets.sort();
    files.push((format!("{host_name}/bootstrap.sh"), bootstrap_script(host, mode)));
    Ok(HostPlan { files, buckets })
}

/// Only the directory layout matters for a unit; the cluster key does not exist before the bootstrap ran.
fn settings_without_key(host: &Host, service: &EdgeService) -> Settings {
    Settings {
        listen_address: String::new(),
        state_directory: format!("{}/state/{}", host.edge_root(), service.name),
        storage: Storage::LocalDisk,
        linked_services: BTreeMap::new(),
        secrets: Vec::new(),
        project_directory: PathBuf::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topology::{Garage, WorkerdPackage};

    fn fixture() -> (Host, DualMode) {
        let host = Host { ssh: "h".into(), address: "10.0.0.1".into(), user: "svc".into(), work: "/srv/w".into() };
        let mode = DualMode {
            workerd: WorkerdPackage { package: "@cloudflare/workerd-linux-64".into(), version: "1.0.0".into() },
            garage: Garage { binary: "/usr/local/bin/garage".into(), region: "garage".into(), s3_port: 3910, rpc_port: 3912, admin_port: 3913, r2_bucket: "r2".into() },
            services: vec![],
        };
        (host, mode)
    }

    #[test]
    fn the_garage_of_the_edge_mode_has_its_own_ports_and_directories() {
        let (host, mode) = fixture();
        let config = garage_config(&host, &mode);
        assert!(config.contains("api_bind_addr = \"127.0.0.1:3910\""));
        assert!(config.contains("data_dir = \"/srv/w/edge/garage/data\""));
        assert!(config.contains("replication_factor = 1"));
    }

    #[test]
    fn the_worker_unit_reads_secrets_from_files_under_the_edge_root() {
        let (host, mode) = fixture();
        let service = EdgeService { name: "material-cdn".into(), wrangler: "w".into(), env: None, port: 8811, hosts: vec!["h".into()], cloud_url: None, secrets: vec![] };
        let unit = workerd_unit(&host, &mode, &service, &["/srv/w/edge/state/material-cdn/do".to_owned()]);
        assert!(unit.contains("ExecStartPre=/usr/bin/mkdir -p /srv/w/edge/state/material-cdn/do"));
        assert!(unit.contains("EnvironmentFile=-/srv/w/edge/secrets/material-cdn.env"));
        assert!(unit.contains("ExecStart=/srv/w/edge/workerd/node_modules/@cloudflare/workerd-linux-64/bin/workerd serve --experimental /srv/w/edge/services/material-cdn/material-cdn.capnp"));
        assert!(unit.contains("User=svc"));
    }
}
