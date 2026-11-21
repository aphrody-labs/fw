//! AOSP's `repo`: the manifest (`default.xml`, `<include>`, `<remote>`, `<default>`, `<project>`, `<copyfile>`, `<linkfile>`, `<remove-project>`,
//! `<extend-project>`, groups), `init` (the manifest repository into `.repo/manifests`), `list`, and `sync` with parallel fetches.
//!
//! A project is a plain git checkout at its path (not repo's shared `.repo/project-objects` store), so a tree synced here is read by git as is.
//! Not ported: superproject sync, `repo upload`, `repo forall`, manifest servers and smart sync.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result, fs};
use crate::git;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Remote {
    pub name: String,
    pub fetch: String,
    pub review: Option<String>,
    pub revision: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Defaults {
    pub remote: Option<String>,
    pub revision: Option<String>,
    pub dest_branch: Option<String>,
    pub sync_j: Option<usize>,
    pub sync_c: bool,
    pub sync_s: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileOp {
    pub src: String,
    pub dest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Project {
    pub name: String,
    pub path: String,
    pub remote: Option<String>,
    pub revision: Option<String>,
    pub groups: Vec<String>,
    pub clone_depth: Option<u32>,
    pub upstream: Option<String>,
    pub copyfiles: Vec<FileOp>,
    pub linkfiles: Vec<FileOp>,
}

#[derive(Debug, Clone, Default)]
pub struct Manifest {
    pub remotes: Vec<Remote>,
    pub defaults: Defaults,
    pub projects: Vec<Project>,
}

fn attribute(node: roxmltree::Node<'_, '_>, name: &str) -> Option<String> {
    node.attribute(name).map(str::to_owned)
}

fn groups(text: Option<&str>) -> Vec<String> {
    text.map(|text| {
        text.split([',', ' '])
            .filter(|group| !group.is_empty())
            .map(str::to_owned)
            .collect()
    })
    .unwrap_or_default()
}

impl Manifest {
    /// Reads `name` from `directory` (the `.repo/manifests` of a checkout), following includes.
    pub fn load(directory: &Path, name: &str) -> Result<Manifest> {
        let mut manifest = Manifest::default();
        let mut visiting = Vec::new();
        manifest.read(directory, name, &[], &mut visiting)?;
        Ok(manifest)
    }

    pub fn parse(text: &str) -> Result<Manifest> {
        let mut manifest = Manifest::default();
        let document = roxmltree::Document::parse(text).map_err(|error| Error::message(format!("manifest: {error}")))?;
        manifest.apply(&document, &[], None)?;
        Ok(manifest)
    }

    fn read(&mut self, directory: &Path, name: &str, inherited: &[String], visiting: &mut Vec<String>) -> Result<()> {
        if visiting.iter().any(|seen| seen == name) {
            return Err(Error::message(format!("manifest: {name} includes itself")));
        }
        visiting.push(name.to_owned());
        let path = directory.join(name);
        let text = fs::read_to_string(&path)?;
        let document = roxmltree::Document::parse(&text)
            .map_err(|error| Error::message(format!("{}: {error}", path.display())))?;
        self.apply(&document, inherited, Some((directory, visiting)))?;
        visiting.pop();
        Ok(())
    }

    fn apply(
        &mut self,
        document: &roxmltree::Document<'_>,
        inherited: &[String],
        mut context: Option<(&Path, &mut Vec<String>)>,
    ) -> Result<()> {
        let root = document.root_element();
        if root.tag_name().name() != "manifest" {
            return Err(Error::message("manifest: the root element is not <manifest>"));
        }
        for node in root.children().filter(roxmltree::Node::is_element) {
            match node.tag_name().name() {
                "remote" => {
                    let name = attribute(node, "name").ok_or_else(|| Error::message("<remote> needs a name"))?;
                    let remote = Remote {
                        fetch: attribute(node, "fetch").unwrap_or_default(),
                        review: attribute(node, "review"),
                        revision: attribute(node, "revision"),
                        name,
                    };
                    if let Some(existing) = self.remotes.iter_mut().find(|other| other.name == remote.name) {
                        *existing = remote;
                    } else {
                        self.remotes.push(remote);
                    }
                }
                "default" => {
                    self.defaults = Defaults {
                        remote: attribute(node, "remote"),
                        revision: attribute(node, "revision"),
                        dest_branch: attribute(node, "dest-branch"),
                        sync_j: node.attribute("sync-j").and_then(|value| value.parse().ok()),
                        sync_c: node.attribute("sync-c") == Some("true"),
                        sync_s: node.attribute("sync-s") == Some("true"),
                    };
                }
                "project" => {
                    let name = attribute(node, "name").ok_or_else(|| Error::message("<project> needs a name"))?;
                    let path = attribute(node, "path").unwrap_or_else(|| name.clone());
                    let mut project = Project {
                        path: path.trim_end_matches('/').to_owned(),
                        remote: attribute(node, "remote"),
                        revision: attribute(node, "revision"),
                        groups: groups(node.attribute("groups")),
                        clone_depth: node.attribute("clone-depth").and_then(|value| value.parse().ok()),
                        upstream: attribute(node, "upstream"),
                        name,
                        ..Project::default()
                    };
                    project.groups.extend(inherited.iter().cloned());
                    for child in node.children().filter(roxmltree::Node::is_element) {
                        let operation = FileOp {
                            src: attribute(child, "src").unwrap_or_default(),
                            dest: attribute(child, "dest").unwrap_or_default(),
                        };
                        match child.tag_name().name() {
                            "copyfile" => project.copyfiles.push(operation),
                            "linkfile" => project.linkfiles.push(operation),
                            _ => {}
                        }
                    }
                    if self.projects.iter().any(|other| other.path == project.path) {
                        return Err(Error::message(format!(
                            "manifest: duplicate project path {}",
                            project.path
                        )));
                    }
                    self.projects.push(project);
                }
                "remove-project" => {
                    let name = attribute(node, "name");
                    let path = attribute(node, "path");
                    let optional = node.attribute("optional") == Some("true");
                    let before = self.projects.len();
                    self.projects.retain(|project| {
                        let by_name = name.as_deref().is_some_and(|name| name == project.name);
                        let by_path = path.as_deref().is_some_and(|path| path == project.path);
                        let matches = match (&name, &path) {
                            (Some(_), Some(_)) => by_name && by_path,
                            _ => by_name || by_path,
                        };
                        !matches
                    });
                    if self.projects.len() == before && !optional {
                        return Err(Error::message(format!(
                            "manifest: <remove-project> of {} matches nothing",
                            name.or(path).unwrap_or_default()
                        )));
                    }
                }
                "extend-project" => {
                    let name = attribute(node, "name").ok_or_else(|| Error::message("<extend-project> needs a name"))?;
                    let path = attribute(node, "path");
                    let mut touched = false;
                    for project in self.projects.iter_mut().filter(|project| {
                        project.name == name && path.as_deref().is_none_or(|path| path == project.path)
                    }) {
                        touched = true;
                        if let Some(revision) = attribute(node, "revision") {
                            project.revision = Some(revision);
                        }
                        if let Some(remote) = attribute(node, "remote") {
                            project.remote = Some(remote);
                        }
                        if let Some(depth) = node.attribute("clone-depth").and_then(|value| value.parse().ok()) {
                            project.clone_depth = Some(depth);
                        }
                        if let Some(dest) = attribute(node, "dest-path") {
                            project.path = dest;
                        }
                        project.groups.extend(groups(node.attribute("groups")));
                    }
                    if !touched {
                        return Err(Error::message(format!(
                            "manifest: <extend-project> of {name} matches nothing"
                        )));
                    }
                }
                "include" => {
                    let name = attribute(node, "name").ok_or_else(|| Error::message("<include> needs a name"))?;
                    let mut inherited = inherited.to_vec();
                    inherited.extend(groups(node.attribute("groups")));
                    let Some((directory, visiting)) = context.as_mut() else {
                        return Err(Error::message(format!("manifest: cannot include {name} from a string")));
                    };
                    self.read(directory, &name, &inherited, visiting)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn remote(&self, name: &str) -> Option<&Remote> {
        self.remotes.iter().find(|remote| remote.name == name)
    }

    /// The remote, revision and url of a project against `manifest_url` (the url of the manifest repository, for relative `fetch` values).
    pub fn resolve(&self, project: &Project, manifest_url: &str) -> Result<Resolved> {
        let remote_name = project
            .remote
            .as_deref()
            .or(self.defaults.remote.as_deref())
            .ok_or_else(|| Error::message(format!("project {} has no remote", project.name)))?;
        let remote = self
            .remote(remote_name)
            .ok_or_else(|| Error::message(format!("project {}: no remote named {remote_name}", project.name)))?;
        let revision = project
            .revision
            .clone()
            .or_else(|| remote.revision.clone())
            .or_else(|| self.defaults.revision.clone())
            .ok_or_else(|| Error::message(format!("project {} has no revision", project.name)))?;
        let base = absolute_fetch(manifest_url, &remote.fetch);
        Ok(Resolved {
            url: format!("{}/{}", base.trim_end_matches('/'), project.name.trim_start_matches('/')),
            revision,
            remote: remote.name.clone(),
        })
    }

    /// The projects a group filter selects (repo's `MatchesGroups`): `["default"]` when empty.
    pub fn select(&self, manifest_groups: &[String]) -> Vec<&Project> {
        self.projects
            .iter()
            .filter(|project| matches_groups(project, manifest_groups))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub url: String,
    pub revision: String,
    pub remote: String,
}

/// repo's `Project.MatchesGroups`.
pub fn matches_groups(project: &Project, manifest_groups: &[String]) -> bool {
    let default = ["default".to_owned()];
    let wanted: &[String] = if manifest_groups.is_empty() { &default } else { manifest_groups };
    let mut have: Vec<String> = vec!["all".to_owned()];
    have.extend(project.groups.iter().cloned());
    have.push(format!("name:{}", project.name));
    have.push(format!("path:{}", project.path));
    if !project.groups.iter().any(|group| group == "notdefault") {
        have.push("default".to_owned());
    }
    let mut matched = false;
    for group in wanted {
        if let Some(excluded) = group.strip_prefix('-') {
            if have.iter().any(|item| item == excluded) {
                matched = false;
            }
        } else if have.iter().any(|item| item == group) {
            matched = true;
        }
    }
    matched
}

/// A relative `fetch` (`..`, `../x`) resolved against the manifest repository's url the way repo does, with `urljoin`: the last segment of the
/// manifest url is a file name, so `..` from `.../platform/manifest` is the host root.
pub fn absolute_fetch(manifest_url: &str, fetch: &str) -> String {
    if fetch.contains("://") || fetch.starts_with("git@") || fetch.starts_with('/') {
        return fetch.to_owned();
    }
    let manifest_url = manifest_url.trim_end_matches('/');
    let (scheme_and_host, path) = match manifest_url.split_once("://") {
        Some((scheme, rest)) => match rest.split_once('/') {
            Some((host, path)) => (format!("{scheme}://{host}"), format!("/{path}")),
            None => (format!("{scheme}://{rest}"), String::new()),
        },
        None => (String::new(), manifest_url.to_owned()),
    };
    let mut segments: Vec<&str> = path.split('/').collect();
    segments.pop();
    for part in fetch.trim_end_matches('/').split('/') {
        match part {
            ".." => {
                if segments.len() > 1 {
                    segments.pop();
                }
            }
            "." | "" => {}
            other => segments.push(other),
        }
    }
    let joined = segments.join("/");
    format!("{scheme_and_host}{}", if joined.is_empty() { "/".to_owned() } else { joined })
}

/// `.repo/manifests` of a checkout rooted at `top`, and the url it was cloned from.
pub fn manifest_dir(top: &Path) -> PathBuf {
    top.join(".repo").join("manifests")
}

/// `repo init -u URL -b REVISION [-m NAME]`: the manifest repository into `.repo/manifests`, and `.repo/manifest.xml` including it.
pub fn init(top: &Path, url: &str, revision: &str, manifest: &str) -> Result<()> {
    let directory = manifest_dir(top);
    git::checkout(&directory, url, Some(revision), git::History::Shallow)?;
    fs::write(
        &top.join(".repo").join("manifest.xml"),
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<manifest>\n  <include name=\"{manifest}\" />\n</manifest>\n"),
    )?;
    git::run(&directory, &["config", "fw-depot.url", url])?;
    git::run(&directory, &["config", "fw-depot.manifest", manifest])?;
    Ok(())
}

/// The manifest of an initialised checkout and the url of its manifest repository.
pub fn load_checkout(top: &Path) -> Result<(Manifest, String)> {
    let directory = manifest_dir(top);
    let url = git::run(&directory, &["config", "fw-depot.url"])
        .or_else(|_| git::run(&directory, &["remote", "get-url", "origin"]))?;
    let name = git::run(&directory, &["config", "fw-depot.manifest"]).unwrap_or_else(|_| "default.xml".to_owned());
    Ok((Manifest::load(&directory, &name)?, url))
}

#[derive(Debug, Clone)]
pub struct SyncOptions {
    pub jobs: usize,
    pub groups: Vec<String>,
    pub shallow: bool,
    pub partial: bool,
    /// Fetch and record the commit but check no files out.
    pub metadata_only: bool,
    pub dry_run: bool,
    /// Only the projects whose path starts with one of these.
    pub only: Vec<String>,
}

impl Default for SyncOptions {
    fn default() -> Self {
        SyncOptions {
            jobs: 8,
            groups: Vec::new(),
            shallow: false,
            partial: false,
            metadata_only: false,
            dry_run: false,
            only: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Synced {
    pub path: String,
    pub revision: String,
    pub commit: String,
}

fn sync_project(top: &Path, project: &Project, resolved: &Resolved, options: &SyncOptions) -> Result<Synced> {
    let directory = top.join(&project.path);
    if options.dry_run {
        return Ok(Synced {
            path: project.path.clone(),
            revision: resolved.revision.clone(),
            commit: String::new(),
        });
    }
    let revision = resolved.revision.as_str();
    let history = if options.shallow || project.clone_depth.is_some() {
        git::History::Shallow
    } else if options.partial || options.metadata_only {
        git::History::BlobLess
    } else {
        git::History::Full
    };
    let commit = if options.metadata_only {
        fetch_only(&directory, &resolved.url, revision, history)?
    } else {
        let reference = if revision.starts_with("refs/heads/") || revision.starts_with("refs/tags/") || git::is_full_hash(revision) {
            revision.to_owned()
        } else {
            format!("refs/heads/{revision}")
        };
        git::checkout(&directory, &resolved.url, Some(&reference), history)?
    };
    for operation in &project.copyfiles {
        let source = directory.join(&operation.src);
        let target = top.join(&operation.dest);
        if options.metadata_only || !source.is_file() {
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        std::fs::copy(&source, &target).map_err(|error| Error::io(&target, error))?;
    }
    for operation in &project.linkfiles {
        if options.metadata_only {
            continue;
        }
        link(top, &directory, operation)?;
    }
    Ok(Synced {
        path: project.path.clone(),
        revision: resolved.revision.clone(),
        commit,
    })
}

/// A repository with the commit fetched and no work tree files.
fn fetch_only(directory: &Path, url: &str, revision: &str, history: git::History) -> Result<String> {
    fs::create_dir_all(directory)?;
    if !git::is_repository(directory) {
        git::run(directory, &["init", "--quiet"])?;
        git::run(directory, &["remote", "add", "origin", url])?;
    }
    let reference = if git::is_full_hash(revision) || revision.starts_with("refs/") {
        revision.to_owned()
    } else {
        format!("refs/heads/{revision}")
    };
    let mut arguments = vec!["fetch", "--quiet", "--no-tags"];
    match history {
        git::History::Shallow => arguments.push("--depth=1"),
        git::History::BlobLess => arguments.push("--filter=blob:none"),
        git::History::Full => {}
    }
    arguments.extend(["origin", &reference]);
    git::run(directory, &arguments)?;
    let commit = git::run(directory, &["rev-parse", "FETCH_HEAD^{commit}"])?;
    git::run(directory, &["update-ref", "HEAD", &commit])?;
    Ok(commit)
}

fn link(top: &Path, directory: &Path, operation: &FileOp) -> Result<()> {
    let target = top.join(&operation.dest);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let relative = relative_path(target.parent().unwrap_or(top), &directory.join(&operation.src));
    if target.symlink_metadata().is_ok() {
        std::fs::remove_file(&target).map_err(|error| Error::io(&target, error))?;
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&relative, &target).map_err(|error| Error::io(&target, error))
    }
    #[cfg(not(unix))]
    {
        std::fs::write(&target, relative.to_string_lossy().as_bytes()).map_err(|error| Error::io(&target, error))
    }
}

/// `to` as a path relative to the directory `from`.
pub fn relative_path(from: &Path, to: &Path) -> PathBuf {
    let from: Vec<_> = from.components().collect();
    let to: Vec<_> = to.components().collect();
    let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let mut out = PathBuf::new();
    for _ in common..from.len() {
        out.push("..");
    }
    for part in &to[common..] {
        out.push(part);
    }
    out
}

/// `repo sync`: every selected project fetched at its pinned revision, `jobs` at a time.
pub fn sync(top: &Path, options: &SyncOptions) -> Result<Vec<Synced>> {
    let (manifest, url) = load_checkout(top)?;
    let selected: Vec<&Project> = manifest
        .select(&options.groups)
        .into_iter()
        .filter(|project| options.only.is_empty() || options.only.iter().any(|prefix| project.path.starts_with(prefix.as_str())))
        .collect();
    let jobs = options.jobs.max(1);
    let mut work = Vec::new();
    for project in selected {
        work.push((project, manifest.resolve(project, &url)?));
    }
    crate::cipd::parallel(&work, jobs, |(project, resolved)| {
        sync_project(top, project, resolved, options).map_err(|error| Error::message(format!("{}: {error}", project.path)))
    })
}

/// `repo list`: `path : name`, sorted by path.
pub fn list(manifest: &Manifest, groups: &[String]) -> Vec<String> {
    let mut lines: Vec<(String, String)> = manifest
        .select(groups)
        .into_iter()
        .map(|project| (project.path.clone(), project.name.clone()))
        .collect();
    lines.sort();
    lines.into_iter().map(|(path, name)| format!("{path} : {name}")).collect()
}

/// A summary by remote and revision, for `repo info`-like output.
pub fn summary(manifest: &Manifest) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for project in &manifest.projects {
        let remote = project
            .remote
            .clone()
            .or_else(|| manifest.defaults.remote.clone())
            .unwrap_or_default();
        *counts.entry(remote).or_insert(0) += 1;
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<manifest>
  <remote name="aosp" fetch=".." review="https://r" />
  <default revision="refs/heads/main" remote="aosp" sync-j="4" />
  <project path="build/make" name="platform/build" groups="pdk">
    <copyfile src="core/root.mk" dest="Makefile" />
    <linkfile src="envsetup.sh" dest="build/envsetup.sh" />
  </project>
  <project path="external/x" name="platform/external/x" groups="notdefault,tools" clone-depth="1" revision="refs/tags/t" />
  <project path="gone" name="platform/gone" />
  <remove-project name="platform/gone" />
  <extend-project name="platform/build" revision="refs/heads/other" groups="extra" />
</manifest>"#;

    #[test]
    fn parses_projects_files_and_edits() {
        let manifest = Manifest::parse(SAMPLE).unwrap();
        assert_eq!(manifest.projects.len(), 2);
        let build = &manifest.projects[0];
        assert_eq!(build.revision.as_deref(), Some("refs/heads/other"));
        assert_eq!(build.groups, ["pdk", "extra"]);
        assert_eq!(build.copyfiles, [FileOp { src: "core/root.mk".into(), dest: "Makefile".into() }]);
        assert_eq!(build.linkfiles[0].dest, "build/envsetup.sh");
        let resolved = manifest.resolve(build, "https://android.googlesource.com/platform/manifest").unwrap();
        assert_eq!(resolved.url, "https://android.googlesource.com/platform/build");
        assert_eq!(resolved.revision, "refs/heads/other");
    }

    #[test]
    fn groups_follow_repo() {
        let manifest = Manifest::parse(SAMPLE).unwrap();
        let names = |groups: &[&str]| -> Vec<String> {
            let groups: Vec<String> = groups.iter().map(|group| (*group).to_owned()).collect();
            manifest.select(&groups).iter().map(|project| project.path.clone()).collect()
        };
        assert_eq!(names(&[]), ["build/make"]);
        assert_eq!(names(&["tools"]), ["external/x"]);
        assert_eq!(names(&["all"]), ["build/make", "external/x"]);
        assert_eq!(names(&["all", "-pdk"]), ["external/x"]);
        assert_eq!(names(&["path:external/x"]), ["external/x"]);
    }

    #[test]
    fn relative_fetch_follows_the_manifest_url() {
        assert_eq!(
            absolute_fetch("https://android.googlesource.com/platform/manifest", ".."),
            "https://android.googlesource.com/"
        );
        assert_eq!(absolute_fetch("https://a/b/manifest.git", "../x"), "https://a/x");
        assert_eq!(absolute_fetch("https://a/b/manifest", "x"), "https://a/b/x");
        assert_eq!(absolute_fetch("https://a/b", "https://c"), "https://c");
    }

    #[test]
    fn links_are_relative() {
        assert_eq!(
            relative_path(Path::new("/t/build"), Path::new("/t/build/make/envsetup.sh")),
            PathBuf::from("make/envsetup.sh")
        );
        assert_eq!(relative_path(Path::new("/t/a/b"), Path::new("/t/c/d")), PathBuf::from("../../c/d"));
    }
}
