//! Terminal file manager engine distilled from [yazi](https://github.com/sxyazi/yazi) (MIT).
//!
//! yazi's crates are welded to its Lua runtime, so they cannot be used as a library; what is reusable is
//! its design, which lives here as small Rust modules that take paths and produce serde values:
//!
//! * [`entry`]: file metadata (`Cha` in yazi) and directory listing,
//! * [`sort`]: yazi's sort modes with natural ordering,
//! * [`mime`]: MIME detection by content, then extension, then text sniffing, normalised the way yazi does,
//! * [`rules`]: yazi's own `[plugin]` rule table (previewers, preloaders, spotters, fetchers) as data,
//! * [`preview`]: a structured preview per previewer (text, JSON, folder, hex, external tool hints),
//! * [`tasks`]: a worker-pool scheduler for copy, move and remove with progress events (yazi-scheduler),
//! * `sftp` (feature `sftp`): remote listing and reading through `yazi-sftp`, the one yazi crate that stands alone.

pub mod entry;
pub mod mime;
pub mod preview;
pub mod rules;
pub mod sort;
pub mod tasks;

#[cfg(feature = "sftp")]
pub mod sftp;
