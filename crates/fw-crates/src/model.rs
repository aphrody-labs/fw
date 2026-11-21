//! Data model: one `Record` per crate, `Meta` for the dump it came from, `Category` from crates.io.

use serde::{Deserialize, Serialize};

/// Bumped whenever `rank.rs` changes the formula or the weights, so stored scores are never compared across versions.
pub const SCORE_VERSION: u32 = 1;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Components {
    pub downloads: f64,
    pub dependents: f64,
    pub recency: f64,
    pub maintenance: f64,
    pub license: f64,
    pub msrv: f64,
    pub security: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Advisories {
    /// RustSec ids of non-informational advisories that affect the default version.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub open: Vec<String>,
    /// RustSec ids of informational advisories (unmaintained, unsound, notice) on the crate.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub informational: Vec<String>,
}

impl Advisories {
    pub fn is_empty(&self) -> bool {
        self.open.is_empty() && self.informational.is_empty()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub name: String,
    /// Default version: the highest non-yanked stable release, as crates.io picks it.
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub repository: String,
    /// First publish, `YYYY-MM-DD`.
    pub created: String,
    /// Latest non-yanked publish, `YYYY-MM-DD`.
    pub last_release: String,
    pub downloads: u64,
    /// Downloads over the 90 days that end at the dump (the dump's `version_downloads` window).
    pub recent_downloads: u64,
    /// Crates whose default version depends on this one (normal and build dependencies, optional included).
    pub dependents: u32,
    pub owners: u32,
    pub versions: u32,
    /// Non-yanked releases published in the 365 days before the dump.
    pub releases_365d: u32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub msrv: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub edition: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub license: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub yanked: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    #[serde(default, skip_serializing_if = "Advisories::is_empty")]
    pub advisories: Advisories,
    pub score: f64,
    pub components: Components,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Category {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub crates: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Meta {
    pub score_version: u32,
    /// `YYYY-MM-DD` of the dump; every number in the dataset is as of this date.
    pub dump_date: String,
    /// The dump's own `metadata.json` timestamp.
    pub dump_timestamp: String,
    pub crates_io_commit: String,
    /// Commit of rustsec/advisory-db used, when advisories were loaded.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub advisory_db: String,
    pub advisories: u32,
    pub crates_total: u32,
    pub crates_ranked: u32,
    pub categories: u32,
    pub total_downloads: u64,
    pub source: String,
}
