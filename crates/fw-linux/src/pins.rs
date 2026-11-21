//! `kernel.toml`: the declarative pins of every kernel tree.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result, read_to_string};

pub const FILE_NAME: &str = "kernel.toml";

const HEADER: &str = "# Kernel pins read by `fw kernel` (crates/fw-linux). Edit with `fw kernel pin`, never by hand.\n\
# Each tree is a git submodule (`.gitmodules`: relative URL, shallow, update = none), so a normal clone of fw stays\n\
# light; `fw kernel sync` fetches the pinned commit on demand with the profile named by the tree.\n\n";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pins {
    pub schema: u32,
    /// The kernel.org releases feed (`file://` and plain paths work for offline runs).
    pub releases: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub profile: BTreeMap<String, Profile>,
    #[serde(rename = "tree")]
    pub trees: Vec<Tree>,
}

/// How much of a tree `fw kernel sync` fetches and checks out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    /// Partial clone filter (`blob:none`): blobs are fetched when the checkout needs them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    /// Sparse checkout patterns; empty means the whole tree.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sparse: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tree {
    pub id: String,
    pub role: String,
    pub path: String,
    /// The fork in the organisation that the submodule points at (`owner/name`).
    pub repo: String,
    pub upstream: String,
    /// kernel.org moniker the tag follows: `mainline`, `stable` or `longterm`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moniker: Option<String>,
    /// Profile `fw kernel sync` uses when none is given.
    pub profile: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    pub commit: String,
}

impl Pins {
    pub fn load(path: &Path) -> Result<Pins> {
        let text = read_to_string(path)?;
        let pins: Pins = toml::from_str(&text).map_err(|error| Error::Toml { path: path.to_owned(), detail: error.to_string() })?;
        for tree in &pins.trees {
            if !pins.profile.contains_key(&tree.profile) {
                return Err(Error::message(format!("{}: tree {} names the unknown profile {}", path.display(), tree.id, tree.profile)));
            }
        }
        Ok(pins)
    }

    pub fn to_toml(&self) -> Result<String> {
        let body = toml::to_string_pretty(self).map_err(|error| Error::message(format!("serialize {FILE_NAME}: {error}")))?;
        Ok(format!("{HEADER}{body}"))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let text = self.to_toml()?;
        std::fs::write(path, text).map_err(|source| Error::Io { path: path.to_owned(), source })
    }

    pub fn tree(&self, id: &str) -> Result<&Tree> {
        self.trees.iter().find(|tree| tree.id == id).ok_or_else(|| {
            let known: Vec<&str> = self.trees.iter().map(|tree| tree.id.as_str()).collect();
            Error::message(format!("unknown tree `{id}` (known: {})", known.join(", ")))
        })
    }

    pub fn tree_mut(&mut self, id: &str) -> Result<&mut Tree> {
        let known: Vec<String> = self.trees.iter().map(|tree| tree.id.clone()).collect();
        self.trees.iter_mut().find(|tree| tree.id == id).ok_or_else(|| Error::message(format!("unknown tree `{id}` (known: {})", known.join(", "))))
    }

    pub fn profile(&self, name: &str) -> Result<&Profile> {
        self.profile.get(name).ok_or_else(|| Error::message(format!("unknown profile `{name}`")))
    }
}

/// The fw checkout that holds `kernel.toml`: `--root`, `$FW_ROOT`, or the nearest ancestor of `start`.
pub fn find_root(explicit: Option<&Path>, start: &Path) -> Result<PathBuf> {
    if let Some(root) = explicit {
        return Ok(root.to_owned());
    }
    if let Some(root) = std::env::var_os("FW_ROOT") {
        return Ok(PathBuf::from(root));
    }
    start
        .ancestors()
        .find(|directory| directory.join(FILE_NAME).is_file())
        .map(Path::to_owned)
        .ok_or_else(|| Error::message(format!("no {FILE_NAME} in {} or above (use --root DIR)", start.display())))
}
