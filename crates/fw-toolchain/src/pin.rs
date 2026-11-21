use crate::config::{Artifact, Config, LlvmAsset, Lock, LockLlvm, LockRust, LockTool};
use anyhow::{anyhow, bail, Context, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::Read;

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn github(path: &str, raw: bool) -> Result<Vec<u8>> {
    let mut request = ureq::get(format!("https://api.github.com/{path}"))
        .header("user-agent", "fw-toolchain")
        .header("x-github-api-version", "2022-11-28")
        .header("accept", if raw { "application/vnd.github.raw+json" } else { "application/vnd.github+json" });
    if let Ok(token) = std::env::var("GH_TOKEN").or_else(|_| std::env::var("GITHUB_TOKEN")) {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    let mut body = request.call().with_context(|| format!("GET {path}"))?.into_body();
    let mut bytes = Vec::new();
    body.as_reader().read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn github_json(path: &str) -> Result<Value> {
    Ok(serde_json::from_slice(&github(path, false)?)?)
}

fn exists(path: &str) -> bool {
    github(path, false).is_ok()
}

/// The commit a tag points at, peeling an annotated tag object.
pub fn tag_commit(repo: &str, tag: &str) -> Result<String> {
    let reference = github_json(&format!("repos/{repo}/git/ref/tags/{tag}"))?;
    let object = &reference["object"];
    let sha = object["sha"].as_str().ok_or_else(|| anyhow!("{repo} tag {tag}: no sha"))?;
    if object["type"] == "tag" {
        let annotated = github_json(&format!("repos/{repo}/git/tags/{sha}"))?;
        return annotated["object"]["sha"].as_str().map(str::to_owned).ok_or_else(|| anyhow!("{repo} tag {tag}: no peeled sha"));
    }
    Ok(sha.to_owned())
}

fn has_commit(repo: &str, sha: &str) -> bool {
    exists(&format!("repos/{repo}/commits/{sha}"))
}

fn cmake_version(text: &str, key: &str) -> Result<String> {
    for line in text.lines() {
        if let Some(rest) = line.trim().strip_prefix(&format!("set({key} ")) {
            return Ok(rest.trim_end_matches(')').trim().to_owned());
        }
    }
    bail!("{key} not found")
}

pub fn pin(config: &Config, version: &str) -> Result<Lock> {
    let commit = tag_commit(&config.rust.upstream, version)?;
    let fork_has_tag = tag_commit(&config.rust.fork, version).map(|fork_commit| fork_commit == commit).unwrap_or(false);

    let llvm_link = github_json(&format!("repos/{}/contents/src/llvm-project?ref={version}", config.rust.upstream))?;
    let llvm_commit = llvm_link["sha"].as_str().ok_or_else(|| anyhow!("no src/llvm-project gitlink at {version}"))?.to_owned();
    let modules = String::from_utf8(github(&format!("repos/{}/contents/.gitmodules?ref={version}", config.rust.upstream), true)?)?;
    let branch = modules
        .split("[submodule \"src/llvm-project\"]")
        .nth(1)
        .and_then(|section| section.lines().find_map(|line| line.trim().strip_prefix("branch = ").map(str::to_owned)))
        .ok_or_else(|| anyhow!("no branch for src/llvm-project in .gitmodules at {version}"))?;
    let cmake = String::from_utf8(github(&format!("repos/{}/contents/cmake/Modules/LLVMVersion.cmake?ref={llvm_commit}", config.llvm.upstream), true)?)?;
    let llvm_version = format!("{}.{}.{}", cmake_version(&cmake, "LLVM_VERSION_MAJOR")?, cmake_version(&cmake, "LLVM_VERSION_MINOR")?, cmake_version(&cmake, "LLVM_VERSION_PATCH")?);

    let release = github_json(&format!("repos/{}/releases/tags/llvmorg-{llvm_version}", config.llvm.release_repo))?;
    let mut llvm_assets = Vec::new();
    for pattern in &config.llvm.assets {
        let name = pattern.replace("{version}", &llvm_version);
        let asset = release["assets"].as_array().and_then(|assets| assets.iter().find(|asset| asset["name"] == name.as_str())).ok_or_else(|| anyhow!("llvmorg-{llvm_version} has no asset {name}"))?;
        let digest = asset["digest"].as_str().and_then(|digest| digest.strip_prefix("sha256:")).ok_or_else(|| anyhow!("asset {name} has no sha256 digest"))?;
        llvm_assets.push(LlvmAsset { name, url: asset["browser_download_url"].as_str().unwrap_or_default().to_owned(), sha256: digest.to_owned(), bytes: asset["size"].as_u64().unwrap_or_default() });
    }

    let manifest_url = format!("{}/dist/channel-rust-{version}.toml", config.rust.dist_server);
    let mut manifest_bytes = Vec::new();
    ureq::get(&manifest_url).call().with_context(|| manifest_url.clone())?.into_body().as_reader().read_to_end(&mut manifest_bytes)?;
    let manifest: toml::Value = toml::from_str(std::str::from_utf8(&manifest_bytes)?)?;
    let mut artifacts = Vec::new();
    let wanted: Vec<&str> = config.targets.iter().map(|target| target.triple.as_str()).chain([config.rust.host.as_str()]).collect();
    for package in &config.rust.packages {
        let targets = manifest.get("pkg").and_then(|pkg| pkg.get(package)).and_then(|entry| entry.get("target")).and_then(toml::Value::as_table).ok_or_else(|| anyhow!("manifest has no package {package}"))?;
        for (target, entry) in targets {
            let applies = target == "*" || (package != "rust-std" && target == &config.rust.host) || (package == "rust-std" && wanted.contains(&target.as_str()));
            let (Some(url), Some(hash)) = (entry.get("xz_url").and_then(toml::Value::as_str), entry.get("xz_hash").and_then(toml::Value::as_str)) else { continue };
            if applies && entry.get("available").and_then(toml::Value::as_bool).unwrap_or(false) {
                artifacts.push(Artifact { package: package.clone(), target: target.clone(), url: url.to_owned(), sha256: hash.to_owned() });
            }
        }
    }
    artifacts.sort_by(|a, b| (&a.package, &a.target).cmp(&(&b.package, &b.target)));

    let mut tools = Vec::new();
    for (name, tool) in [("xwin", (&config.xwin.repo, &config.xwin.upstream, &config.xwin.tag)), ("cargo-xwin", (&config.cargo_xwin.repo, &config.cargo_xwin.upstream, &config.cargo_xwin.tag)), ("cargo-zigbuild", (&config.cargo_zigbuild.repo, &config.cargo_zigbuild.upstream, &config.cargo_zigbuild.tag))] {
        let commit = match (name, &config.xwin.rev) {
            ("xwin", Some(rev)) => rev.clone(),
            _ => tag_commit(tool.1, tool.2)?,
        };
        tools.push(LockTool { name: name.to_owned(), fork: tool.0.clone(), tag: tool.2.clone(), fork_has_commit: has_commit(tool.0, &commit), commit });
    }

    Ok(Lock {
        rust: LockRust {
            version: version.to_owned(),
            fork_has_tag,
            commit,
            manifest_sha256: sha256_hex(&manifest_bytes),
            manifest_date: manifest.get("date").and_then(toml::Value::as_str).unwrap_or_default().to_owned(),
        },
        llvm: LockLlvm { repo: config.llvm.upstream.clone(), branch, fork_has_commit: has_commit(&config.llvm.fork, &llvm_commit), commit: llvm_commit, version: llvm_version, asset: llvm_assets },
        tool: tools,
        artifact: artifacts,
        xwin_terms: None,
    })
}
