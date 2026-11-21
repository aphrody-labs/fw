//! `fw kernel status`: the pins against kernel.org and CachyOS.

use std::path::Path;

use serde::Serialize;

use crate::error::{Error, Result};
use crate::pins::{Pins, Tree};
use crate::releases::Releases;
use crate::repo::{self, Remotes};
use crate::version::{CachyTag, Version};

#[derive(Debug, Serialize)]
pub struct Status {
    pub ok: bool,
    pub offline: bool,
    pub releases: Option<ReleasesView>,
    pub cachyos: Option<CachyosView>,
    pub trees: Vec<TreeStatus>,
    /// Pins older than what their moniker or CachyOS publishes now.
    pub behind: Vec<String>,
    /// Pins that are inconsistent: gitlink or remote tag disagreeing with kernel.toml, CachyOS not on the stable pin.
    pub problems: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ReleasesView {
    pub source: String,
    pub mainline: Option<String>,
    pub stable: Option<String>,
    pub stable_series: Vec<String>,
    pub longterm: Vec<String>,
    pub linux_next: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CachyosView {
    pub latest: Option<String>,
    pub latest_release_candidate: Option<String>,
    pub pkgbuilds_head: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TreeStatus {
    pub id: String,
    pub path: String,
    pub repo: String,
    pub moniker: Option<String>,
    pub tag: Option<String>,
    pub commit: String,
    /// What the pin is compared with: the newest release of its moniker and series, or the newest CachyOS tag.
    pub latest: Option<String>,
    pub behind: bool,
    pub gitlink: Option<String>,
    pub gitlink_matches: Option<bool>,
    pub checkout: Option<String>,
    pub checkout_matches: Option<bool>,
    pub remote_commit: Option<String>,
    pub remote_matches: Option<bool>,
}

pub struct Options {
    pub offline: bool,
}

pub fn compute(root: &Path, pins: &Pins, remotes: &Remotes, options: &Options) -> Result<Status> {
    let mut problems: Vec<String> = Vec::new();
    let mut behind: Vec<String> = Vec::new();

    let remote = if options.offline { None } else { Some(fetch_remote(pins, remotes)) };
    let (releases, cachyos, mut tag_lookups) = match remote {
        Some(Remote { releases, cachyos, tags }) => {
            let releases = match releases {
                Ok(releases) => Some(releases),
                Err(error) => {
                    problems.push(format!("releases feed unreachable: {error}"));
                    None
                }
            };
            let cachyos = match cachyos {
                Some(Ok(view)) => Some(view),
                Some(Err(error)) => {
                    problems.push(format!("CachyOS tags unreachable: {error}"));
                    None
                }
                None => None,
            };
            (releases, cachyos, tags)
        }
        None => (None, None, Vec::new()),
    };
    tag_lookups.reverse();

    let in_checkout = repo::is_work_tree(root);
    let mut trees = Vec::new();
    for tree in &pins.trees {
        let version = tree.tag.as_deref().and_then(tag_version);
        let latest = latest_for(tree, version, releases.as_ref(), cachyos.as_ref());
        let is_behind = match (&latest, version, tree.id.as_str()) {
            (Some(latest), _, "cachyos") => match (CachyTag::parse(latest), tree.tag.as_deref().and_then(CachyTag::parse)) {
                (Some(latest), Some(pinned)) => pinned < latest,
                _ => false,
            },
            (Some(latest), Some(pinned), _) => Version::parse(latest).is_some_and(|latest| pinned < latest),
            _ => false,
        };
        if is_behind && let (Some(tag), Some(latest)) = (&tree.tag, &latest) {
            behind.push(format!("{}: {tag} -> {latest}", tree.id));
        }

        let gitlink = in_checkout.then(|| repo::gitlink(root, &tree.path)).flatten();
        let gitlink_matches = in_checkout.then(|| gitlink.as_deref() == Some(tree.commit.as_str()));
        if gitlink_matches == Some(false) {
            problems.push(format!("{}: gitlink {} differs from kernel.toml commit {}", tree.id, gitlink.as_deref().unwrap_or("(none)"), tree.commit));
        }
        let checkout = repo::checkout_head(&root.join(&tree.path));
        let checkout_matches = checkout.as_ref().map(|head| *head == tree.commit);

        let lookup = tag_lookups.pop().flatten();
        let (remote_commit, remote_matches) = match (lookup, &tree.tag) {
            (Some(lookup), Some(tag)) => match lookup {
                Ok(Some(found)) => {
                    let matches = found.commit == tree.commit;
                    if !matches {
                        problems.push(format!("{}: tag {tag} in {} is {} but kernel.toml pins {}", tree.id, tree.repo, found.commit, tree.commit));
                    }
                    (Some(found.commit), Some(matches))
                }
                Ok(None) => {
                    problems.push(format!("{}: tag {tag} does not exist in {} (fw kernel pin creates it)", tree.id, tree.repo));
                    (None, Some(false))
                }
                Err(error) => {
                    problems.push(format!("{}: cannot list tags of {}: {error}", tree.id, tree.repo));
                    (None, None)
                }
            },
            _ => (None, None),
        };

        trees.push(TreeStatus {
            id: tree.id.clone(),
            path: tree.path.clone(),
            repo: tree.repo.clone(),
            moniker: tree.moniker.clone(),
            tag: tree.tag.clone(),
            commit: tree.commit.clone(),
            latest,
            behind: is_behind,
            gitlink,
            gitlink_matches,
            checkout,
            checkout_matches,
            remote_commit,
            remote_matches,
        });
    }

    check_cachyos_base(pins, &mut problems);

    let releases_view = releases.as_ref().map(|releases| ReleasesView {
        source: releases.source.clone(),
        mainline: releases.mainline().map(|release| release.version.clone()),
        stable: releases.latest_stable.clone(),
        stable_series: releases.by_moniker("stable").map(|release| release.version.clone()).collect(),
        longterm: releases.by_moniker("longterm").map(|release| release.version.clone()).collect(),
        linux_next: releases.linux_next().map(|release| release.version.clone()),
    });

    Ok(Status { ok: problems.is_empty(), offline: options.offline, releases: releases_view, cachyos, trees, behind, problems })
}

/// The version a tag names: `v7.3.5` and `cachyos-7.3.5-2` both name 7.3.5.
pub fn tag_version(tag: &str) -> Option<Version> {
    Version::parse(tag).or_else(|| CachyTag::parse(tag).map(|cachy| cachy.version))
}

fn latest_for(tree: &Tree, pinned: Option<Version>, releases: Option<&Releases>, cachyos: Option<&CachyosView>) -> Option<String> {
    if tree.id == "cachyos" {
        return cachyos.and_then(|view| view.latest.clone());
    }
    let moniker = tree.moniker.as_deref()?;
    let releases = releases?;
    let series = (moniker != "mainline").then(|| pinned.map(|version| version.series())).flatten();
    match releases.newest(moniker, series) {
        Some(release) => Some(release.version.clone()),
        // The pinned series is no longer published: anything newer of the moniker is where it moved.
        None => releases.newest(moniker, None).map(|release| release.version.clone()),
    }
}

fn cachyos_view(remotes: &Remotes, tree: &Tree, pins: &Pins) -> Result<CachyosView> {
    let tags = repo::remote_tags(&remotes.url(&tree.upstream), "cachyos-*")?;
    let parsed: Vec<CachyTag> = tags.iter().filter_map(|tag| CachyTag::parse(&tag.name)).collect();
    let latest = parsed.iter().filter(|tag| !tag.version.is_release_candidate()).max().map(CachyTag::tag);
    let latest_release_candidate = parsed.iter().filter(|tag| tag.version.is_release_candidate()).max().map(CachyTag::tag);
    let pkgbuilds_head = match pins.trees.iter().find(|tree| tree.id == "cachyos-pkgbuilds") {
        Some(pkgbuilds) => repo::remote_head(&remotes.url(&pkgbuilds.upstream), "HEAD")?,
        None => None,
    };
    Ok(CachyosView { latest, latest_release_candidate, pkgbuilds_head })
}

/// The CachyOS tree must sit on the kernel.org tree its tag names: mainline for release candidates, stable otherwise.
fn check_cachyos_base(pins: &Pins, problems: &mut Vec<String>) {
    let Some(cachyos) = pins.trees.iter().find(|tree| tree.id == "cachyos") else { return };
    let Some(tag) = cachyos.tag.as_deref().and_then(CachyTag::parse) else {
        problems.push("cachyos: the tag is not cachyos-<version>-<release>".to_owned());
        return;
    };
    let base_id = if tag.version.is_release_candidate() { "mainline" } else { "stable" };
    let Some(base) = pins.trees.iter().find(|tree| tree.id == base_id) else { return };
    let base_version = base.tag.as_deref().and_then(Version::parse);
    if base_version != Some(tag.version) {
        problems.push(format!("cachyos: tag {} is based on {} but tree {base_id} is pinned at {}", cachyos.tag.as_deref().unwrap_or(""), tag.version, base.tag.as_deref().unwrap_or("(none)")));
    }
}

/// Plain text for people; `--json` prints [`Status`] itself.
pub fn render(status: &Status) -> String {
    let mut text = String::new();
    if let Some(releases) = &status.releases {
        text.push_str(&format!(
            "kernel.org  mainline {}  stable {}  longterm {}  linux-next {}\n",
            releases.mainline.as_deref().unwrap_or("-"),
            releases.stable.as_deref().unwrap_or("-"),
            releases.longterm.join(" "),
            releases.linux_next.as_deref().unwrap_or("-")
        ));
    }
    if let Some(cachyos) = &status.cachyos {
        text.push_str(&format!("cachyos     latest {}  rc {}\n", cachyos.latest.as_deref().unwrap_or("-"), cachyos.latest_release_candidate.as_deref().unwrap_or("-")));
    }
    for tree in &status.trees {
        text.push_str(&format!(
            "{:<18} {:<18} {}  {}{}\n",
            tree.id,
            tree.tag.as_deref().unwrap_or("-"),
            &tree.commit[..tree.commit.len().min(12)],
            tree.latest.as_deref().map(|latest| format!("latest {latest}")).unwrap_or_default(),
            if tree.behind { "  BEHIND" } else { "" }
        ));
    }
    for problem in &status.problems {
        text.push_str(&format!("problem: {problem}\n"));
    }
    for line in &status.behind {
        text.push_str(&format!("behind: {line}\n"));
    }
    text
}

/// Everything that needs the network, fetched at once: the feed, the CachyOS tags and one tag lookup per tree.
struct Remote {
    releases: Result<Releases>,
    cachyos: Option<Result<CachyosView>>,
    tags: Vec<Option<Result<Option<repo::RemoteTag>>>>,
}

fn fetch_remote(pins: &Pins, remotes: &Remotes) -> Remote {
    let joined = |error: Box<dyn std::any::Any + Send>| Error::message(format!("remote lookup panicked: {error:?}"));
    std::thread::scope(|scope| {
        let releases = scope.spawn(|| Releases::fetch(&pins.releases));
        let cachyos = pins.trees.iter().find(|tree| tree.id == "cachyos").map(|tree| scope.spawn(move || cachyos_view(remotes, tree, pins)));
        let mut handles = Vec::new();
        for tree in &pins.trees {
            handles.push(tree.tag.as_ref().map(|tag| scope.spawn(move || repo::remote_tag(&remotes.url(&tree.repo), tag))));
        }
        let mut tags = Vec::new();
        for handle in handles {
            tags.push(handle.map(|handle| handle.join().map_err(joined).and_then(|result| result)));
        }
        Remote {
            releases: releases.join().map_err(joined).and_then(|result| result),
            cachyos: cachyos.map(|handle| handle.join().map_err(joined).and_then(|result| result)),
            tags,
        }
    })
}
