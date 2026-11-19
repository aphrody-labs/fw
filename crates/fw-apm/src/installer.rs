use crate::audit::scan_hidden_unicode;
use crate::compiler::synthesize_mcp_configs;
use crate::lockfile::{
    ApmLockfile, FW_APM_VERSION, LOCKFILE_VERSION, LockedDependency, LockedMcpServer,
    compute_package_nix_store_path, compute_tree_hash, sha256_prefixed,
};
use crate::manifest::{
    ApmManifest, PackageSource, PrimitiveKind, parse_apm_yml, serialize_apm_yml,
};
use crate::resolver::{ResolvedPackage, resolve_manifest};
use fw_hermes_claw::SandboxPolicy;
use fw_uutils::{SUPPORTED_APPLETS, run_applet};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Default)]
pub struct InstallOptions {
    pub frozen: bool,
    pub trust_transitive_mcp: bool,
    pub force: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallReport {
    pub packages_installed: usize,
    pub mcp_servers_admitted: usize,
    pub mcp_servers_omitted: usize,
    pub mcp_configs_written: Vec<String>,
    pub lockfile_path: String,
    pub frozen_verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptRunReport {
    pub script: String,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub engine: String,
}

/// Initialize a new `apm.yml` and `.apm/` layout in `root`.
pub fn init_project(root: &Path, name: &str, force: bool) -> Result<PathBuf, String> {
    std::fs::create_dir_all(root)
        .map_err(|e| format!("failed to create directory {}: {e}", root.display()))?;
    let manifest_path = root.join("apm.yml");
    if manifest_path.exists() && !force {
        return Err(format!(
            "{} already exists (pass --force to overwrite)",
            manifest_path.display()
        ));
    }

    let manifest = ApmManifest::minimal(name);
    let yaml = serialize_apm_yml(&manifest);
    std::fs::write(&manifest_path, yaml)
        .map_err(|e| format!("failed to write {}: {e}", manifest_path.display()))?;

    for sub in ["instructions", "prompts", "agents", "skills"] {
        std::fs::create_dir_all(root.join(".apm").join(sub))
            .map_err(|e| format!("failed to create .apm/{sub}: {e}"))?;
    }

    Ok(manifest_path)
}

/// Read and parse `apm.yml` from `root`.
pub fn load_manifest(root: &Path) -> Result<(ApmManifest, String), String> {
    let manifest_path = root.join("apm.yml");
    let raw = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("failed to read {}: {e}", manifest_path.display()))?;
    let manifest = parse_apm_yml(&raw)?;
    Ok((manifest, raw))
}

fn collect_dir_files_recursive(cur: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(cur)
        .map_err(|e| format!("failed to read dir {}: {e}", cur.display()))?;
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_dir_files_recursive(&p, out)?;
        } else if p.is_file() {
            out.push(p);
        }
    }
    Ok(())
}

fn deploy_package(
    root: &Path,
    pkg: &ResolvedPackage,
    force: bool,
) -> Result<(Vec<String>, BTreeMap<String, String>), String> {
    let mut deployed_files = Vec::new();
    let mut file_hashes = BTreeMap::new();

    match pkg.spec.source {
        PackageSource::Local => {
            let src_dir = pkg
                .resolved_dir
                .as_ref()
                .ok_or_else(|| format!("missing resolved_dir for local package `{}`", pkg.spec.name))?;
            let dest_rel = format!(".agents/skills/{}", pkg.spec.name);
            let dest_dir = root.join(&dest_rel);
            std::fs::create_dir_all(&dest_dir)
                .map_err(|e| format!("failed to create {}: {e}", dest_dir.display()))?;
            deployed_files.push(dest_rel.clone());

            let mut src_files = Vec::new();
            collect_dir_files_recursive(src_dir, &mut src_files)?;
            src_files.sort();

            for src_file in src_files {
                let rel_inside = src_file
                    .strip_prefix(src_dir)
                    .unwrap_or(&src_file)
                    .to_string_lossy()
                    .replace('\\', "/");
                let bytes = std::fs::read(&src_file)
                    .map_err(|e| format!("failed to read {}: {e}", src_file.display()))?;

                // Content security gate: block hidden Unicode unless `--force`.
                if !force
                    && let Ok(text) = std::str::from_utf8(&bytes)
                {
                    let bad = scan_hidden_unicode(&rel_inside, text);
                    if !bad.is_empty() {
                        return Err(format!(
                            "blocked install of `{}`: {} (pass --force to override)",
                            pkg.spec.name, bad[0].message
                        ));
                    }
                }

                let dst_file = dest_dir.join(&rel_inside);
                if let Some(parent) = dst_file.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
                }
                std::fs::write(&dst_file, &bytes)
                    .map_err(|e| format!("failed to write {}: {e}", dst_file.display()))?;

                let deployed_rel = format!("{dest_rel}/{rel_inside}");
                file_hashes.insert(deployed_rel.clone(), sha256_prefixed(&bytes));
                deployed_files.push(deployed_rel);
            }
        }
        PackageSource::GitHub | PackageSource::Git | PackageSource::Marketplace => {
            // Materialize deterministic skill/package descriptor under `apm_modules/<name>/`
            // so offline/pure-Rust resolution, locking, and compilation are 100% reproducible.
            let dest_rel = format!("apm_modules/{}", pkg.spec.name);
            let dest_dir = root.join(&dest_rel);
            std::fs::create_dir_all(&dest_dir)
                .map_err(|e| format!("failed to create {}: {e}", dest_dir.display()))?;
            deployed_files.push(dest_rel.clone());

            let meta_rel = format!("{dest_rel}/package.meta.json");
            let meta_bytes = serde_json::to_vec_pretty(&serde_json::json!({
                "name": pkg.spec.name,
                "repo_url": pkg.spec.canonical_repo_url(),
                "source": pkg.spec.source.as_str(),
                "kind": pkg.kind.as_str(),
                "git_ref": pkg.spec.git_ref,
                "subpath": pkg.spec.subpath,
            }))
            .map_err(|e| format!("failed to serialize package metadata: {e}"))?;
            std::fs::write(root.join(&meta_rel), &meta_bytes)
                .map_err(|e| format!("failed to write {meta_rel}: {e}"))?;
            file_hashes.insert(meta_rel.clone(), sha256_prefixed(&meta_bytes));
            deployed_files.push(meta_rel);

            if matches!(pkg.kind, PrimitiveKind::Skill | PrimitiveKind::Hybrid) {
                let skill_rel = format!(".agents/skills/{}/SKILL.md", pkg.spec.name);
                let skill_path = root.join(&skill_rel);
                if let Some(parent) = skill_path.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
                }
                let skill_body = format!(
                    "---\nname: {}\ndescription: APM skill installed from {}\n---\n\n# {}\n\nSource: `{}`\n",
                    pkg.spec.name,
                    pkg.spec.canonical_repo_url(),
                    pkg.spec.name,
                    pkg.spec.canonical_repo_url()
                );
                std::fs::write(&skill_path, skill_body.as_bytes())
                    .map_err(|e| format!("failed to write {skill_rel}: {e}"))?;
                file_hashes.insert(skill_rel.clone(), sha256_prefixed(skill_body.as_bytes()));
                deployed_files.push(skill_rel);
            }
        }
    }

    Ok((deployed_files, file_hashes))
}

/// Resolve, deploy, verify/write `apm.lock.json`, and synthesize MCP configs.
pub fn install_project(root: &Path, opts: &InstallOptions) -> Result<InstallReport, String> {
    let (manifest, raw_manifest) = load_manifest(root)?;
    let manifest_sha256 = sha256_prefixed(raw_manifest.as_bytes());
    let plan = resolve_manifest(root, &manifest, opts.trust_transitive_mcp)?;

    let mut locked_deps = Vec::with_capacity(plan.packages.len());
    for pkg in &plan.packages {
        let (deployed_files, deployed_file_hashes) = deploy_package(root, pkg, opts.force)?;
        let content_hash = compute_tree_hash(&deployed_file_hashes);
        let nix_store_path = compute_package_nix_store_path(&pkg.spec.name, &content_hash);
        locked_deps.push(LockedDependency {
            repo_url: pkg.spec.canonical_repo_url(),
            name: pkg.spec.name.clone(),
            version: pkg.version.clone(),
            package_type: pkg.kind,
            source: pkg.spec.source.clone(),
            local_path: pkg.spec.local_path.clone(),
            git_ref: pkg.spec.git_ref.clone(),
            resolved_commit: pkg.spec.git_ref.clone(),
            depth: pkg.depth,
            deployed_files,
            deployed_file_hashes,
            content_hash,
            nix_store_path,
            declared_license: pkg.license.clone(),
        });
    }

    let locked_mcps: Vec<LockedMcpServer> = plan
        .mcp_servers
        .iter()
        .map(|m| LockedMcpServer {
            name: m.spec.name.clone(),
            transport: m.spec.transport.as_str().to_string(),
            source_package: m.source_package.clone(),
            depth: m.depth,
            definition_fingerprint: m.fingerprint.clone(),
            admitted: true,
        })
        .collect();

    let new_lock = ApmLockfile {
        lockfile_version: LOCKFILE_VERSION.to_string(),
        apm_version: FW_APM_VERSION.to_string(),
        manifest_sha256,
        dependencies: locked_deps,
        mcp_servers: locked_mcps,
        mcp_omissions: plan.mcp_omissions.clone(),
    };

    let lock_path = root.join("apm.lock.json");
    if opts.frozen {
        let existing = ApmLockfile::load_json(&lock_path)
            .map_err(|e| format!("--frozen requires an existing apm.lock.json: {e}"))?;
        if existing != new_lock {
            return Err(
                "frozen lockfile drift: apm.lock.json does not match current apm.yml and resolved packages"
                    .into(),
            );
        }
    } else {
        new_lock.save_json(&lock_path)?;
    }

    let mcp_configs_written = synthesize_mcp_configs(root, &manifest.targets, &plan.mcp_servers)?;

    Ok(InstallReport {
        packages_installed: new_lock.dependencies.len(),
        mcp_servers_admitted: new_lock.mcp_servers.len(),
        mcp_servers_omitted: new_lock.mcp_omissions.len(),
        mcp_configs_written,
        lockfile_path: "apm.lock.json".into(),
        frozen_verified: opts.frozen,
    })
}

/// Remove a package from `apm.yml`, delete its deployed files, and update `apm.lock.json`.
pub fn uninstall_package(root: &Path, pkg_name: &str) -> Result<bool, String> {
    let (mut manifest, _) = load_manifest(root)?;
    let before = manifest.dependencies.apm.len();
    manifest.dependencies.apm.retain(|raw| {
        crate::manifest::PackageSpec::parse(raw, manifest.default_host.as_deref())
            .map(|s| s.name != pkg_name && s.raw != pkg_name)
            .unwrap_or(true)
    });
    let removed_from_manifest = manifest.dependencies.apm.len() < before;

    // Clean deployed files tracked in lockfile (if present)
    let lock_path = root.join("apm.lock.json");
    if let Ok(mut lock) = ApmLockfile::load_json(&lock_path) {
        for dep in lock.dependencies.iter().filter(|d| d.name == pkg_name) {
            for rel in &dep.deployed_files {
                let p = root.join(rel);
                if p.is_file() {
                    let _ = std::fs::remove_file(&p);
                }
            }
        }
        let skill_dir = root.join(".agents").join("skills").join(pkg_name);
        if skill_dir.is_dir() {
            let _ = std::fs::remove_dir_all(&skill_dir);
        }
        let mod_dir = root.join("apm_modules").join(pkg_name);
        if mod_dir.is_dir() {
            let _ = std::fs::remove_dir_all(&mod_dir);
        }
        lock.dependencies.retain(|d| d.name != pkg_name);
        lock.save_json(&lock_path)?;
    }

    if removed_from_manifest {
        let updated = serialize_apm_yml(&manifest);
        std::fs::write(root.join("apm.yml"), updated)
            .map_err(|e| format!("failed to update apm.yml: {e}"))?;
    }

    Ok(removed_from_manifest)
}

/// List locked or declared dependencies.
pub fn list_installed(root: &Path) -> Result<Vec<LockedDependency>, String> {
    let lock_path = root.join("apm.lock.json");
    if lock_path.exists() {
        let lock = ApmLockfile::load_json(&lock_path)?;
        return Ok(lock.dependencies);
    }
    let (manifest, _) = load_manifest(root)?;
    let plan = resolve_manifest(root, &manifest, false)?;
    Ok(plan
        .packages
        .into_iter()
        .map(|p| LockedDependency {
            repo_url: p.spec.canonical_repo_url(),
            name: p.spec.name,
            version: p.version,
            package_type: p.kind,
            source: p.spec.source,
            local_path: p.spec.local_path,
            git_ref: p.spec.git_ref,
            resolved_commit: None,
            depth: p.depth,
            deployed_files: Vec::new(),
            deployed_file_hashes: BTreeMap::new(),
            content_hash: String::new(),
            nix_store_path: String::new(),
            declared_license: p.license,
        })
        .collect())
}

/// Execute a named script from `apm.yml` `scripts:`.
///
/// Pure-Rust `fw-uutils` applets (`echo`, `cat`, `wc`, `head`, `tail`, `sha256sum`, `uname`, etc.)
/// execute in-process with zero subprocess overhead. External binaries are validated against
/// `fw_hermes_claw::SandboxPolicy` before execution.
pub fn run_script(root: &Path, script_name: &str) -> Result<ScriptRunReport, String> {
    let (manifest, _) = load_manifest(root)?;
    let cmd_line = manifest
        .scripts
        .get(script_name)
        .ok_or_else(|| format!("script `{script_name}` not found in apm.yml"))?;

    let parts: Vec<&str> = cmd_line.split_whitespace().collect();
    let (&binary, rest) = parts
        .split_first()
        .ok_or_else(|| format!("script `{script_name}` is empty"))?;

    if SUPPORTED_APPLETS.contains(&binary) {
        let out = run_applet(binary, rest, "");
        return Ok(ScriptRunReport {
            script: script_name.to_string(),
            exit_code: out.exit_code,
            stdout: out.stdout,
            stderr: out.stderr,
            engine: "fw-uutils".into(),
        });
    }

    let policy = SandboxPolicy::default();
    let owned_args: Vec<String> = rest.iter().map(|s| (*s).to_string()).collect();
    policy.validate(binary, &owned_args)?;

    let out = Command::new(binary)
        .args(rest)
        .current_dir(root)
        .output()
        .map_err(|e| format!("failed to spawn `{binary}`: {e}"))?;

    Ok(ScriptRunReport {
        script: script_name.to_string(),
        exit_code: out.status.code().unwrap_or(1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        engine: "sandboxed-process".into(),
    })
}

/// Record a SHA-256 content-pinned approval in `apm.yml` (`executables.allow.<pkg>.<item>`).
pub fn approve_executable_pin(
    root: &Path,
    pkg_key: &str,
    item_key: &str,
    fingerprint: &str,
) -> Result<(), String> {
    let (mut manifest, _) = load_manifest(root)?;
    manifest.executables.pin_item(pkg_key, item_key, fingerprint);
    let updated = serialize_apm_yml(&manifest);
    std::fs::write(root.join("apm.yml"), updated)
        .map_err(|e| format!("failed to write updated apm.yml: {e}"))
}
