//! depot_tools in Rust. `deps` reads and resolves DEPS files, `siso` models a ninja/siso build; with the `tools` feature the rest of the
//! tool set follows: `gclient` (sync, revinfo, getdep/setdep, hooks), `cipd`, `fetch`, `autoninja`, `gerrit` and `git_cl`, `presubmit`,
//! `repo_manifest` and `envsetup`. Every tool is a library function first and a `fw depot` subcommand second.

#[cfg(feature = "deps")]
pub mod deps;
#[cfg(feature = "siso")]
pub mod siso;
#[cfg(feature = "siso")]
pub mod model;
#[cfg(feature = "tools")]
pub mod cli;
#[cfg(feature = "tools")]
pub mod error;
#[cfg(feature = "tools")]
pub mod gclient;
#[cfg(feature = "tools")]
pub mod git;
#[cfg(feature = "tools")]
pub mod cipd;
#[cfg(feature = "tools")]
pub mod http;
#[cfg(feature = "tools")]
pub mod sync;
#[cfg(feature = "tools")]
pub mod depedit;
#[cfg(feature = "tools")]
pub mod autoninja;
#[cfg(feature = "tools")]
pub mod fetch;
#[cfg(feature = "tools")]
pub mod gerrit;
#[cfg(feature = "tools")]
pub mod git_cl;
#[cfg(feature = "tools")]
pub mod repo_manifest;
#[cfg(feature = "tools")]
pub mod envsetup;
#[cfg(feature = "tools")]
pub mod presubmit;
