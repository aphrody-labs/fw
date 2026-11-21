//! RustSec advisory-db ingestion: `crates/<name>/RUSTSEC-*.md` files whose front matter is a fenced TOML block.

use semver::{Version, VersionReq};
use std::collections::HashMap;
use std::io::Read;

#[derive(Debug, Clone)]
pub struct Advisory {
    pub id: String,
    pub package: String,
    /// `unmaintained`, `unsound`, `notice`; `None` for a vulnerability.
    pub informational: Option<String>,
    pub withdrawn: bool,
    patched: Vec<VersionReq>,
    unaffected: Vec<VersionReq>,
}

impl Advisory {
    /// A version is affected unless a `patched` or `unaffected` requirement matches it.
    pub fn affects(&self, v: &Version) -> bool {
        !self.withdrawn && !self.patched.iter().chain(&self.unaffected).any(|r| r.matches(v))
    }
}

/// Parses one advisory file; `None` when it has no usable front matter.
pub fn parse_advisory(md: &str) -> Option<Advisory> {
    let start = md.find("```toml")? + "```toml".len();
    let end = start + md[start..].find("\n```")?;
    let doc: toml::Table = md[start..end].parse().ok()?;
    let adv = doc.get("advisory")?.as_table()?;
    let reqs = |key: &str| -> Vec<VersionReq> {
        doc.get("versions")
            .and_then(|v| v.get(key))
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|s| s.as_str()).filter_map(|s| VersionReq::parse(s).ok()).collect())
            .unwrap_or_default()
    };
    Some(Advisory {
        id: adv.get("id")?.as_str()?.to_owned(),
        package: adv.get("package")?.as_str()?.to_owned(),
        informational: adv.get("informational").and_then(|v| v.as_str()).map(str::to_owned),
        withdrawn: adv.get("withdrawn").is_some(),
        patched: reqs("patched"),
        unaffected: reqs("unaffected"),
    })
}

/// Reads every advisory from a gzip tarball of the advisory-db repository (GitHub's `tar.gz/<sha>` archive).
pub fn load_tar_gz<R: Read>(r: R) -> Result<Vec<Advisory>, String> {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(r));
    let mut out = Vec::new();
    for entry in archive.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path().map_err(|e| e.to_string())?.to_string_lossy().into_owned();
        let is_advisory = path.contains("/crates/") && path.rsplit('/').next().is_some_and(|f| f.starts_with("RUSTSEC-") && f.ends_with(".md"));
        if !is_advisory {
            continue;
        }
        let mut md = String::new();
        if entry.read_to_string(&mut md).is_ok()
            && let Some(a) = parse_advisory(&md)
        {
            out.push(a);
        }
    }
    Ok(out)
}

/// Advisories grouped by package name.
pub fn by_package(list: Vec<Advisory>) -> HashMap<String, Vec<Advisory>> {
    let mut map: HashMap<String, Vec<Advisory>> = HashMap::new();
    for a in list {
        map.entry(a.package.clone()).or_default().push(a);
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "```toml\n[advisory]\nid = \"RUSTSEC-2020-0071\"\npackage = \"time\"\ndate = \"2020-11-18\"\n\n[versions]\npatched = [\">=0.2.23\"]\nunaffected = [\"=0.2.0\"]\n```\n\n# Title\n";

    #[test]
    fn affects_unpatched_only() {
        let a = parse_advisory(SAMPLE).unwrap();
        assert_eq!(a.package, "time");
        assert!(a.affects(&Version::parse("0.2.5").unwrap()));
        assert!(!a.affects(&Version::parse("0.2.23").unwrap()));
        assert!(!a.affects(&Version::parse("0.2.0").unwrap()));
    }

    #[test]
    fn informational_and_withdrawn() {
        let md = format!("{fence}toml\n[advisory]\nid = \"RUSTSEC-2021-0001\"\npackage = \"x\"\ninformational = \"unmaintained\"\nwithdrawn = \"2022-01-01\"\n{fence}\n", fence = "\u{60}".repeat(3));
        let a = parse_advisory(&md).unwrap();
        assert_eq!(a.informational.as_deref(), Some("unmaintained"));
        assert!(!a.affects(&Version::parse("1.0.0").unwrap()));
        assert!(parse_advisory("no front matter").is_none());
    }
}
