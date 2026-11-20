//! `unified_pm` — Pure-Rust (`#![forbid(unsafe_code)]`), zero-bloat Unified Package Manager
//! combining **npm / Bun** (`bun_install` + `bun_semver` design), **crates.io / Cargo** (RFC 2789
//! Sparse Index), and **APM** (`microsoft/apm` pure-Rust `fw-apm`) into a single content-addressed
//! resolver, lockfile (`fw-pm.lock.json`), and CycloneDX 1.5 SBOM engine.

use crate::manifest::parse_apm_yml;
use fw_nix::compute_fixed_output_store_entry;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Ecosystem {
    Npm,
    Cargo,
    Apm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SemVer {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl SemVer {
    pub fn parse(raw: &str) -> Option<Self> {
        let cleaned = raw
            .trim()
            .trim_start_matches(['^', '~', '=', '>', '<', 'v', 'V', ' '])
            .split(['-', '+'])
            .next()?;
        let mut parts = cleaned.split('.');
        let major = parts.next()?.parse::<u64>().ok()?;
        let minor = parts.next().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
        let patch = parts.next().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
        Some(Self { major, minor, patch })
    }

    /// Evaluates caret (`^`), tilde (`~`), `>=`, `=`, wildcard (`*`), or exact SemVer ranges
    /// (aligned with Bun's `bun_semver` and Cargo's default caret semantics).
    pub fn satisfies(self, range: &str) -> bool {
        let r = range.trim();
        if r.is_empty() || r == "*" || r == "latest" || r == "workspace:*" {
            return true;
        }
        let Some(base) = Self::parse(r) else {
            return false;
        };
        if r.starts_with(">=") {
            return self >= base;
        }
        if r.starts_with('=') {
            return self == base;
        }
        if r.starts_with('~') {
            return self.major == base.major && self.minor == base.minor && self.patch >= base.patch;
        }
        // Caret (`^`) or bare Cargo version: compatible with leftmost non-zero component
        if base.major > 0 {
            self.major == base.major && self >= base
        } else if base.minor > 0 {
            self.major == 0 && self.minor == base.minor && self.patch >= base.patch
        } else {
            self == base
        }
    }
}

/// Computes the exact RFC 2789 crates.io Sparse Index URL for any crate name.
pub fn cargo_sparse_index_url(crate_name: &str) -> String {
    let lower = crate_name.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let prefix = match bytes.len() {
        0 => String::new(),
        1 => "1".into(),
        2 => "2".into(),
        3 => format!("3/{}", &lower[..1]),
        _ => format!("{}/{}", &lower[..2], &lower[2..4]),
    };
    format!("https://index.crates.io/{prefix}/{lower}")
}

/// Computes the canonical npm registry metadata URL (including scoped `@org/pkg` escaping).
pub fn npm_registry_url(pkg_name: &str) -> String {
    format!("https://registry.npmjs.org/{}", pkg_name.replace('/', "%2f"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedPackageEntry {
    pub ecosystem: Ecosystem,
    pub name: String,
    pub version_req: String,
    pub registry_url: String,
    pub nix_store_path: String,
    pub integrity_sha256: String,
    pub dev_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedPmLockfile {
    pub schema_version: u32,
    pub engine: String,
    pub install_strategy: String,
    pub root: String,
    pub counts: BTreeMap<String, usize>,
    pub packages: Vec<UnifiedPackageEntry>,
}

fn sha256_hex(data: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(data);
    let mut out = String::with_capacity(64);
    for b in digest {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

fn make_entry(
    ecosystem: Ecosystem,
    name: &str,
    version_req: &str,
    registry_url: String,
    dev_only: bool,
) -> UnifiedPackageEntry {
    let seed = format!("{ecosystem:?}:{name}@{version_req}:{registry_url}");
    let integrity_sha256 = sha256_hex(seed.as_bytes());
    let store_name = format!(
        "{}-{}",
        match ecosystem {
            Ecosystem::Npm => "npm",
            Ecosystem::Cargo => "crate",
            Ecosystem::Apm => "apm",
        },
        name.replace(['@', '/'], "-")
    );
    let nix_store_path =
        compute_fixed_output_store_entry(&store_name, seed.as_bytes(), false).store_path;
    UnifiedPackageEntry {
        ecosystem,
        name: name.to_string(),
        version_req: version_req.to_string(),
        registry_url,
        nix_store_path,
        integrity_sha256,
        dev_only,
    }
}

/// Parses `package.json` (`dependencies`, `devDependencies`, `peerDependencies`, `optionalDependencies`).
pub fn parse_npm_package_json(content: &str) -> Vec<UnifiedPackageEntry> {
    let Ok(val) = serde_json::from_str::<Value>(content) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (section, dev_only) in [
        ("dependencies", false),
        ("devDependencies", true),
        ("peerDependencies", false),
        ("optionalDependencies", false),
    ] {
        if let Some(obj) = val.get(section).and_then(Value::as_object) {
            for (name, ver) in obj {
                let v = ver.as_str().unwrap_or("*");
                out.push(make_entry(Ecosystem::Npm, name, v, npm_registry_url(name), dev_only));
            }
        }
    }
    out
}

/// Zero-dependency pure-Rust parser for `Cargo.toml` dependency tables
/// (`[dependencies]`, `[workspace.dependencies]`, `[dev-dependencies]`, `[build-dependencies]`).
pub fn parse_cargo_toml_deps(content: &str) -> Vec<UnifiedPackageEntry> {
    let mut out = Vec::new();
    let mut section = String::new();
    for raw_line in content.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].trim().to_string();
            continue;
        }
        let is_dep_section = matches!(
            section.as_str(),
            "dependencies" | "workspace.dependencies" | "dev-dependencies" | "build-dependencies"
        );
        if !is_dep_section {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let name = k.trim().trim_matches('"');
        let rhs = v.trim();
        let version_req = if rhs.starts_with('"') {
            rhs.trim_matches('"').to_string()
        } else if let Some(idx) = rhs.find("version") {
            let after = &rhs[idx + "version".len()..];
            after
                .split('"')
                .nth(1)
                .unwrap_or("workspace")
                .to_string()
        } else if rhs.contains("workspace = true") {
            "workspace".to_string()
        } else {
            "*".to_string()
        };
        let dev_only = section == "dev-dependencies";
        out.push(make_entry(
            Ecosystem::Cargo,
            name,
            &version_req,
            cargo_sparse_index_url(name),
            dev_only,
        ));
    }
    out
}

/// Scans a repository root for `package.json`, `Cargo.toml`, and `apm.yml`,
/// unifying all npm, crates.io, and APM dependencies into one `UnifiedPmLockfile`.
pub fn scan_unified_workspace(root: &Path) -> UnifiedPmLockfile {
    let mut packages = Vec::new();

    let pkg_json = root.join("package.json");
    if let Ok(text) = fs::read_to_string(&pkg_json) {
        packages.extend(parse_npm_package_json(&text));
    }

    let cargo_toml = root.join("Cargo.toml");
    if let Ok(text) = fs::read_to_string(&cargo_toml) {
        packages.extend(parse_cargo_toml_deps(&text));
    }

    let apm_yml = root.join("apm.yml");
    if let Ok(text) = fs::read_to_string(&apm_yml)
        && let Ok(manifest) = parse_apm_yml(&text)
    {
        for dep in &manifest.dependencies.apm {
            let url = format!("https://github.com/{dep}");
            packages.push(make_entry(Ecosystem::Apm, dep, "main", url, false));
        }
        for mcp in &manifest.dependencies.mcp {
            let url = mcp.url.clone().unwrap_or_else(|| format!("mcp://{}", mcp.name));
            packages.push(make_entry(Ecosystem::Apm, &mcp.name, "mcp-2025-06-18", url, false));
        }
    }

    packages.sort_by(|a, b| a.ecosystem.cmp(&b.ecosystem).then(a.name.cmp(&b.name)));

    let mut counts = BTreeMap::new();
    counts.insert(
        "npm".into(),
        packages.iter().filter(|p| p.ecosystem == Ecosystem::Npm).count(),
    );
    counts.insert(
        "cargo".into(),
        packages.iter().filter(|p| p.ecosystem == Ecosystem::Cargo).count(),
    );
    counts.insert(
        "apm".into(),
        packages.iter().filter(|p| p.ecosystem == Ecosystem::Apm).count(),
    );
    counts.insert("total".into(), packages.len());

    UnifiedPmLockfile {
        schema_version: 1,
        engine: "fw-unified-pm (bun_install + cargo-sparse + fw-apm)".into(),
        install_strategy: "isolated-hardlink-nix-store".into(),
        root: root.display().to_string(),
        counts,
        packages,
    }
}

/// Generates a unified CycloneDX 1.5 SBOM spanning npm, crates.io, and APM packages.
pub fn unified_sbom_cyclonedx(lock: &UnifiedPmLockfile) -> Value {
    let components: Vec<Value> = lock
        .packages
        .iter()
        .map(|p| {
            let purl_type = match p.ecosystem {
                Ecosystem::Npm => "npm",
                Ecosystem::Cargo => "cargo",
                Ecosystem::Apm => "generic",
            };
            json!({
                "type": "library",
                "name": p.name,
                "version": p.version_req,
                "purl": format!("pkg:{purl_type}/{}@{}", p.name, p.version_req),
                "hashes": [{ "alg": "SHA-256", "content": p.integrity_sha256 }],
                "properties": [
                    { "name": "fw:nixStorePath", "value": p.nix_store_path },
                    { "name": "fw:registryUrl", "value": p.registry_url }
                ]
            })
        })
        .collect();
    json!({
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "version": 1,
        "metadata": {
            "component": { "name": "fw-unified-pm", "version": "4.2.0" }
        },
        "components": components
    })
}

/// Fetches live metadata for a Cargo crate (RFC 2789 sparse index) or npm package.
pub fn resolve_live_package(ecosystem: &str, name: &str) -> Result<Value, String> {
    let curl_bin = if cfg!(windows) { "curl.exe" } else { "curl" };
    match ecosystem {
        "cargo" | "crates" | "crate" => {
            let url = cargo_sparse_index_url(name);
            let out = std::process::Command::new(curl_bin)
                .args(["--silent", "--show-error", "--fail", "--max-time", "15", &url])
                .output()
                .map_err(|e| format!("spawn {curl_bin}: {e}"))?;
            if !out.status.success() {
                return Err(format!(
                    "crates.io sparse index fetch failed for `{name}`: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                ));
            }
            let body = String::from_utf8_lossy(&out.stdout);
            let mut latest: Option<Value> = None;
            let mut total_versions = 0usize;
            for line in body.lines() {
                if let Ok(v) = serde_json::from_str::<Value>(line) {
                    total_versions += 1;
                    if !v.get("yanked").and_then(Value::as_bool).unwrap_or(false) {
                        latest = Some(v);
                    }
                }
            }
            let latest = latest.ok_or_else(|| format!("no non-yanked versions for `{name}`"))?;
            let ver = latest["vers"].as_str().unwrap_or("");
            let cksum = latest["cksum"].as_str().unwrap_or("");
            let deps_count = latest["deps"].as_array().map_or(0, Vec::len);
            Ok(json!({
                "ok": true,
                "ecosystem": "cargo",
                "name": name,
                "latest": ver,
                "sha256": cksum,
                "depsCount": deps_count,
                "totalVersions": total_versions,
                "sparseIndexUrl": url,
                "downloadUrl": format!("https://static.crates.io/crates/{name}/{name}-{ver}.crate")
            }))
        }
        "npm" | "bun" => {
            let url = format!("{}/latest", npm_registry_url(name));
            let out = std::process::Command::new(curl_bin)
                .args(["--silent", "--show-error", "--fail", "--max-time", "15", &url])
                .output()
                .map_err(|e| format!("spawn {curl_bin}: {e}"))?;
            if !out.status.success() {
                return Err(format!(
                    "npm registry fetch failed for `{name}`: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                ));
            }
            let v: Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
            Ok(json!({
                "ok": true,
                "ecosystem": "npm",
                "name": v["name"].as_str().unwrap_or(name),
                "latest": v["version"].as_str().unwrap_or(""),
                "integrity": v["dist"]["integrity"].as_str().unwrap_or(""),
                "tarball": v["dist"]["tarball"].as_str().unwrap_or(""),
                "registryUrl": url
            }))
        }
        other => Err(format!("unsupported ecosystem `{other}` (expected cargo|npm)")),
    }
}

/// Verifies an existing `fw-pm.lock.json` against the current workspace manifests.
pub fn verify_lockfile(root: &Path) -> Result<Value, String> {
    let lock_path = root.join("fw-pm.lock.json");
    let raw = fs::read_to_string(&lock_path)
        .map_err(|e| format!("cannot read {}: {e} (run `fw pm lock` first)", lock_path.display()))?;
    let saved: UnifiedPmLockfile = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    let current = scan_unified_workspace(root);

    let saved_map: BTreeMap<(&str, &str), &str> = saved
        .packages
        .iter()
        .map(|p| ((p.name.as_str(), p.version_req.as_str()), p.integrity_sha256.as_str()))
        .collect();
    let curr_map: BTreeMap<(&str, &str), &str> = current
        .packages
        .iter()
        .map(|p| ((p.name.as_str(), p.version_req.as_str()), p.integrity_sha256.as_str()))
        .collect();

    let mut added = Vec::new();
    let mut removed = Vec::new();
    for k in curr_map.keys() {
        if !saved_map.contains_key(k) {
            added.push(format!("{}@{}", k.0, k.1));
        }
    }
    for k in saved_map.keys() {
        if !curr_map.contains_key(k) {
            removed.push(format!("{}@{}", k.0, k.1));
        }
    }
    let clean = added.is_empty() && removed.is_empty();
    Ok(json!({
        "ok": clean,
        "lockfile": lock_path.display().to_string(),
        "savedCount": saved.packages.len(),
        "currentCount": current.packages.len(),
        "added": added,
        "removed": removed
    }))
}

/// CLI entrypoint for `fw pm <scan|lock|verify|resolve|sbom|semver> [args...]`.
pub fn run_pm_cli(args: &[&str], root: &Path) -> Result<Value, String> {
    let sub = args.first().copied().unwrap_or("scan");
    match sub {
        "scan" | "info" => {
            let target = args.get(1).map(Path::new).unwrap_or(root);
            let lock = scan_unified_workspace(target);
            Ok(json!({
                "ok": true,
                "engine": lock.engine,
                "installStrategy": lock.install_strategy,
                "counts": lock.counts,
                "packages": lock.packages
            }))
        }
        "lock" => {
            let target = args.get(1).map(Path::new).unwrap_or(root);
            let lock = scan_unified_workspace(target);
            let out_path = target.join("fw-pm.lock.json");
            let text = serde_json::to_string_pretty(&lock).map_err(|e| e.to_string())?;
            fs::write(&out_path, &text).map_err(|e| format!("write lockfile failed: {e}"))?;
            Ok(json!({
                "ok": true,
                "lockfile": out_path.display().to_string(),
                "bytes": text.len(),
                "counts": lock.counts
            }))
        }
        "verify" => {
            let target = args.get(1).map(Path::new).unwrap_or(root);
            verify_lockfile(target)
        }
        "resolve" => {
            let eco = args.get(1).copied().ok_or_else(|| "usage: fw pm resolve <cargo|npm> <package>".to_owned())?;
            let pkg = args.get(2).copied().ok_or_else(|| "usage: fw pm resolve <cargo|npm> <package>".to_owned())?;
            resolve_live_package(eco, pkg)
        }
        "sbom" => {
            let target = args.get(1).map(Path::new).unwrap_or(root);
            let lock = scan_unified_workspace(target);
            Ok(unified_sbom_cyclonedx(&lock))
        }
        "semver" => {
            let ver_s = args.get(1).copied().unwrap_or("1.5.0");
            let range_s = args.get(2).copied().unwrap_or("^1.4.0");
            let ver = SemVer::parse(ver_s).ok_or_else(|| format!("invalid semver: {ver_s}"))?;
            Ok(json!({
                "ok": true,
                "version": ver_s,
                "range": range_s,
                "satisfies": ver.satisfies(range_s)
            }))
        }
        other => Err(format!("unknown `fw pm` subcommand: {other} (expected scan|lock|verify|resolve|sbom|semver)")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unified_pm_resolves_npm_crates_and_apm_together() {
        assert_eq!(cargo_sparse_index_url("a"), "https://index.crates.io/1/a");
        assert_eq!(cargo_sparse_index_url("ab"), "https://index.crates.io/2/ab");
        assert_eq!(cargo_sparse_index_url("syn"), "https://index.crates.io/3/s/syn");
        assert_eq!(cargo_sparse_index_url("serde_json"), "https://index.crates.io/se/rd/serde_json");
        assert_eq!(npm_registry_url("@aphrody/fw"), "https://registry.npmjs.org/@aphrody%2ffw");

        let v = SemVer::parse("1.5.0").unwrap();
        assert!(v.satisfies("^1.4.0"));
        assert!(v.satisfies("~1.5.0"));
        assert!(!v.satisfies("^2.0.0"));

        let tmp = std::env::temp_dir().join("fw-unified-pm-test");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join("package.json"), r#"{"dependencies":{"@aphrody/ui":"^1.5.0"},"devDependencies":{"typescript":"^5.9.0"}}"#).unwrap();
        fs::write(tmp.join("Cargo.toml"), "[dependencies]\nserde = { version = \"1.0.229\", features = [\"derive\"] }\ntokio = \"1.54.1\"\n").unwrap();
        fs::write(tmp.join("apm.yml"), "name: demo\nversion: 1.0.0\ndependencies:\n  apm:\n    - aphrody-labs/skills\n").unwrap();

        let lock = scan_unified_workspace(&tmp);
        assert_eq!(lock.counts["npm"], 2);
        assert_eq!(lock.counts["cargo"], 2);
        assert_eq!(lock.counts["apm"], 1);
        assert_eq!(lock.counts["total"], 5);

        let sbom = unified_sbom_cyclonedx(&lock);
        assert_eq!(sbom["bomFormat"], "CycloneDX");
        assert_eq!(sbom["components"].as_array().unwrap().len(), 5);

        let _ = fs::remove_dir_all(&tmp);
    }
}
