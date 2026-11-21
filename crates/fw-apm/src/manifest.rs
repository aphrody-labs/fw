use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// Supported AI coding agent harnesses (matching `microsoft/apm` v0.36.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TargetHarness {
    Copilot,
    Claude,
    Cursor,
    Codex,
    OpenCode,
    Gemini,
    Windsurf,
    Kiro,
    Grok,
}

impl TargetHarness {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "copilot" | "vscode" => Ok(Self::Copilot),
            "claude" | "claude-code" => Ok(Self::Claude),
            "cursor" => Ok(Self::Cursor),
            "codex" => Ok(Self::Codex),
            "opencode" => Ok(Self::OpenCode),
            "gemini" => Ok(Self::Gemini),
            "windsurf" => Ok(Self::Windsurf),
            "kiro" => Ok(Self::Kiro),
            "grok" => Ok(Self::Grok),
            other => Err(format!("unknown target harness: {other}")),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Copilot => "copilot",
            Self::Claude => "claude",
            Self::Cursor => "cursor",
            Self::Codex => "codex",
            Self::OpenCode => "opencode",
            Self::Gemini => "gemini",
            Self::Windsurf => "windsurf",
            Self::Kiro => "kiro",
            Self::Grok => "grok",
        }
    }

    /// Relative path where compiled instructions land for this harness.
    pub const fn instructions_path(self) -> &'static str {
        match self {
            Self::Copilot => ".github/copilot-instructions.md",
            Self::Claude => "CLAUDE.md",
            Self::Cursor => ".cursor/rules/apm.mdc",
            Self::Codex => "AGENTS.md",
            Self::OpenCode => "AGENTS.md",
            Self::Gemini => "GEMINI.md",
            Self::Windsurf => ".windsurfrules",
            Self::Kiro => ".kiro/steering/apm.md",
            Self::Grok => "AGENTS.md",
        }
    }

    /// Relative path where MCP server JSON configuration lands for this harness.
    pub const fn mcp_config_path(self) -> &'static str {
        match self {
            Self::Copilot => ".vscode/mcp.json",
            Self::Claude | Self::Codex | Self::OpenCode | Self::Grok => ".mcp.json",
            Self::Cursor => ".cursor/mcp.json",
            Self::Gemini => ".gemini/settings.json",
            Self::Windsurf => ".windsurf/mcp.json",
            Self::Kiro => ".kiro/settings/mcp.json",
        }
    }
}

/// MCP server transport protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum McpTransport {
    #[default]
    Stdio,
    Http,
    Sse,
}

impl McpTransport {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "stdio" => Ok(Self::Stdio),
            "http" | "streamable-http" => Ok(Self::Http),
            "sse" => Ok(Self::Sse),
            other => Err(format!("unsupported MCP transport: {other}")),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::Http => "http",
            Self::Sse => "sse",
        }
    }
}

/// MCP server declaration inside `apm.yml` (`dependencies.mcp`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpServerSpec {
    pub name: String,
    #[serde(default)]
    pub transport: McpTransport,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
}

/// Primitive classification for APM packages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrimitiveKind {
    ApmPackage,
    Skill,
    Plugin,
    Prompt,
    Instruction,
    Agent,
    Hook,
    Hybrid,
}

impl PrimitiveKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ApmPackage => "apm_package",
            Self::Skill => "skill",
            Self::Plugin => "plugin",
            Self::Prompt => "prompt",
            Self::Instruction => "instruction",
            Self::Agent => "agent",
            Self::Hook => "hook",
            Self::Hybrid => "hybrid",
        }
    }
}

/// Source kind for an APM package dependency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackageSource {
    Local,
    GitHub,
    Git,
    Marketplace,
}

impl PackageSource {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::GitHub => "github",
            Self::Git => "git",
            Self::Marketplace => "marketplace",
        }
    }
}

/// Parsed APM dependency specifier (local path, GitHub shorthand, git host URL, or marketplace).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageSpec {
    pub raw: String,
    pub name: String,
    pub source: PackageSource,
    pub host: Option<String>,
    pub repo: Option<String>,
    pub subpath: Option<String>,
    pub git_ref: Option<String>,
    pub local_path: Option<String>,
    pub marketplace: Option<String>,
    pub kind: PrimitiveKind,
}

impl PackageSpec {
    /// Parse an APM dependency string using the `microsoft/apm` grammar:
    /// - `./local/path` or `../local/path`
    /// - `pkg@marketplace`
    /// - `owner/repo[/subpath][#ref]`
    /// - `host.tld/owner/repo[/subpath][#ref]`
    pub fn parse(raw: &str, default_host: Option<&str>) -> Result<Self, String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err("empty dependency spec".into());
        }

        // 1. Local path
        if trimmed.starts_with("./")
            || trimmed.starts_with("../")
            || trimmed.starts_with('/')
            || trimmed.starts_with(".\\")
            || trimmed.starts_with("..\\")
        {
            let normalized = trimmed.replace('\\', "/");
            let name = normalized
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .filter(|s| !s.is_empty() && *s != "." && *s != "..")
                .unwrap_or("local-pkg")
                .to_string();
            return Ok(Self {
                raw: trimmed.to_string(),
                name,
                source: PackageSource::Local,
                host: None,
                repo: None,
                subpath: None,
                git_ref: None,
                local_path: Some(normalized),
                marketplace: None,
                kind: PrimitiveKind::ApmPackage,
            });
        }

        // 2. Marketplace (`package@marketplace[#ref]`)
        if let Some((pkg_part, market_and_ref)) = trimmed.split_once('@')
            && !pkg_part.contains('/')
            && !pkg_part.is_empty()
        {
            let (marketplace, git_ref) = split_ref(market_and_ref);
            return Ok(Self {
                raw: trimmed.to_string(),
                name: pkg_part.to_string(),
                source: PackageSource::Marketplace,
                host: None,
                repo: None,
                subpath: None,
                git_ref,
                local_path: None,
                marketplace: Some(marketplace.to_string()),
                kind: PrimitiveKind::Plugin,
            });
        }

        // 3. Git host or GitHub shorthand (`[host/]owner/repo[/subpath][#ref]`)
        let (path_part, git_ref) = split_ref(trimmed);
        let stripped = path_part
            .strip_prefix("https://")
            .or_else(|| path_part.strip_prefix("http://"))
            .unwrap_or(path_part);
        let segs: Vec<&str> = stripped.split('/').filter(|s| !s.is_empty()).collect();
        if segs.len() < 2 {
            return Err(format!("invalid APM dependency specifier: {trimmed}"));
        }

        let (host, owner, repo_name, rest) = if segs[0].contains('.') {
            if segs.len() < 3 {
                return Err(format!("missing owner/repo after host in: {trimmed}"));
            }
            (segs[0].to_string(), segs[1], segs[2], &segs[3..])
        } else {
            let h = default_host.unwrap_or("github.com").to_string();
            (h, segs[0], segs[1], &segs[2..])
        };

        let repo_clean = repo_name.strip_suffix(".git").unwrap_or(repo_name);
        let subpath = if rest.is_empty() {
            None
        } else {
            Some(rest.join("/"))
        };
        let kind = classify_subpath(subpath.as_deref());
        let name = rest
            .last()
            .copied()
            .unwrap_or(repo_clean)
            .trim_end_matches(".agent.md")
            .trim_end_matches(".prompt.md")
            .trim_end_matches(".instructions.md")
            .to_string();
        let source = if host.eq_ignore_ascii_case("github.com") {
            PackageSource::GitHub
        } else {
            PackageSource::Git
        };

        Ok(Self {
            raw: trimmed.to_string(),
            name,
            source,
            host: Some(host),
            repo: Some(format!("{owner}/{repo_clean}")),
            subpath,
            git_ref,
            local_path: None,
            marketplace: None,
            kind,
        })
    }

    pub fn canonical_repo_url(&self) -> String {
        match self.source {
            PackageSource::Local => format!("_local/{}", self.name),
            PackageSource::Marketplace => format!(
                "marketplace/{}/{}",
                self.marketplace.as_deref().unwrap_or("default"),
                self.name
            ),
            PackageSource::GitHub | PackageSource::Git => {
                let host = self.host.as_deref().unwrap_or("github.com");
                let repo = self.repo.as_deref().unwrap_or(&self.name);
                match &self.subpath {
                    Some(sub) => format!("{host}/{repo}/{sub}"),
                    None => format!("{host}/{repo}"),
                }
            }
        }
    }
}

fn split_ref(s: &str) -> (&str, Option<String>) {
    match s.split_once('#') {
        Some((head, r)) if !r.is_empty() => (head, Some(r.to_string())),
        Some((head, _)) => (head, None),
        None => (s, None),
    }
}

fn classify_subpath(subpath: Option<&str>) -> PrimitiveKind {
    let Some(sub) = subpath else {
        return PrimitiveKind::ApmPackage;
    };
    if sub.ends_with(".agent.md") || sub.starts_with("agents/") {
        PrimitiveKind::Agent
    } else if sub.ends_with(".prompt.md") || sub.starts_with("prompts/") {
        PrimitiveKind::Prompt
    } else if sub.ends_with(".instructions.md") || sub.starts_with("instructions/") {
        PrimitiveKind::Instruction
    } else if sub.starts_with("skills/") || sub.contains("/skills/") {
        PrimitiveKind::Skill
    } else if sub.starts_with("plugins/") || sub.contains("/plugins/") {
        PrimitiveKind::Plugin
    } else if sub.starts_with("hooks/") {
        PrimitiveKind::Hook
    } else {
        PrimitiveKind::ApmPackage
    }
}

/// Content-pinned executable approval policy (`executables.allow.<pkg>.<item> = sha256:...`).
/// Implements OpenAPM v0.1 `req-sc-016` (microsoft/apm commit `ee00d6aa`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutablesPolicy {
    /// Map of `<package_key> -> (<item_key> -> "sha256:<hex>" | "true")`.
    #[serde(default)]
    pub allow: BTreeMap<String, BTreeMap<String, String>>,
}

impl ExecutablesPolicy {
    /// Check if a package's MCP server definition fingerprint is explicitly pinned/approved.
    pub fn is_mcp_approved(&self, pkg_key: &str, server_name: &str, fingerprint: &str) -> bool {
        let Some(pkg_pins) = self.allow.get(pkg_key) else {
            return false;
        };
        if let Some(pin) = pkg_pins.get(server_name) {
            return pin == fingerprint;
        }
        if let Some(pin) = pkg_pins.get("mcp") {
            return pin == fingerprint || pin.eq_ignore_ascii_case("true");
        }
        false
    }

    pub fn pin_item(&mut self, pkg_key: &str, item_key: &str, fingerprint: &str) {
        self.allow
            .entry(pkg_key.to_string())
            .or_default()
            .insert(item_key.to_string(), fingerprint.to_string());
    }
}

/// Dependencies section of `apm.yml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dependencies {
    #[serde(default)]
    pub apm: Vec<String>,
    #[serde(default)]
    pub mcp: Vec<McpServerSpec>,
}

/// Complete `apm.yml` manifest model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApmManifest {
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub includes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_host: Option<String>,
    #[serde(default)]
    pub targets: Vec<TargetHarness>,
    #[serde(default)]
    pub dependencies: Dependencies,
    #[serde(default)]
    pub scripts: BTreeMap<String, String>,
    #[serde(default)]
    pub executables: ExecutablesPolicy,
}

impl ApmManifest {
    pub fn minimal(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: "0.1.0".into(),
            description: Some("Agent Package Manager manifest managed by fw-apm".into()),
            author: None,
            license: Some("MIT".into()),
            includes: Some("auto".into()),
            default_host: None,
            targets: vec![TargetHarness::Copilot, TargetHarness::Claude],
            dependencies: Dependencies::default(),
            scripts: BTreeMap::new(),
            executables: ExecutablesPolicy::default(),
        }
    }

    pub fn parsed_apm_deps(&self) -> Result<Vec<PackageSpec>, String> {
        let mut out = Vec::with_capacity(self.dependencies.apm.len());
        for raw in &self.dependencies.apm {
            out.push(PackageSpec::parse(raw, self.default_host.as_deref())?);
        }
        Ok(out)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TopSection {
    Root,
    Targets,
    Dependencies,
    Scripts,
    Executables,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DepSubSection {
    None,
    Apm,
    Mcp,
}

/// Strip unquoted `# ...` trailing YAML comments and trim outer whitespace.
fn strip_yaml_comment(line: &str) -> &str {
    let mut in_single = false;
    let mut in_double = false;
    let bytes = line.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'\'' if !in_double => in_single = !in_single,
            b'"' if !in_single => in_double = !in_double,
            b'#' if !in_single && !in_double && (i == 0 || bytes[i - 1].is_ascii_whitespace()) => {
                return line[..i].trim_end();
            }
            _ => {}
        }
    }
    line.trim_end()
}

fn unquote(raw: &str) -> String {
    let t = raw.trim();
    if t.len() >= 2
        && ((t.starts_with('"') && t.ends_with('"')) || (t.starts_with('\'') && t.ends_with('\'')))
    {
        t[1..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}

fn parse_inline_list(raw: &str) -> Vec<String> {
    let inner = raw.trim();
    let Some(body) = inner.strip_prefix('[').and_then(|s| s.strip_suffix(']')) else {
        return Vec::new();
    };
    body.split(',')
        .map(unquote)
        .filter(|s| !s.is_empty())
        .collect()
}

/// Pure-Rust zero-dependency parser for `apm.yml` manifests.
pub fn parse_apm_yml(input: &str) -> Result<ApmManifest, String> {
    let mut manifest = ApmManifest {
        name: String::new(),
        version: "0.1.0".into(),
        description: None,
        author: None,
        license: None,
        includes: None,
        default_host: None,
        targets: Vec::new(),
        dependencies: Dependencies::default(),
        scripts: BTreeMap::new(),
        executables: ExecutablesPolicy::default(),
    };

    let mut section = TopSection::Root;
    let mut dep_sub = DepSubSection::None;
    let mut current_mcp: Option<McpServerSpec> = None;
    let mut in_mcp_args = false;
    let mut in_mcp_env = false;
    let mut in_exec_allow = false;
    let mut current_exec_pkg: Option<String> = None;

    for (idx, raw_line) in input.lines().enumerate() {
        let lineno = idx + 1;
        let clean = strip_yaml_comment(raw_line);
        if clean.trim().is_empty() {
            continue;
        }
        let indent = clean.bytes().take_while(|&b| b == b' ').count();
        let trimmed = clean.trim();

        // Top-level key (indent == 0)
        if indent == 0 {
            if let Some(mcp) = current_mcp.take() {
                manifest.dependencies.mcp.push(mcp);
            }
            in_mcp_args = false;
            in_mcp_env = false;
            dep_sub = DepSubSection::None;
            in_exec_allow = false;
            current_exec_pkg = None;

            let Some((k, v)) = trimmed.split_once(':') else {
                return Err(format!("line {lineno}: expected `key: value`, got `{trimmed}`"));
            };
            let key = k.trim();
            let val = v.trim();
            match key {
                "name" => {
                    manifest.name = unquote(val);
                    section = TopSection::Root;
                }
                "version" => {
                    manifest.version = unquote(val);
                    section = TopSection::Root;
                }
                "description" => {
                    manifest.description = Some(unquote(val));
                    section = TopSection::Root;
                }
                "author" => {
                    manifest.author = Some(unquote(val));
                    section = TopSection::Root;
                }
                "license" => {
                    manifest.license = Some(unquote(val));
                    section = TopSection::Root;
                }
                "includes" => {
                    manifest.includes = Some(unquote(val));
                    section = TopSection::Root;
                }
                "default_host" => {
                    manifest.default_host = Some(unquote(val));
                    section = TopSection::Root;
                }
                "targets" => {
                    section = TopSection::Targets;
                    if val.starts_with('[') {
                        for item in parse_inline_list(val) {
                            manifest.targets.push(TargetHarness::parse(&item)?);
                        }
                    }
                }
                "dependencies" => {
                    section = TopSection::Dependencies;
                }
                "scripts" => {
                    section = TopSection::Scripts;
                }
                "executables" => {
                    section = TopSection::Executables;
                }
                _ => {
                    section = TopSection::Root;
                }
            }
            continue;
        }

        // Indented lines inside sections
        match section {
            TopSection::Targets => {
                if let Some(item) = trimmed.strip_prefix('-') {
                    manifest.targets.push(TargetHarness::parse(&unquote(item))?);
                }
            }
            TopSection::Scripts => {
                if let Some((k, v)) = trimmed.split_once(':') {
                    manifest.scripts.insert(unquote(k), unquote(v));
                }
            }
            TopSection::Executables => {
                if indent == 2 && trimmed.starts_with("allow:") {
                    in_exec_allow = true;
                    current_exec_pkg = None;
                } else if in_exec_allow && indent == 4 && trimmed.ends_with(':') {
                    current_exec_pkg = Some(unquote(trimmed.trim_end_matches(':')));
                } else if in_exec_allow
                    && indent >= 6
                    && let Some(pkg) = current_exec_pkg.as_deref()
                    && let Some((k, v)) = trimmed.split_once(':')
                {
                    manifest.executables.pin_item(pkg, &unquote(k), &unquote(v));
                }
            }
            TopSection::Dependencies => {
                if indent == 2 && !trimmed.starts_with('-') {
                    if let Some(mcp) = current_mcp.take() {
                        manifest.dependencies.mcp.push(mcp);
                    }
                    in_mcp_args = false;
                    in_mcp_env = false;
                    if let Some((k, v)) = trimmed.split_once(':') {
                        let key = k.trim();
                        let val = v.trim();
                        match key {
                            "apm" => {
                                dep_sub = DepSubSection::Apm;
                                if val.starts_with('[') {
                                    manifest.dependencies.apm.extend(parse_inline_list(val));
                                }
                            }
                            "mcp" => {
                                dep_sub = DepSubSection::Mcp;
                                if val.starts_with('[') {
                                    for name in parse_inline_list(val) {
                                        manifest.dependencies.mcp.push(McpServerSpec {
                                            name,
                                            transport: McpTransport::Stdio,
                                            command: None,
                                            args: Vec::new(),
                                            url: None,
                                            env: BTreeMap::new(),
                                        });
                                    }
                                }
                            }
                            _ => dep_sub = DepSubSection::None,
                        }
                    }
                    continue;
                }

                match dep_sub {
                    DepSubSection::Apm => {
                        if let Some(item) = trimmed.strip_prefix('-') {
                            let v = unquote(item);
                            if !v.is_empty() {
                                manifest.dependencies.apm.push(v);
                            }
                        }
                    }
                    DepSubSection::Mcp => {
                        if indent <= 4 && trimmed.starts_with('-') {
                            if let Some(mcp) = current_mcp.take() {
                                manifest.dependencies.mcp.push(mcp);
                            }
                            in_mcp_args = false;
                            in_mcp_env = false;
                            let rest = trimmed.trim_start_matches('-').trim();
                            if let Some((k, v)) = rest.split_once(':') {
                                let mut spec = McpServerSpec {
                                    name: String::new(),
                                    transport: McpTransport::Stdio,
                                    command: None,
                                    args: Vec::new(),
                                    url: None,
                                    env: BTreeMap::new(),
                                };
                                apply_mcp_field(&mut spec, k.trim(), v.trim())?;
                                current_mcp = Some(spec);
                            } else if !rest.is_empty() {
                                manifest.dependencies.mcp.push(McpServerSpec {
                                    name: unquote(rest),
                                    transport: McpTransport::Stdio,
                                    command: None,
                                    args: Vec::new(),
                                    url: None,
                                    env: BTreeMap::new(),
                                });
                            }
                        } else if let Some(spec) = current_mcp.as_mut() {
                            if in_mcp_args && trimmed.starts_with('-') {
                                spec.args.push(unquote(trimmed.trim_start_matches('-')));
                            } else if in_mcp_env && indent >= 8 && trimmed.contains(':') {
                                if let Some((ek, ev)) = trimmed.split_once(':') {
                                    spec.env.insert(unquote(ek), unquote(ev));
                                }
                            } else if let Some((k, v)) = trimmed.split_once(':') {
                                let key = k.trim();
                                let val = v.trim();
                                in_mcp_args = key == "args" && val.is_empty();
                                in_mcp_env = key == "env" && val.is_empty();
                                apply_mcp_field(spec, key, val)?;
                            }
                        }
                    }
                    DepSubSection::None => {}
                }
            }
            TopSection::Root => {}
        }
    }

    if let Some(mcp) = current_mcp.take() {
        manifest.dependencies.mcp.push(mcp);
    }

    if manifest.name.is_empty() {
        return Err("apm.yml is missing required `name` field".into());
    }

    Ok(manifest)
}

fn apply_mcp_field(spec: &mut McpServerSpec, key: &str, val: &str) -> Result<(), String> {
    match key {
        "name" => spec.name = unquote(val),
        "transport" => spec.transport = McpTransport::parse(&unquote(val))?,
        "command" => spec.command = Some(unquote(val)),
        "url" => spec.url = Some(unquote(val)),
        "args" if val.starts_with('[') => spec.args = parse_inline_list(val),
        _ => {}
    }
    Ok(())
}

/// Deterministic pure-Rust `apm.yml` serializer.
pub fn serialize_apm_yml(m: &ApmManifest) -> String {
    let mut out = String::with_capacity(512);
    let _ = writeln!(out, "name: {}", m.name);
    let _ = writeln!(out, "version: {}", m.version);
    if let Some(desc) = &m.description {
        let _ = writeln!(out, "description: {desc}");
    }
    if let Some(inc) = &m.includes {
        let _ = writeln!(out, "includes: {inc}");
    }
    if let Some(author) = &m.author {
        let _ = writeln!(out, "author: {author}");
    }
    if let Some(lic) = &m.license {
        let _ = writeln!(out, "license: {lic}");
    }
    if let Some(host) = &m.default_host {
        let _ = writeln!(out, "default_host: {host}");
    }

    if !m.targets.is_empty() {
        out.push_str("\ntargets:\n");
        for t in &m.targets {
            let _ = writeln!(out, "  - {}", t.as_str());
        }
    }

    out.push_str("\ndependencies:\n");
    if m.dependencies.apm.is_empty() {
        out.push_str("  apm: []\n");
    } else {
        out.push_str("  apm:\n");
        for dep in &m.dependencies.apm {
            let _ = writeln!(out, "    - {dep}");
        }
    }

    if m.dependencies.mcp.is_empty() {
        out.push_str("  mcp: []\n");
    } else {
        out.push_str("  mcp:\n");
        for mcp in &m.dependencies.mcp {
            let _ = writeln!(out, "    - name: {}", mcp.name);
            let _ = writeln!(out, "      transport: {}", mcp.transport.as_str());
            if let Some(cmd) = &mcp.command {
                let _ = writeln!(out, "      command: {cmd}");
            }
            if !mcp.args.is_empty() {
                out.push_str("      args:\n");
                for a in &mcp.args {
                    let _ = writeln!(out, "        - {a}");
                }
            }
            if let Some(url) = &mcp.url {
                let _ = writeln!(out, "      url: {url}");
            }
            if !mcp.env.is_empty() {
                out.push_str("      env:\n");
                for (ek, ev) in &mcp.env {
                    let _ = writeln!(out, "        {ek}: {ev}");
                }
            }
        }
    }

    if m.scripts.is_empty() {
        out.push_str("\nscripts: {}\n");
    } else {
        out.push_str("\nscripts:\n");
        for (k, v) in &m.scripts {
            let _ = writeln!(out, "  {k}: \"{v}\"");
        }
    }

    if !m.executables.allow.is_empty() {
        out.push_str("\nexecutables:\n  allow:\n");
        for (pkg, pins) in &m.executables.allow {
            let _ = writeln!(out, "    {pkg}:");
            for (item, fp) in pins {
                let _ = writeln!(out, "      {item}: {fp}");
            }
        }
    }

    out
}
