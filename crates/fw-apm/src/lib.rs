#![forbid(unsafe_code)]

//! `fw-apm` — Pure-Rust migration of `microsoft/apm` (Agent Package Manager).
//!
//! Replaces the 17-dependency Python stack (`click`, `pyyaml`, `ruamel.yaml`, `python-frontmatter`,
//! `GitPython`, `requests`, `rich`, `llm`, `filelock`, `watchdog`, etc.) with zero-bloat Rust
//! built directly on `fw` workspace crates (`fw-hermes-claw`, `fw-nix`, `fw-uutils`, `serde`,
//! `serde_json`, `sha2`, `flate2`):
//!
//! - `manifest`: Pure-Rust `apm.yml` parser/serializer, dependency spec grammar, and OpenAPM `req-sc-016` pins.
//! - `resolver`: Transitive dependency resolver with cycle detection, diamond dedup, and MCP trust gating.
//! - `lockfile`: Canonical `apm.lock.json` + `apm.lock.yaml` with SHA-256 integrity, `fw-nix` store paths,
//!   CycloneDX/SPDX SBOM export, and `flate2` gzip bundle packing.
//! - `compiler`: Frontmatter parser, `AGENTS.md` + multi-harness instruction compiler, and MCP JSON synthesizer.
//! - `installer`: `init`, `install` (`--frozen`), `uninstall`, `list`, `run`, and `approve`.
//! - `audit`: Hidden-Unicode (BiDi/zero-width/tag) scanner, SHA-256 drift detection, and `--ci` gate.

pub mod audit;
pub mod compiler;
pub mod installer;
pub mod lockfile;
pub mod manifest;
pub mod resolver;
pub mod unified_pm;

use serde_json::{Value, json};
use std::path::Path;

/// Execute an `apm` CLI subcommand (`init`, `install`, `compile`, `run`, `list`, `audit`,
/// `uninstall`, `deps`, `approve`, `pack`) against `root` and return structured JSON output.
pub fn run_cli(args: &[&str], root: &Path) -> Result<Value, String> {
    let Some((&cmd, rest)) = args.split_first() else {
        return Ok(json!({
            "name": "fw-apm",
            "version": env!("CARGO_PKG_VERSION"),
            "commands": [
                "init", "install", "compile", "run", "list",
                "audit", "uninstall", "deps", "approve", "pack"
            ]
        }));
    };

    match cmd {
        "init" => {
            let name = rest
                .iter()
                .find(|a| !a.starts_with('-'))
                .copied()
                .unwrap_or("my-agent-project");
            let force = rest.contains(&"--force");
            let path = installer::init_project(root, name, force)?;
            Ok(json!({ "ok": true, "manifest": path.display().to_string() }))
        }
        "install" => {
            let opts = installer::InstallOptions {
                frozen: rest.contains(&"--frozen"),
                trust_transitive_mcp: rest.contains(&"--trust-transitive-mcp"),
                force: rest.contains(&"--force"),
            };
            let report = installer::install_project(root, &opts)?;
            serde_json::to_value(report).map_err(|e| e.to_string())
        }
        "compile" => {
            let (manifest, _) = installer::load_manifest(root)?;
            let mut override_targets = Vec::new();
            let mut i = 0;
            while i < rest.len() {
                if (rest[i] == "-t" || rest[i] == "--target") && i + 1 < rest.len() {
                    override_targets.push(manifest::TargetHarness::parse(rest[i + 1])?);
                    i += 2;
                } else {
                    i += 1;
                }
            }
            let slice = if override_targets.is_empty() {
                None
            } else {
                Some(override_targets.as_slice())
            };
            let report = compiler::compile_project(root, &manifest, slice)?;
            serde_json::to_value(report).map_err(|e| e.to_string())
        }
        "run" => {
            let script = rest
                .first()
                .copied()
                .ok_or_else(|| "usage: apm run <script>".to_string())?;
            let report = installer::run_script(root, script)?;
            serde_json::to_value(report).map_err(|e| e.to_string())
        }
        "list" => {
            let deps = installer::list_installed(root)?;
            Ok(json!({ "count": deps.len(), "dependencies": deps }))
        }
        "audit" => {
            let (manifest, _) = installer::load_manifest(root)?;
            let ci = rest.contains(&"--ci");
            let report = audit::audit_project(root, &manifest, ci);
            serde_json::to_value(report).map_err(|e| e.to_string())
        }
        "uninstall" => {
            let pkg = rest
                .first()
                .copied()
                .ok_or_else(|| "usage: apm uninstall <package>".to_string())?;
            let removed = installer::uninstall_package(root, pkg)?;
            Ok(json!({ "ok": true, "package": pkg, "removed_from_manifest": removed }))
        }
        "deps" => {
            let (manifest, _) = installer::load_manifest(root)?;
            let plan = resolver::resolve_manifest(root, &manifest, false)?;
            let nodes: Vec<Value> = plan
                .packages
                .iter()
                .map(|p| {
                    json!({
                        "name": p.spec.name,
                        "repo_url": p.spec.canonical_repo_url(),
                        "version": p.version,
                        "depth": p.depth,
                        "kind": p.kind.as_str(),
                        "children": p.children,
                    })
                })
                .collect();
            Ok(json!({
                "packages": nodes,
                "mcp_servers": plan.mcp_servers.len(),
                "mcp_omissions": plan.mcp_omissions,
            }))
        }
        "approve" => {
            if rest.len() < 3 {
                return Err("usage: apm approve <package> <item> <sha256:...>".into());
            }
            installer::approve_executable_pin(root, rest[0], rest[1], rest[2])?;
            Ok(json!({ "ok": true, "package": rest[0], "item": rest[1], "pin": rest[2] }))
        }
        "pack" => {
            let lock = lockfile::ApmLockfile::load_json(&root.join("apm.lock.json"))?;
            let gz = lock.pack_gzip_bundle()?;
            let out_path = root.join("apm-bundle.json.gz");
            std::fs::write(&out_path, &gz)
                .map_err(|e| format!("failed to write {}: {e}", out_path.display()))?;
            Ok(json!({
                "ok": true,
                "bundle": "apm-bundle.json.gz",
                "bytes": gz.len(),
                "sha256": lockfile::sha256_prefixed(&gz),
            }))
        }
        other => Err(format!("unknown apm subcommand: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::{FindingSeverity, audit_project, scan_hidden_unicode};
    use crate::compiler::{compile_project, split_frontmatter, synthesize_cli_skill_md};
    use crate::installer::{
        InstallOptions, approve_executable_pin, init_project, install_project, list_installed,
        run_script, uninstall_package,
    };
    use crate::lockfile::{ApmLockfile, SbomFormat, compute_mcp_fingerprint};
    use crate::manifest::{
        McpServerSpec, McpTransport, PackageSource, PackageSpec, PrimitiveKind, TargetHarness,
        parse_apm_yml, serialize_apm_yml,
    };
    use crate::resolver::resolve_manifest;
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_workspace(label: &str) -> PathBuf {
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "fw_apm_test_{label}_{}_{id}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn parse_and_roundtrip_microsoft_apm_yml() {
        let sample = r#"
# Real microsoft/apm manifest schema
name: demo-agent-app
version: 1.2.3
description: Portable AI agent context
includes: auto
author: Aphrody
license: Apache-2.0
targets:
  - copilot
  - claude
  - antigravity
dependencies:
  apm:
    - anthropics/skills/skills/frontend-design
    - github/awesome-copilot/agents/api-architect.agent.md
    - microsoft/apm-sample-package#v1.0.0
    - ./packages/local-skill
    - azure-cloud-development@awesome-copilot
  mcp:
    - name: io.github.github/github-mcp-server
      transport: http
      url: https://api.githubcopilot.com/mcp/
    - name: local-rg
      transport: stdio
      command: rg
      args:
        - --json
scripts:
  hello: "echo fw-apm-ok"
"#;

        let parsed = parse_apm_yml(sample).expect("parse apm.yml");
        assert_eq!(parsed.name, "demo-agent-app");
        assert_eq!(parsed.version, "1.2.3");
        assert_eq!(parsed.targets.len(), 3);
        assert_eq!(parsed.targets[2], TargetHarness::Antigravity);
        assert_eq!(parsed.dependencies.apm.len(), 5);
        assert_eq!(parsed.dependencies.mcp.len(), 2);
        assert_eq!(parsed.dependencies.mcp[0].transport, McpTransport::Http);
        assert_eq!(parsed.dependencies.mcp[1].args, vec!["--json"]);
        assert_eq!(
            parsed.scripts.get("hello").map(String::as_str),
            Some("echo fw-apm-ok")
        );

        let specs = parsed.parsed_apm_deps().expect("parse dep specs");
        assert_eq!(specs[0].kind, PrimitiveKind::Skill);
        assert_eq!(specs[0].source, PackageSource::GitHub);
        assert_eq!(specs[1].kind, PrimitiveKind::Agent);
        assert_eq!(specs[2].git_ref.as_deref(), Some("v1.0.0"));
        assert_eq!(specs[3].source, PackageSource::Local);
        assert_eq!(specs[4].source, PackageSource::Marketplace);
        assert_eq!(specs[4].marketplace.as_deref(), Some("awesome-copilot"));

        // Round-trip serialization
        let serialized = serialize_apm_yml(&parsed);
        let reparsed = parse_apm_yml(&serialized).expect("reparse serialized apm.yml");
        assert_eq!(parsed, reparsed);
    }

    #[test]
    fn init_and_run_pure_rust_uutils_script() {
        let dir = temp_workspace("init_run");
        let manifest_path = init_project(&dir, "fw-demo", false).expect("init project");
        assert!(manifest_path.exists());
        assert!(dir.join(".apm/instructions").is_dir());

        // Add a script and run it via fw-uutils (zero subprocess)
        let mut m = parse_apm_yml(&std::fs::read_to_string(&manifest_path).unwrap()).unwrap();
        m.scripts
            .insert("greet".into(), "echo hello from fw-apm".into());
        std::fs::write(&manifest_path, serialize_apm_yml(&m)).unwrap();

        let res = run_script(&dir, "greet").expect("run script");
        assert_eq!(res.exit_code, 0);
        assert_eq!(res.engine, "fw-uutils");
        assert_eq!(res.stdout, "hello from fw-apm\n");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_transitive_deps_and_gate_mcp_pins() {
        let dir = temp_workspace("resolve_gate");

        // Create transitive leaf package with an MCP server
        let leaf = dir.join("packages/leaf-worker");
        std::fs::create_dir_all(&leaf).unwrap();
        std::fs::write(
            leaf.join("SKILL.md"),
            "---\nname: leaf-worker\ndescription: Transitive worker skill\n---\n# Leaf\n",
        )
        .unwrap();
        std::fs::write(
            leaf.join("apm.yml"),
            r#"name: leaf-worker
version: 0.2.0
license: Apache-2.0
dependencies:
  apm: []
  mcp:
    - name: deep-mcp
      transport: http
      url: https://example.invalid/mcp
"#,
        )
        .unwrap();

        // Create direct package depending on leaf-worker
        let sched = dir.join("packages/scheduler");
        std::fs::create_dir_all(&sched).unwrap();
        std::fs::write(
            sched.join("SKILL.md"),
            "---\nname: scheduler\ndescription: Scheduler skill\n---\n# Scheduler\n",
        )
        .unwrap();
        std::fs::write(
            sched.join("apm.yml"),
            r#"name: scheduler
version: 0.3.0
license: MIT
dependencies:
  apm:
    - ../leaf-worker
  mcp: []
"#,
        )
        .unwrap();

        // Root manifest declares scheduler (deep-mcp is transitive -> parked by default)
        let root_yml = r#"name: root-app
version: 1.0.0
targets:
  - copilot
  - claude
dependencies:
  apm:
    - ./packages/scheduler
  mcp: []
"#;
        std::fs::write(dir.join("apm.yml"), root_yml).unwrap();

        let manifest = parse_apm_yml(root_yml).unwrap();
        let plan_default = resolve_manifest(&dir, &manifest, false).expect("resolve default");
        assert_eq!(plan_default.packages.len(), 2);
        assert_eq!(plan_default.mcp_servers.len(), 0);
        assert_eq!(plan_default.mcp_omissions.len(), 1);
        assert_eq!(plan_default.mcp_omissions[0].reason, "unapproved-transitive-mcp");

        // Pin the exact SHA-256 fingerprint via approve_executable_pin (req-sc-016)
        let expected_fp = compute_mcp_fingerprint(&McpServerSpec {
            name: "deep-mcp".into(),
            transport: McpTransport::Http,
            command: None,
            args: Vec::new(),
            url: Some("https://example.invalid/mcp".into()),
            env: BTreeMap::new(),
        });
        assert_eq!(expected_fp, plan_default.mcp_omissions[0].definition_fingerprint);

        approve_executable_pin(&dir, "leaf-worker", "deep-mcp", &expected_fp)
            .expect("approve pin");
        let updated = parse_apm_yml(&std::fs::read_to_string(dir.join("apm.yml")).unwrap()).unwrap();
        let plan_pinned = resolve_manifest(&dir, &updated, false).expect("resolve pinned");
        assert_eq!(plan_pinned.mcp_servers.len(), 1);
        assert_eq!(plan_pinned.mcp_servers[0].spec.name, "deep-mcp");
        assert_eq!(plan_pinned.mcp_omissions.len(), 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_lockfile_frozen_compile_list_audit_and_uninstall() {
        let dir = temp_workspace("full_lifecycle");

        // Create a local skill package
        let pkg = dir.join("packages/code-review");
        std::fs::create_dir_all(&pkg).unwrap();
        std::fs::write(
            pkg.join("SKILL.md"),
            "---\nname: code-review\ndescription: Deterministic Rust code review skill\n---\n# Code Review\nCheck clippy.\n",
        )
        .unwrap();
        std::fs::write(
            pkg.join("apm.yml"),
            "name: code-review\nversion: 1.0.0\nlicense: MIT\ndependencies:\n  apm: []\n  mcp: []\n",
        )
        .unwrap();

        // Create local instruction
        std::fs::create_dir_all(dir.join(".apm/instructions")).unwrap();
        std::fs::write(
            dir.join(".apm/instructions/rust.instructions.md"),
            "---\ndescription: Rust zero-warning policy\napplyTo: \"**/*.rs\"\n---\nAlways use `#![forbid(unsafe_code)]`.\n",
        )
        .unwrap();

        // Write root apm.yml
        std::fs::write(
            dir.join("apm.yml"),
            r#"name: fw-workspace
version: 4.2.0
description: Unified Fw Agent Workspace
targets:
  - copilot
  - claude
dependencies:
  apm:
    - ./packages/code-review
    - microsoft/apm-sample-package#v1.0.0
  mcp:
    - name: fw-cli
      transport: stdio
      command: fw
      args:
        - mcp
"#,
        )
        .unwrap();

        // 1. Install
        let report = install_project(&dir, &InstallOptions::default()).expect("install");
        assert_eq!(report.packages_installed, 2);
        assert_eq!(report.mcp_servers_admitted, 1);
        assert_eq!(report.mcp_configs_written.len(), 2);
        assert!(dir.join(".vscode/mcp.json").exists());
        assert!(dir.join(".mcp.json").exists());
        assert!(dir.join(".agents/skills/code-review/SKILL.md").exists());

        // 2. Verify lockfile JSON, YAML view, Nix store path, SBOM, and gzip pack
        let lock = ApmLockfile::load_json(&dir.join("apm.lock.json")).expect("load lock");
        assert_eq!(lock.dependencies.len(), 2);
        assert!(lock.dependencies[0].nix_store_path.starts_with("/nix/store/"));
        assert!(lock.to_yaml().contains("lockfile_version: '1'"));
        let cdx = lock.export_sbom("fw-workspace", SbomFormat::CycloneDx);
        assert_eq!(cdx["bomFormat"], "CycloneDX");
        let spdx = lock.export_sbom("fw-workspace", SbomFormat::Spdx);
        assert_eq!(spdx["spdxVersion"], "SPDX-2.3");
        let gz = lock.pack_gzip_bundle().expect("pack gz");
        assert!(gz.starts_with(&[0x1f, 0x8b]));

        // 3. Frozen install succeeds when unchanged
        let frozen_ok = install_project(
            &dir,
            &InstallOptions {
                frozen: true,
                ..InstallOptions::default()
            },
        )
        .expect("frozen install");
        assert!(frozen_ok.frozen_verified);

        // 4. Compile AGENTS.md + .github/copilot-instructions.md + CLAUDE.md
        let (manifest, _) = installer::load_manifest(&dir).unwrap();
        let comp = compile_project(&dir, &manifest, None).expect("compile");
        assert_eq!(comp.instruction_files, 1);
        assert_eq!(comp.skill_files, 1);
        assert!(dir.join("AGENTS.md").exists());
        assert!(dir.join(".github/copilot-instructions.md").exists());
        assert!(dir.join("CLAUDE.md").exists());
        let agents_md = std::fs::read_to_string(dir.join("AGENTS.md")).unwrap();
        assert!(agents_md.contains("Rust zero-warning policy"));
        assert!(agents_md.contains("code-review"));

        // 5. List installed
        let listed = list_installed(&dir).expect("list");
        assert_eq!(listed.len(), 2);

        // 6. Audit passes cleanly before tampering
        let clean_audit = audit_project(&dir, &manifest, true);
        assert!(clean_audit.ok, "expected clean audit: {:?}", clean_audit.findings);

        // 7. Tamper with deployed file + inject hidden BiDi Unicode -> audit catches BOTH!
        std::fs::write(
            dir.join(".agents/skills/code-review/SKILL.md"),
            "---\nname: code-review\n---\nInjected \u{202E}hidden payload\n",
        )
        .unwrap();
        let dirty_audit = audit_project(&dir, &manifest, true);
        assert!(!dirty_audit.ok);
        assert!(
            dirty_audit
                .findings
                .iter()
                .any(|f| f.rule == "content-integrity" && f.severity == FindingSeverity::Critical)
        );
        assert!(
            dirty_audit
                .findings
                .iter()
                .any(|f| f.rule == "hidden-unicode" && f.severity == FindingSeverity::Critical)
        );

        // 8. Uninstall cleans deployed files and manifest
        let removed = uninstall_package(&dir, "code-review").expect("uninstall");
        assert!(removed);
        assert!(!dir.join(".agents/skills/code-review").exists());
        assert_eq!(list_installed(&dir).unwrap().len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hidden_unicode_scanner_and_frontmatter_and_cli_bridge() {
        let bad = scan_hidden_unicode("test.md", "ok\u{200B}invisible\u{E0041}tag");
        assert_eq!(bad.len(), 2);

        let doc = split_frontmatter("---\nname: demo\ndescription: hi\n---\nBody text");
        assert_eq!(doc.meta.get("name").map(String::as_str), Some("demo"));
        assert_eq!(doc.body, "Body text");

        let skill_md = synthesize_cli_skill_md(&fw_hermes_claw::default_cli_tools()[0]);
        assert!(skill_md.starts_with("---\nname: cli-rg-search\n"));

        let spec = PackageSpec::parse("gitlab.com/org/repo/skills/my-skill#main", None).unwrap();
        assert_eq!(spec.source, PackageSource::Git);
        assert_eq!(spec.host.as_deref(), Some("gitlab.com"));
        assert_eq!(spec.kind, PrimitiveKind::Skill);

        let help = run_cli(&[], Path::new(".")).unwrap();
        assert_eq!(help["name"], "fw-apm");
    }
}
