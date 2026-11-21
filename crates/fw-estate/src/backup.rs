//! Backups of Cloudflare data onto our own disks: D1 exports through the API, R2 mirrored with rclone.
use crate::cloudflare::Cloudflare;
use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Backup {
    /// Key of a `[hosts.*]` table in the cluster file: the host that holds the copies.
    pub host: String,
    /// Directory under that host's `work` path.
    pub subdir: String,
    /// rclone remote that points at the R2 account.
    pub rclone_remote: String,
    /// Buckets that are caches, not data.
    #[serde(default)]
    pub skip_buckets: Vec<String>,
    pub schedule: String,
}

/// The `[hosts.*]` table whose key or one of whose `aliases` is `name`.
pub fn find_host<'a>(cluster: &'a toml::Value, name: &str) -> Option<&'a toml::Value> {
    cluster.get("hosts")?.as_table()?.iter().find(|(key, host)| key.as_str() == name || host.get("aliases").and_then(toml::Value::as_array).is_some_and(|aliases| aliases.iter().any(|alias| alias.as_str() == Some(name)))).map(|(_, host)| host)
}

/// `<work of the backup host>/<subdir>`, read from the cluster file.
pub fn root(backup: &Backup, cluster_text: &str) -> Result<PathBuf> {
    let cluster: toml::Value = toml::from_str(cluster_text).context("parse the cluster file")?;
    let work = find_host(&cluster, &backup.host).and_then(|host| host.get("work")).and_then(toml::Value::as_str).ok_or_else(|| anyhow!("cluster file has no host {} with a work path", backup.host))?;
    Ok(Path::new(work).join(&backup.subdir))
}

/// `hosts.<host>.user` of the cluster file: the account that owns the rclone configuration.
pub fn host_user(backup: &Backup, cluster_text: &str) -> Result<String> {
    let cluster: toml::Value = toml::from_str(cluster_text).context("parse the cluster file")?;
    find_host(&cluster, &backup.host).and_then(|host| host.get("user")).and_then(toml::Value::as_str).map(str::to_string).ok_or_else(|| anyhow!("cluster file has no host {} with a user", backup.host))
}

pub fn export_d1(cloudflare: &Cloudflare, id: &str, name: &str, destination: &Path) -> Result<u64> {
    let path = format!("/accounts/{}/d1/database/{id}/export", cloudflare.account);
    let mut bookmark: Option<String> = None;
    for _ in 0..60 {
        let mut body = json!({ "output_format": "polling" });
        if let Some(current) = &bookmark {
            body["current_bookmark"] = json!(current);
        }
        let result = cloudflare.call("POST", &path, Some(&body))?["result"].clone();
        if let Some(url) = result["result"]["signed_url"].as_str() {
            let mut response = ureq::get(url).call().with_context(|| format!("download the export of {name}"))?;
            let bytes = response.body_mut().read_to_vec().with_context(|| format!("read the export of {name}"))?;
            std::fs::create_dir_all(destination.parent().ok_or_else(|| anyhow!("destination has no parent"))?)?;
            std::fs::write(destination, &bytes).with_context(|| format!("write {}", destination.display()))?;
            return Ok(bytes.len() as u64);
        }
        bookmark = result["at_bookmark"].as_str().map(str::to_string);
        std::thread::sleep(Duration::from_secs(2));
    }
    bail!("the export of D1 {name} did not finish in 120 seconds")
}

/// One rclone sync per bucket; objects deleted or replaced in R2 move to a dated trash directory instead of disappearing from the copy.
pub fn mirror_bucket(remote: &str, bucket: &str, root: &Path, stamp: &str) -> Result<()> {
    let destination = root.join("r2").join(bucket);
    let trash = root.join("r2-trash").join(stamp).join(bucket);
    let status = Command::new("rclone")
        .arg("sync")
        .arg(format!("{remote}:{bucket}"))
        .arg(&destination)
        .args(["--fast-list", "--transfers", "16", "--backup-dir"])
        .arg(&trash)
        .status()
        .context("run rclone")?;
    if !status.success() {
        bail!("rclone sync of {bucket} exited with {status}");
    }
    Ok(())
}

pub fn units(backup: &Backup, user: &str, binary: &str, estate: &str) -> (String, String) {
    let service = format!("[Unit]\nDescription=Back up Cloudflare D1 and R2 to local disks\nAfter=network-online.target\nWants=network-online.target\n\n[Service]\nType=oneshot\nUser={}\nEnvironmentFile=/etc/aphrody/cloudflare-backup.env\nExecStart={binary} backup --estate {estate}\nNice=19\nIOSchedulingClass=idle\n", user);
    let timer = format!("[Unit]\nDescription=Daily Cloudflare backup\n\n[Timer]\nOnCalendar={}\nPersistent=true\nRandomizedDelaySec=15min\n\n[Install]\nWantedBy=timers.target\n", backup.schedule);
    (service, timer)
}

pub fn stamp() -> String {
    let seconds = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|elapsed| elapsed.as_secs()).unwrap_or(0);
    format!("{}T{:02}{:02}Z", crate::cloudflare::civil_date(seconds), (seconds % 86_400) / 3600, (seconds % 3600) / 60)
}

pub fn d1_names(live: &[Value]) -> Vec<(String, String)> {
    live.iter().filter_map(|row| Some((row["uuid"].as_str()?.to_string(), row["name"].as_str()?.to_string()))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_comes_from_the_cluster_file() {
        let backup = Backup { host: "fr1".into(), subdir: "backup/cloudflare".into(), rclone_remote: "r2".into(), skip_buckets: vec![], schedule: "daily".into() };
        let root = root(&backup, "[hosts.n1]\naliases = [\"fr1\"]\nwork = \"/srv/xbuild\"\n").expect("root");
        assert_eq!(root, Path::new("/srv/xbuild").join("backup/cloudflare"));
        assert!(super::root(&backup, "[hosts.prd]\nwork = \"/x\"\n").is_err());
    }

    #[test]
    fn units_carry_the_schedule() {
        let backup = Backup { host: "fr1".into(), subdir: "b".into(), rclone_remote: "r2".into(), skip_buckets: vec![], schedule: "*-*-* 03:30:00 UTC".into() };
        let (service, timer) = units(&backup, "ubuntu", "/usr/local/bin/fw-estate", "/etc/aphrody/estate.toml");
        assert!(service.contains("ExecStart=/usr/local/bin/fw-estate backup --estate /etc/aphrody/estate.toml"));
        assert!(timer.contains("OnCalendar=*-*-* 03:30:00 UTC"));
    }
}
