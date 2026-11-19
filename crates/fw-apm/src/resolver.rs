use crate::lockfile::{McpOmission, compute_mcp_fingerprint};
use crate::manifest::{
    ApmManifest, McpServerSpec, PackageSource, PackageSpec, PrimitiveKind, parse_apm_yml,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// A resolved APM package node in the dependency graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPackage {
    pub spec: PackageSpec,
    pub version: String,
    pub license: Option<String>,
    pub depth: usize,
    pub resolved_dir: Option<PathBuf>,
    pub kind: PrimitiveKind,
    pub children: Vec<String>,
}

/// An admitted MCP server in the resolved plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedMcp {
    pub spec: McpServerSpec,
    pub source_package: String,
    pub depth: usize,
    pub fingerprint: String,
}

/// Full resolved dependency graph + MCP admission decisions.
#[derive(Debug, Clone, Default)]
pub struct ResolutionPlan {
    pub packages: Vec<ResolvedPackage>,
    pub mcp_servers: Vec<ResolvedMcp>,
    pub mcp_omissions: Vec<McpOmission>,
}

/// Resolve all direct and transitive dependencies declared in `manifest`.
///
/// - Detects cycles on local package graphs.
/// - Deduplicates diamond dependencies (keeping minimum depth).
/// - Enforces OpenAPM `req-sc-016` transitive MCP trust gating:
///   direct (`depth == 0`) MCP servers are admitted; transitive (`depth > 0`) MCP servers
///   require either root re-declaration, a SHA-256 pin in `manifest.executables.allow`,
///   or explicit non-persisted `trust_transitive_mcp = true`.
pub fn resolve_manifest(
    root: &Path,
    manifest: &ApmManifest,
    trust_transitive_mcp: bool,
) -> Result<ResolutionPlan, String> {
    let mut resolved: BTreeMap<String, ResolvedPackage> = BTreeMap::new();
    let mut active_stack: BTreeSet<String> = BTreeSet::new();
    let mut mcp_servers: Vec<ResolvedMcp> = Vec::new();
    let mut mcp_omissions: Vec<McpOmission> = Vec::new();
    let mut direct_mcp_names: BTreeSet<String> = BTreeSet::new();

    // Direct MCP servers declared in root apm.yml are always admitted.
    for mcp in &manifest.dependencies.mcp {
        let fp = compute_mcp_fingerprint(mcp);
        direct_mcp_names.insert(mcp.name.clone());
        mcp_servers.push(ResolvedMcp {
            spec: mcp.clone(),
            source_package: manifest.name.clone(),
            depth: 0,
            fingerprint: fp,
        });
    }

    let direct_deps = manifest.parsed_apm_deps()?;
    for spec in direct_deps {
        resolve_recursive(
            root,
            root,
            manifest,
            spec,
            0,
            trust_transitive_mcp,
            &direct_mcp_names,
            &mut active_stack,
            &mut resolved,
            &mut mcp_servers,
            &mut mcp_omissions,
        )?;
    }

    Ok(ResolutionPlan {
        packages: resolved.into_values().collect(),
        mcp_servers,
        mcp_omissions,
    })
}

#[allow(clippy::too_many_arguments)]
fn resolve_recursive(
    workspace_root: &Path,
    parent_dir: &Path,
    root_manifest: &ApmManifest,
    spec: PackageSpec,
    depth: usize,
    trust_transitive_mcp: bool,
    direct_mcp_names: &BTreeSet<String>,
    active_stack: &mut BTreeSet<String>,
    resolved: &mut BTreeMap<String, ResolvedPackage>,
    mcp_servers: &mut Vec<ResolvedMcp>,
    mcp_omissions: &mut Vec<McpOmission>,
) -> Result<(), String> {
    let key = spec.canonical_repo_url();
    if active_stack.contains(&key) {
        return Err(format!("dependency cycle detected involving `{key}`"));
    }
    if let Some(existing) = resolved.get_mut(&key) {
        if depth < existing.depth {
            existing.depth = depth;
        }
        return Ok(());
    }

    active_stack.insert(key.clone());

    let mut version = spec
        .git_ref
        .clone()
        .unwrap_or_else(|| "0.1.0".to_string());
    let mut license = None;
    let mut kind = spec.kind;
    let mut resolved_dir = None;
    let mut children = Vec::new();

    if spec.source == PackageSource::Local {
        let rel = spec
            .local_path
            .as_deref()
            .ok_or_else(|| format!("local dependency `{}` missing local_path", spec.name))?;
        let candidate = parent_dir.join(rel);
        let pkg_dir = if candidate.exists() {
            candidate
        } else {
            workspace_root.join(rel)
        };
        if !pkg_dir.exists() {
            active_stack.remove(&key);
            return Err(format!(
                "local dependency `{}` not found at {}",
                spec.name,
                pkg_dir.display()
            ));
        }

        let has_skill = pkg_dir.join("SKILL.md").exists();
        let nested_manifest_path = pkg_dir.join("apm.yml");
        if nested_manifest_path.exists() {
            let raw = std::fs::read_to_string(&nested_manifest_path).map_err(|e| {
                format!("failed to read {}: {e}", nested_manifest_path.display())
            })?;
            let nested = parse_apm_yml(&raw)?;
            version = nested.version.clone();
            license = nested.license.clone();
            kind = if has_skill {
                PrimitiveKind::Hybrid
            } else {
                PrimitiveKind::ApmPackage
            };

            // Gate transitive MCP servers declared by dependency packages.
            for mcp in &nested.dependencies.mcp {
                let fp = compute_mcp_fingerprint(mcp);
                let approved = direct_mcp_names.contains(&mcp.name)
                    || root_manifest
                        .executables
                        .is_mcp_approved(&spec.name, &mcp.name, &fp)
                    || root_manifest
                        .executables
                        .is_mcp_approved(&key, &mcp.name, &fp)
                    || trust_transitive_mcp;

                if approved {
                    if !mcp_servers.iter().any(|m| m.spec.name == mcp.name) {
                        mcp_servers.push(ResolvedMcp {
                            spec: mcp.clone(),
                            source_package: spec.name.clone(),
                            depth: depth + 1,
                            fingerprint: fp,
                        });
                    }
                } else {
                    let reason = if root_manifest.executables.allow.contains_key(&spec.name)
                        || root_manifest.executables.allow.contains_key(&key)
                    {
                        "definition-changed".to_string()
                    } else {
                        "unapproved-transitive-mcp".to_string()
                    };
                    mcp_omissions.push(McpOmission {
                        name: mcp.name.clone(),
                        source_package: spec.name.clone(),
                        reason,
                        definition_fingerprint: fp,
                    });
                }
            }

            // Recurse into transitive APM dependencies.
            for child_spec in nested.parsed_apm_deps()? {
                children.push(child_spec.canonical_repo_url());
                resolve_recursive(
                    workspace_root,
                    &pkg_dir,
                    root_manifest,
                    child_spec,
                    depth + 1,
                    trust_transitive_mcp,
                    direct_mcp_names,
                    active_stack,
                    resolved,
                    mcp_servers,
                    mcp_omissions,
                )?;
            }
        } else if has_skill {
            kind = PrimitiveKind::Skill;
        }
        resolved_dir = Some(pkg_dir);
    }

    active_stack.remove(&key);
    resolved.insert(
        key,
        ResolvedPackage {
            spec,
            version,
            license,
            depth,
            resolved_dir,
            kind,
            children,
        },
    );
    Ok(())
}
