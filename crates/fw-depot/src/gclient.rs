//! gclient's dependency graph: the `.gclient` file, the DEPS file of every solution and of every recursed dependency, and what gclient derives
//! from them (`revinfo` lines, hooks, GN args). Network and disk work is in `sync`; everything here is pure.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::deps::{Deps, Env, Platform, Value};
use crate::error::{Error, Result, fs};

/// The CIPD service gclient resolves package URLs against.
pub const CIPD_SERVICE: &str = "https://chrome-infra-packages.appspot.com";

/// One entry of the `solutions` list of a `.gclient` file.
#[derive(Debug, Clone)]
pub struct Solution {
    pub name: String,
    pub url: Option<String>,
    pub deps_file: String,
    pub managed: bool,
    /// `None` removes the dependency; a url replaces it.
    pub custom_deps: BTreeMap<String, Option<String>>,
    pub custom_vars: Vec<(String, Value)>,
    pub custom_hooks: Vec<Value>,
}

/// A `.gclient` file.
#[derive(Debug, Clone)]
pub struct Config {
    pub root: PathBuf,
    pub solutions: Vec<Solution>,
    pub target_os: Vec<String>,
    pub target_os_only: bool,
    pub target_cpu: Vec<String>,
    pub target_cpu_only: bool,
    pub cache_dir: Option<String>,
}

fn strings(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::List(items)) => items
            .iter()
            .filter_map(|item| item.text().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    }
}

impl Config {
    pub fn parse(text: &str, root: &Path) -> Result<Config> {
        let parsed = Deps::parse(text)?;
        let mut other = parsed.other;
        for (name, value) in parsed.vars {
            other.insert(name, value);
        }
        let Some(Value::List(entries)) = other.get("solutions") else {
            return Err(Error::message(".gclient has no `solutions` list"));
        };
        let mut solutions = Vec::new();
        for entry in entries {
            let name = entry
                .get("name")
                .and_then(Value::text)
                .ok_or_else(|| Error::message("a solution has no name"))?;
            let custom_deps = match entry.get("custom_deps") {
                Some(Value::Dict(items)) => items
                    .iter()
                    .map(|(path, value)| (path.clone(), value.text().map(str::to_owned)))
                    .collect(),
                _ => BTreeMap::new(),
            };
            solutions.push(Solution {
                name: name.to_owned(),
                url: entry.get("url").and_then(Value::text).map(str::to_owned),
                deps_file: entry
                    .get("deps_file")
                    .and_then(Value::text)
                    .unwrap_or("DEPS")
                    .to_owned(),
                managed: entry.get("managed").is_none_or(Value::truthy),
                custom_deps,
                custom_vars: match entry.get("custom_vars") {
                    Some(Value::Dict(items)) => items.clone(),
                    _ => Vec::new(),
                },
                custom_hooks: match entry.get("custom_hooks") {
                    Some(Value::List(items)) => items.clone(),
                    _ => Vec::new(),
                },
            });
        }
        Ok(Config {
            root: root.to_owned(),
            solutions,
            target_os: strings(other.get("target_os")),
            target_os_only: other.get("target_os_only").is_some_and(Value::truthy),
            target_cpu: strings(other.get("target_cpu")),
            target_cpu_only: other.get("target_cpu_only").is_some_and(Value::truthy),
            cache_dir: other
                .get("cache_dir")
                .and_then(Value::text)
                .map(str::to_owned),
        })
    }

    /// The `.gclient` of `start` or of its nearest ancestor that has one.
    pub fn find(start: &Path) -> Result<Config> {
        let start = std::path::absolute(start).map_err(|source| Error::io(start, source))?;
        for directory in start.ancestors() {
            let file = directory.join(".gclient");
            if file.is_file() {
                return Config::parse(&fs::read_to_string(&file)?, directory);
            }
        }
        Err(Error::message(
            "client not configured: no .gclient in this directory or above",
        ))
    }

    /// The platform gclient builds for: the host joined by `target_os` and `target_cpu`, unless the file says `_only`.
    pub fn platform(&self, host: &Platform) -> Platform {
        fn union(host: &str, listed: &[String], only: bool) -> Vec<String> {
            let mut out: Vec<String> = if only { Vec::new() } else { vec![host.to_owned()] };
            for name in listed {
                if !out.contains(name) {
                    out.push(name.clone());
                }
            }
            out
        }
        Platform {
            host_os: host.host_os.clone(),
            host_cpu: host.host_cpu.clone(),
            target_os: union(&host.host_os, &self.target_os, self.target_os_only),
            target_cpu: union(&host.host_cpu, &self.target_cpu, self.target_cpu_only),
        }
    }
}

/// A package, an object or a repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Git,
    Cipd {
        package: String,
        version: String,
    },
    Gcs {
        bucket: String,
        object_name: String,
        sha256sum: String,
        size_bytes: u64,
        output_file: Option<String>,
    },
}

/// A node of the graph: a solution or a dependency.
#[derive(Debug, Clone)]
pub struct Dep {
    /// gclient's name: the path, `path:package` for CIPD, `path:object` for GCS.
    pub name: String,
    /// The directory, relative to the root of the checkout.
    pub path: String,
    /// `url@revision` for Git, the package URL for CIPD, `gs://bucket/object` for GCS. `None` for a dependency a `custom_deps` removed.
    pub url: Option<String>,
    pub kind: Kind,
    pub condition: Option<String>,
    pub parent: Option<usize>,
    pub deps_file: String,
    pub recurse: bool,
    pub is_solution: bool,
    pub expanded: bool,
    relative: bool,
    use_relative_paths: bool,
    solution: usize,
    env: Option<Env>,
    pub recursedeps: BTreeMap<String, String>,
    pub gn_args_file: Option<String>,
    pub gn_args: Vec<String>,
    pub gn_args_from: Option<String>,
}

impl Dep {
    /// `(url, revision)` of a Git dependency.
    pub fn git(&self) -> Option<(&str, Option<&str>)> {
        if self.kind != Kind::Git {
            return None;
        }
        let url = self.url.as_deref()?;
        Some(match url.rsplit_once('@') {
            Some((base, revision)) if !revision.contains('/') && !revision.contains(':') => {
                (base, Some(revision))
            }
            _ => (url, None),
        })
    }
}

/// A hook with its working directory resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookEntry {
    pub owner: String,
    pub name: Option<String>,
    pub action: Vec<String>,
    pub cwd: PathBuf,
    pub condition: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Graph {
    pub config: Config,
    pub platform: Platform,
    pub deps: Vec<Dep>,
    /// `custom_hooks` and the hooks of each dependency's DEPS file, by owner.
    hooks: Vec<(usize, Vec<HookEntry>)>,
    pre_hooks: Vec<(usize, Vec<HookEntry>)>,
}

fn dirname(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(parent, _)| parent)
}

/// `a/b/../c` becomes `a/c`.
fn normalize(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

fn join(prefix: &str, path: &str) -> String {
    if prefix.is_empty() {
        normalize(path)
    } else {
        normalize(&format!("{prefix}/{path}"))
    }
}

impl Graph {
    /// A graph of the solutions of `config`, not yet expanded.
    pub fn new(config: Config, host: &Platform) -> Graph {
        let platform = config.platform(host);
        let deps = config
            .solutions
            .iter()
            .enumerate()
            .map(|(index, solution)| Dep {
                name: solution.name.clone(),
                path: solution.name.clone(),
                url: solution.url.as_ref().map(|url| url.replace("/@", "@")),
                kind: Kind::Git,
                condition: None,
                parent: None,
                deps_file: solution.deps_file.clone(),
                recurse: true,
                is_solution: true,
                expanded: false,
                relative: false,
                use_relative_paths: false,
                solution: index,
                env: None,
                recursedeps: BTreeMap::new(),
                gn_args_file: None,
                gn_args: Vec::new(),
                gn_args_from: None,
            })
            .collect();
        Graph {
            config,
            platform,
            deps,
            hooks: Vec::new(),
            pre_hooks: Vec::new(),
        }
    }

    /// Reads the DEPS file of every dependency that has one on disk, recursively. A dependency that is not checked out is a leaf.
    pub fn load_local(&mut self) -> Result<()> {
        let mut next = 0;
        while next < self.deps.len() {
            let index = next;
            next += 1;
            if !self.deps[index].recurse {
                continue;
            }
            let file = self
                .config
                .root
                .join(&self.deps[index].path)
                .join(&self.deps[index].deps_file);
            if file.is_file() {
                let text = fs::read_to_string(&file)?;
                self.expand(index, &text)?;
            }
        }
        Ok(())
    }

    fn lookup_custom(&self, solution: usize, name: &str, url: Option<String>) -> Option<String> {
        match self.config.solutions[solution].custom_deps.get(name) {
            Some(replacement) => replacement.clone(),
            None => url,
        }
    }

    /// Parses the DEPS text of dependency `index`, adds the dependencies and hooks it declares, and returns the new indexes.
    pub fn expand(&mut self, index: usize, text: &str) -> Result<Vec<usize>> {
        let deps = Deps::parse(text)?;
        let solution = self.deps[index].solution;
        let custom_vars = self.config.solutions[solution].custom_vars.clone();
        let parent_env = match self.deps[index].parent {
            Some(parent) => self.deps[parent].env.clone(),
            None => None,
        };
        let env = Env::build(&deps, &self.platform, parent_env.as_ref(), &custom_vars)?;

        let path = self.deps[index].path.clone();
        let use_relative = deps.other.get("use_relative_paths").is_some_and(Value::truthy);
        let prefix = if use_relative {
            path.clone()
        } else if self.deps[index].relative {
            dirname(&path).to_owned()
        } else {
            String::new()
        };
        let own_file = self.deps[index].deps_file.clone();
        let mut recursedeps = BTreeMap::new();
        for entry in &deps.recursedeps {
            let file = deps
                .recurse_files
                .get(entry)
                .cloned()
                .unwrap_or_else(|| own_file.clone());
            let key = if prefix.is_empty() {
                entry.clone()
            } else {
                join(&prefix, entry)
            };
            recursedeps.insert(key, file);
        }

        let mut entries: Vec<(String, Value)> = deps.deps.clone();
        let own_custom = if self.deps[index].is_solution {
            self.config.solutions[solution].custom_deps.clone()
        } else {
            BTreeMap::new()
        };
        for (name, url) in &own_custom {
            if let Some(url) = url
                && !url.ends_with("@unmanaged")
                && !entries.iter().any(|(key, _)| key == name)
            {
                entries.push((
                    name.clone(),
                    Value::Dict(vec![
                        ("url".to_owned(), Value::Str(url.clone())),
                        ("dep_type".to_owned(), Value::Str("git".to_owned())),
                    ]),
                ));
            }
        }

        let first_new = self.deps.len();
        for (raw_key, value) in &entries {
            if matches!(value, Value::None) {
                continue;
            }
            let key = env.format(raw_key)?;
            let key = if prefix.is_empty() {
                key
            } else {
                join(&prefix, &key)
            };
            let condition = value
                .get("condition")
                .and_then(Value::text)
                .map(str::to_owned);
            let active = match &condition {
                Some(condition) => env.condition(&env.format(condition)?)?,
                None => true,
            };
            let kind = value.get("dep_type").and_then(Value::text).unwrap_or("git");
            let fail = |what: &str| Error::message(format!("dependency {key}: {what}"));
            match (kind, value) {
                ("cipd", Value::Dict(_)) => {
                    let Some(Value::List(packages)) = value.get("packages") else {
                        return Err(fail("a cipd dependency needs packages"));
                    };
                    for package in packages {
                        let name = package
                            .get("package")
                            .and_then(Value::text)
                            .ok_or_else(|| fail("a package has no name"))?;
                        let package_name = env.format(name)?;
                        let version = match (
                            package.get("version").and_then(Value::text),
                            package.get("version_file").and_then(Value::text),
                        ) {
                            (Some(version), _) => env.format(version)?,
                            (None, Some(file)) => {
                                let file_path = self.config.root.join(&path).join(file);
                                match std::fs::read_to_string(&file_path) {
                                    Ok(content) => content.trim().to_owned(),
                                    Err(_) if !active => format!("file:{file}"),
                                    Err(error) => {
                                        return Err(Error::io(&file_path, error));
                                    }
                                }
                            }
                            (None, None) => {
                                return Err(fail("a package has neither version nor version_file"));
                            }
                        };
                        let url = format!("{CIPD_SERVICE}/{package_name}@{version}");
                        self.push(
                            index,
                            Dep {
                                name: format!("{key}:{package_name}"),
                                path: key.clone(),
                                url: Some(url),
                                kind: Kind::Cipd {
                                    package: package_name,
                                    version,
                                },
                                condition: condition.clone(),
                                ..self.child(index, active, false, &own_file, solution, use_relative)
                            },
                            active,
                        )?;
                    }
                }
                ("gcs", Value::Dict(_)) => {
                    let bucket = value
                        .get("bucket")
                        .and_then(Value::text)
                        .ok_or_else(|| fail("a gcs dependency needs a bucket"))?;
                    let bucket = env.format(bucket)?;
                    let Some(Value::List(objects)) = value.get("objects") else {
                        continue;
                    };
                    for object in objects {
                        let object_name = object
                            .get("object_name")
                            .and_then(Value::text)
                            .ok_or_else(|| fail("an object has no object_name"))?;
                        let object_name = env.format(object_name)?;
                        let merged = match (
                            condition.as_deref(),
                            object.get("condition").and_then(Value::text),
                        ) {
                            (Some(first), Some(second)) => Some(format!("({first}) and ({second})")),
                            (Some(only), None) | (None, Some(only)) => Some(only.to_owned()),
                            (None, None) => None,
                        };
                        let object_active = match &merged {
                            Some(condition) => active && env.condition(&env.format(condition)?)?,
                            None => active,
                        };
                        let size_bytes = match object.get("size_bytes") {
                            Some(Value::Int(size)) => u64::try_from(*size).unwrap_or(0),
                            _ => 0,
                        };
                        self.push(
                            index,
                            Dep {
                                name: format!("{key}:{object_name}"),
                                path: key.clone(),
                                url: Some(format!("gs://{bucket}/{object_name}")),
                                kind: Kind::Gcs {
                                    bucket: bucket.clone(),
                                    object_name,
                                    sha256sum: object
                                        .get("sha256sum")
                                        .and_then(Value::text)
                                        .unwrap_or_default()
                                        .to_owned(),
                                    size_bytes,
                                    output_file: object
                                        .get("output_file")
                                        .and_then(Value::text)
                                        .map(str::to_owned),
                                },
                                condition: merged,
                                ..self.child(index, object_active, false, &own_file, solution, use_relative)
                            },
                            object_active,
                        )?;
                    }
                }
                ("git", _) => {
                    let raw = match value {
                        Value::Str(raw) => Some(raw.as_str()),
                        Value::Dict(_) => value.get("url").and_then(Value::text),
                        _ => return Err(fail("unknown shape")),
                    };
                    let url = match raw {
                        Some(raw) => Some(env.format(raw)?.replace("/@", "@")),
                        None => None,
                    };
                    let url = self.lookup_custom(solution, &key, url);
                    let url = url.map(|url| self.absolute_url(index, &url)).transpose()?;
                    let recurse = recursedeps.contains_key(&key);
                    let file = recursedeps.get(&key).cloned().unwrap_or_else(|| own_file.clone());
                    self.push(
                        index,
                        Dep {
                            name: key.clone(),
                            path: key.clone(),
                            url,
                            kind: Kind::Git,
                            condition,
                            ..self.child(index, active, recurse, &file, solution, use_relative)
                        },
                        active,
                    )?;
                }
                _ => return Err(fail("unknown dep_type or shape")),
            }
        }

        let hooks_base = if use_relative {
            self.config.root.join(&path)
        } else if self.deps[index].relative {
            self.config.root.join(dirname(&path))
        } else {
            self.config.root.clone()
        };
        let own_hooks = self.collect_hooks(solution, &path, &deps.hooks, &env, &hooks_base)?;
        let pre = match deps.other.get("pre_deps_hooks") {
            Some(Value::List(items)) if self.deps.len() > first_new => {
                self.collect_hooks(solution, &path, items, &env, &hooks_base)?
            }
            _ => Vec::new(),
        };
        self.hooks.push((index, own_hooks));
        self.pre_hooks.push((index, pre));

        let dep = &mut self.deps[index];
        dep.env = Some(env);
        dep.use_relative_paths = use_relative;
        dep.recursedeps = recursedeps;
        dep.expanded = true;
        dep.gn_args_file = deps
            .other
            .get("gclient_gn_args_file")
            .and_then(Value::text)
            .map(str::to_owned);
        dep.gn_args = strings(deps.other.get("gclient_gn_args"));
        dep.gn_args_from = deps
            .other
            .get("gclient_gn_args_from")
            .and_then(Value::text)
            .map(str::to_owned);
        Ok((first_new..self.deps.len()).collect())
    }

    fn child(
        &self,
        parent: usize,
        active: bool,
        recurse: bool,
        deps_file: &str,
        solution: usize,
        relative: bool,
    ) -> Dep {
        Dep {
            name: String::new(),
            path: String::new(),
            url: None,
            kind: Kind::Git,
            condition: None,
            parent: Some(parent),
            deps_file: deps_file.to_owned(),
            recurse: recurse && active,
            is_solution: false,
            expanded: false,
            relative,
            use_relative_paths: false,
            solution,
            env: None,
            recursedeps: BTreeMap::new(),
            gn_args_file: None,
            gn_args: Vec::new(),
            gn_args_from: None,
        }
    }

    fn absolute_url(&self, index: usize, url: &str) -> Result<String> {
        if url.contains("://") || url.starts_with("git@") || !url.starts_with('/') {
            return Ok(url.to_owned());
        }
        let parent = self.deps[index].url.as_deref().unwrap_or_default();
        let base = parent.rsplit_once('@').map_or(parent, |(base, _)| base);
        let cut = base
            .rfind('/')
            .ok_or_else(|| Error::message(format!("relative DEPS entry '{url}' has no parent url")))?;
        Ok(format!("{}{url}", &base[..cut]))
    }

    /// Adds a dependency that is processed; an entry that a condition turned off is not part of the graph, as in gclient's `subtree(False)`.
    fn push(&mut self, parent: usize, dep: Dep, active: bool) -> Result<()> {
        if !active {
            return Ok(());
        }
        if let Some(existing) = self.deps.iter().find(|other| other.name == dep.name) {
            if existing.url != dep.url && existing.url.is_some() && dep.url.is_some() {
                return Err(Error::message(format!(
                    "Dependency {} specified more than once:\n  {} [{}]\nvs\n  {} [{}]",
                    dep.name,
                    self.hierarchy(existing),
                    existing.url.as_deref().unwrap_or_default(),
                    self.hierarchy(&Dep { parent: Some(parent), ..dep.clone() }),
                    dep.url.as_deref().unwrap_or_default(),
                )));
            }
            return Ok(());
        }
        self.deps.push(dep);
        Ok(())
    }

    fn hierarchy(&self, dep: &Dep) -> String {
        let mut names = vec![format!("{}({})", dep.name, dep.url.as_deref().unwrap_or_default())];
        let mut cursor = dep.parent;
        while let Some(parent) = cursor {
            let node = &self.deps[parent];
            names.push(format!("{}({})", node.name, node.url.as_deref().unwrap_or_default()));
            cursor = node.parent;
        }
        names.reverse();
        names.join(" -> ")
    }

    fn collect_hooks(
        &self,
        solution: usize,
        owner: &str,
        hooks: &[Value],
        env: &Env,
        base: &Path,
    ) -> Result<Vec<HookEntry>> {
        let custom = &self.config.solutions[solution].custom_hooks;
        let suppressed: Vec<&str> = custom
            .iter()
            .map(|hook| hook.get("name").and_then(Value::text).unwrap_or_default())
            .collect();
        let mut out = Vec::new();
        for hook in hooks {
            let name = hook.get("name").and_then(Value::text).unwrap_or_default();
            if suppressed.contains(&name) {
                continue;
            }
            out.push(self.hook(owner, hook, env, base)?);
        }
        Ok(out)
    }

    fn hook(&self, owner: &str, hook: &Value, env: &Env, base: &Path) -> Result<HookEntry> {
        let action = match hook.get("action") {
            Some(Value::List(items)) => items
                .iter()
                .filter_map(Value::text)
                .map(|text| env.format(text))
                .collect::<std::result::Result<Vec<_>, _>>()?,
            _ => Vec::new(),
        };
        let condition = match hook.get("condition").and_then(Value::text) {
            Some(text) => Some(env.format(text)?),
            None => None,
        };
        let cwd = match hook.get("cwd").and_then(Value::text) {
            Some(extra) => base.join(extra),
            None => base.to_owned(),
        };
        Ok(HookEntry {
            owner: owner.to_owned(),
            name: hook.get("name").and_then(Value::text).map(str::to_owned),
            action,
            cwd,
            condition,
        })
    }

    /// Whether a hook's condition holds in the variables of the dependency that declared it.
    pub fn hook_applies(&self, hook: &HookEntry) -> Result<bool> {
        let Some(condition) = &hook.condition else {
            return Ok(true);
        };
        let env = self
            .deps
            .iter()
            .find(|dep| dep.path == hook.owner && dep.expanded && dep.kind == Kind::Git)
            .and_then(|dep| dep.env.as_ref())
            .ok_or_else(|| Error::message(format!("hook of {}: no variables", hook.owner)))?;
        Ok(env.condition(condition)?)
    }

    /// The hooks to run after a sync, in gclient's order: a dependency's own hooks, then those of its dependencies in turn.
    pub fn hooks(&self) -> Result<Vec<HookEntry>> {
        let mut out = Vec::new();
        for (index, dep) in self.deps.iter().enumerate() {
            if dep.parent.is_none() {
                self.walk_hooks(index, &mut out)?;
            }
        }
        Ok(out)
    }

    fn walk_hooks(&self, index: usize, out: &mut Vec<HookEntry>) -> Result<()> {
        let dep = &self.deps[index];
        if !dep.recurse {
            return Ok(());
        }
        let solution = &self.config.solutions[dep.solution];
        if let Some((_, hooks)) = self.hooks.iter().find(|(owner, _)| *owner == index) {
            out.extend(hooks.iter().cloned());
        }
        if dep.is_solution {
            let base = &self.config.root;
            let env = dep.env.as_ref();
            if let Some(env) = env {
                for hook in &solution.custom_hooks {
                    if hook.get("action").is_some() {
                        out.push(self.hook(&dep.path, hook, env, base)?);
                    }
                }
            }
        }
        for (child, node) in self.deps.iter().enumerate() {
            if node.parent == Some(index) {
                self.walk_hooks(child, out)?;
            }
        }
        Ok(())
    }

    /// The `pre_deps_hooks` of a dependency.
    pub fn pre_deps_hooks(&self, index: usize) -> Vec<HookEntry> {
        self.pre_hooks
            .iter()
            .find(|(owner, _)| *owner == index)
            .map(|(_, hooks)| hooks.clone())
            .unwrap_or_default()
    }

    /// `name: url` lines of every dependency, sorted by name: the output of `gclient revinfo`.
    pub fn revinfo(&self) -> Vec<String> {
        let mut entries: BTreeMap<&str, &str> = BTreeMap::new();
        for dep in &self.deps {
            entries
                .entry(dep.name.as_str())
                .or_insert(dep.url.as_deref().unwrap_or("None"));
        }
        entries
            .into_iter()
            .map(|(name, url)| format!("{name}: {url}"))
            .collect()
    }

    /// The variables of a dependency that has been expanded.
    pub fn env(&self, index: usize) -> Option<&Env> {
        self.deps.get(index).and_then(|dep| dep.env.as_ref())
    }

    /// The text of the `gclient_gn_args_file` of dependency `index`: one `name = value` line per `gclient_gn_args` entry.
    pub fn gn_args_text(&self, index: usize) -> Result<Option<(PathBuf, String)>> {
        let dep = &self.deps[index];
        let Some(file) = &dep.gn_args_file else {
            return Ok(None);
        };
        let env = dep
            .env
            .as_ref()
            .ok_or_else(|| Error::message(format!("{}: not expanded", dep.name)))?;
        let mut lines = vec![format!("# Generated from '{}'", dep.deps_file)];
        for argument in &dep.gn_args {
            let value = env.get(argument).ok_or_else(|| {
                Error::message(format!("Invalid gclient_gn_args: '{argument}' is not defined in vars."))
            })?;
            lines.push(format!("{argument} = {}", gn_string(value, env)));
        }
        let base = if dep.use_relative_paths {
            self.config.root.join(&dep.path)
        } else {
            self.config.root.clone()
        };
        Ok(Some((base.join(file), lines.join("\n"))))
    }
}

/// gclient's `ToGNString`. A string variable is a condition when it evaluates to a boolean, a literal otherwise.
fn gn_string(value: &Value, env: &Env) -> String {
    match value {
        Value::Bool(flag) => flag.to_string(),
        Value::Int(number) => number.to_string(),
        Value::Str(text) => match env.condition(text) {
            Ok(flag) if matches!(text.as_str(), t if t.contains(' ') || env.get(t).is_some()) => flag.to_string(),
            _ => format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\"").replace('$', "\\$")),
        },
        other => format!("{other:?}"),
    }
}
