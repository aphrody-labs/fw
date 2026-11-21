//! `fw kernel pin`: move a tree to a tag, in kernel.toml and in the gitlink of the fw index.

use std::path::Path;
use std::process::Command;

use serde::Serialize;

use crate::error::{Error, Result};
use crate::pins::Pins;
use crate::releases::Releases;
use crate::repo::{self, Remotes};
use crate::version::{CachyTag, Version};

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Outcome {
    pub tree: String,
    pub previous: Option<String>,
    pub tag: Option<String>,
    pub commit: String,
    /// The tag did not exist in the fork and was created there from the upstream tag.
    pub fork_tag_created: bool,
    pub gitlink_updated: bool,
}

pub struct Options {
    /// Force the tree instead of inferring it from the target.
    pub tree: Option<String>,
    /// Leave the git index alone: only kernel.toml changes.
    pub no_index: bool,
}

/// What a target names: a tag in one tree, plus the trees that must follow it.
struct Plan {
    steps: Vec<(String, Pinned)>,
}

enum Pinned {
    Tag { tag: String, moniker: Option<&'static str> },
    Head,
}

/// Resolves `target` and applies it; the result lists every tree that moved, in order.
pub fn apply(root: &Path, pins: &mut Pins, remotes: &Remotes, target: &str, options: &Options) -> Result<Vec<Outcome>> {
    let plan = plan(pins, remotes, target, options)?;
    let mut outcomes = Vec::new();
    for (tree_id, pinned) in plan.steps {
        let outcome = match pinned {
            Pinned::Tag { tag, moniker } => pin_tag(root, pins, remotes, &tree_id, &tag, moniker, options)?,
            Pinned::Head => pin_head(root, pins, remotes, &tree_id, options)?,
        };
        outcomes.push(outcome);
    }
    Ok(outcomes)
}

fn plan(pins: &Pins, remotes: &Remotes, target: &str, options: &Options) -> Result<Plan> {
    let feed = || Releases::fetch(&pins.releases);
    let steps = match target {
        "mainline" => {
            let feed = feed()?;
            let release = feed.mainline().ok_or_else(|| Error::message("releases feed lists no mainline kernel"))?;
            vec![("mainline".to_owned(), Pinned::Tag { tag: release_tag(&release.version)?, moniker: None })]
        }
        "stable" => {
            let feed = feed()?;
            let version = feed.latest_stable.clone().ok_or_else(|| Error::message("releases feed has no latest_stable"))?;
            vec![("stable".to_owned(), Pinned::Tag { tag: release_tag(&version)?, moniker: Some("stable") })]
        }
        "linux-next" => return Err(Error::message("linux-next (next-YYYYMMDD) tags live in the linux-next tree, which is not pinned; `fw kernel status` reports its version")),
        "cachyos" | "cachyos-rc" => {
            let candidate = target == "cachyos-rc";
            let cachyos = pins.tree("cachyos")?;
            let tags = repo::remote_tags(&remotes.url(&cachyos.upstream), "cachyos-*")?;
            let latest = tags
                .iter()
                .filter_map(|tag| CachyTag::parse(&tag.name))
                .filter(|tag| tag.version.is_release_candidate() == candidate)
                .max()
                .ok_or_else(|| Error::message(format!("no {} tag in {}", if candidate { "release candidate" } else { "stable" }, cachyos.upstream)))?;
            cachyos_steps(pins, &latest)
        }
        text if text == "longterm" || text.starts_with("longterm:") => {
            let feed = feed()?;
            let series = match text.strip_prefix("longterm:") {
                Some(series) => Some(Version::parse(series).map(|version| version.series()).ok_or_else(|| Error::message(format!("bad longterm series `{series}`")))?),
                None => None,
            };
            let release = feed.newest("longterm", series).ok_or_else(|| Error::message(format!("no longterm release for {text} in the releases feed")))?;
            vec![("stable".to_owned(), Pinned::Tag { tag: release_tag(&release.version)?, moniker: Some("longterm") })]
        }
        text => {
            if let Some(cachy) = CachyTag::parse(text) {
                cachyos_steps(pins, &cachy)
            } else if let Some(version) = Version::parse(text) {
                let tree = match &options.tree {
                    Some(tree) => tree.clone(),
                    None if version.is_release_candidate() || version.patch.is_none() => "mainline".to_owned(),
                    None => "stable".to_owned(),
                };
                let moniker = (tree == "stable").then(|| classify(&feed().ok(), &version));
                vec![(tree, Pinned::Tag { tag: version.tag(), moniker })]
            } else {
                return Err(Error::message(format!(
                    "cannot pin `{target}`: use mainline, stable, longterm[:series], cachyos, cachyos-rc, a version (7.3.5, 7.4-rc3) or a tag (v6.18.64, cachyos-7.3.5-2)"
                )));
            }
        }
    };
    if let Some(forced) = &options.tree
        && steps.len() == 1
    {
        let (_, pinned) = steps.into_iter().next().ok_or_else(|| Error::message("empty pin plan"))?;
        return Ok(Plan { steps: vec![(forced.clone(), pinned)] });
    }
    Ok(Plan { steps })
}

/// A CachyOS tag moves the CachyOS tree, the kernel.org tree it is built on, and the PKGBUILDs to their upstream head.
fn cachyos_steps(pins: &Pins, tag: &CachyTag) -> Vec<(String, Pinned)> {
    let base = if tag.version.is_release_candidate() { "mainline" } else { "stable" };
    let mut steps = vec![("cachyos".to_owned(), Pinned::Tag { tag: tag.tag(), moniker: None }), (base.to_owned(), Pinned::Tag { tag: tag.version.tag(), moniker: None })];
    if pins.trees.iter().any(|tree| tree.id == "cachyos-pkgbuilds") {
        steps.push(("cachyos-pkgbuilds".to_owned(), Pinned::Head));
    }
    steps
}

fn release_tag(version: &str) -> Result<String> {
    Version::parse(version).map(|version| version.tag()).ok_or_else(|| Error::message(format!("`{version}` is not a kernel version")))
}

/// `longterm` when the feed lists the series as longterm, `stable` otherwise.
fn classify(feed: &Option<Releases>, version: &Version) -> &'static str {
    let longterm = feed.as_ref().is_some_and(|feed| feed.by_moniker("longterm").filter_map(|release| release.parsed()).any(|listed| listed.series() == version.series()));
    if longterm { "longterm" } else { "stable" }
}

fn pin_tag(root: &Path, pins: &mut Pins, remotes: &Remotes, tree_id: &str, tag: &str, moniker: Option<&'static str>, options: &Options) -> Result<Outcome> {
    let tree = pins.tree(tree_id)?.clone();
    let fork_url = remotes.url(&tree.repo);
    let (found, fork_tag_created) = match repo::remote_tag(&fork_url, tag)? {
        Some(found) => (found, false),
        None => {
            let upstream = repo::remote_tag(&remotes.url(&tree.upstream), tag)?.ok_or_else(|| Error::message(format!("tag {tag} exists neither in {} nor in {}", tree.repo, tree.upstream)))?;
            create_fork_tag(&tree.repo, tag, &upstream.object)?;
            (upstream, true)
        }
    };
    let entry = pins.tree_mut(tree_id)?;
    let previous = entry.tag.clone();
    entry.tag = Some(tag.to_owned());
    entry.commit = found.commit.clone();
    if let Some(moniker) = moniker {
        entry.moniker = Some(moniker.to_owned());
    }
    let gitlink_updated = update_index(root, &tree.path, &found.commit, options);
    Ok(Outcome { tree: tree_id.to_owned(), previous, tag: Some(tag.to_owned()), commit: found.commit, fork_tag_created, gitlink_updated })
}

fn pin_head(root: &Path, pins: &mut Pins, remotes: &Remotes, tree_id: &str, options: &Options) -> Result<Outcome> {
    let tree = pins.tree(tree_id)?.clone();
    let head = repo::remote_head(&remotes.url(&tree.upstream), "HEAD")?.ok_or_else(|| Error::message(format!("{} has no HEAD", tree.upstream)))?;
    let entry = pins.tree_mut(tree_id)?;
    entry.commit = head.clone();
    let gitlink_updated = update_index(root, &tree.path, &head, options);
    Ok(Outcome { tree: tree_id.to_owned(), previous: tree.tag, tag: None, commit: head, fork_tag_created: false, gitlink_updated })
}

fn update_index(root: &Path, path: &str, commit: &str, options: &Options) -> bool {
    !options.no_index && repo::is_work_tree(root) && repo::set_gitlink(root, path, commit).is_ok()
}

/// Tags are not copied when a fork is created with `--default-branch-only`, and forks never follow new upstream tags,
/// so the tag is created in the fork through the API (the objects are shared inside the fork network).
fn create_fork_tag(fork: &str, tag: &str, object: &str) -> Result<()> {
    let endpoint = format!("repos/{fork}/git/refs");
    let output = Command::new("gh")
        .args(["api", &endpoint, "-f", &format!("ref=refs/tags/{tag}"), "-f", &format!("sha={object}")])
        .output()
        .map_err(|error| Error::Command { command: format!("gh api {endpoint}"), detail: format!("{error}; create the tag by hand: gh api {endpoint} -f ref=refs/tags/{tag} -f sha={object}") })?;
    if output.status.success() {
        return Ok(());
    }
    Err(Error::Command { command: format!("gh api {endpoint}"), detail: String::from_utf8_lossy(&output.stderr).trim().to_owned() })
}
