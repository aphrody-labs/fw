//! yazi's `[plugin]` rule table as data. `presets/yazi-plugin.toml` is an excerpt of
//! `yazi-config/preset/yazi-default.toml` at the pinned tag; rules are `{ mime | url, run }` pairs matched
//! top to bottom with yazi's glob syntax (`*`, `?`, `**`, `{a,b}`).

use globset::{GlobBuilder, GlobMatcher};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

const PRESET: &str = include_str!("../presets/yazi-plugin.toml");

/// fw additions tried before yazi's rules: text formats that yazi gets as `text/*` from `file(1)` but
/// that extension lookup reports as `application/*`.
const FW_PREVIEWERS: &[(&str, &str)] = &[("application/{toml,yaml,xml,sql,sh,csh,x-sh,x-yaml,x-ndjson}", "code")];

#[derive(Debug, Deserialize)]
struct RawRule {
    mime: Option<String>,
    url: Option<String>,
    run: String,
    prio: Option<String>,
    group: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawPreset {
    tasks: RawTasks,
    plugin: RawPlugin,
}

#[derive(Debug, Deserialize)]
struct RawTasks {
    file_workers: usize,
}

#[derive(Debug, Deserialize)]
struct RawPlugin {
    fetchers: Vec<RawRule>,
    spotters: Vec<RawRule>,
    preloaders: Vec<RawRule>,
    previewers: Vec<RawRule>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Rule {
    pub mime: Option<String>,
    pub url: Option<String>,
    pub run: String,
    pub prio: Option<String>,
    pub group: Option<String>,
    #[serde(skip)]
    mime_glob: Option<GlobMatcher>,
    #[serde(skip)]
    url_glob: Option<GlobMatcher>,
}

fn glob(pattern: &str) -> Result<GlobMatcher, globset::Error> {
    Ok(GlobBuilder::new(pattern).literal_separator(false).build()?.compile_matcher())
}

impl Rule {
    fn new(raw: RawRule) -> Result<Self, globset::Error> {
        Ok(Self {
            mime_glob: raw.mime.as_deref().map(glob).transpose()?,
            url_glob: raw.url.as_deref().map(glob).transpose()?,
            mime: raw.mime,
            url: raw.url,
            run: raw.run,
            prio: raw.prio,
            group: raw.group,
        })
    }

    fn matches(&self, mime: &str, url: &str) -> bool {
        match (&self.mime_glob, &self.url_glob) {
            (Some(m), _) => m.is_match(mime),
            (None, Some(u)) => u.is_match(url),
            (None, None) => false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RuleSet {
    pub rules: Vec<Rule>,
}

impl RuleSet {
    /// The `run` of the first rule matching. `mime` is the detected type (directories use
    /// `inode/directory`, mapped here to yazi's `folder/local`), `url` the path with a trailing `/` for directories.
    pub fn run(&self, mime: &str, url: &str) -> Option<&str> {
        let mime = if mime == crate::mime::DIRECTORY { "folder/local" } else { mime };
        self.rules.iter().find(|r| r.matches(mime, url)).map(|r| r.run.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct Preset {
    pub fetchers: RuleSet,
    pub spotters: RuleSet,
    pub preloaders: RuleSet,
    pub previewers: RuleSet,
    /// yazi's `tasks.file_workers`: concurrent file operations.
    pub file_workers: usize,
}

fn build(raw: Vec<RawRule>, extra: &[(&str, &str)]) -> Result<RuleSet, globset::Error> {
    let mut rules = Vec::new();
    for (mime, run) in extra {
        rules.push(Rule::new(RawRule { mime: Some((*mime).into()), url: None, run: (*run).into(), prio: None, group: None })?);
    }
    for r in raw {
        rules.push(Rule::new(r)?);
    }
    Ok(RuleSet { rules })
}

/// The parsed preset. The embedded file is checked by tests, so a parse failure is a build defect.
pub fn preset() -> &'static Preset {
    static PRESET_CELL: OnceLock<Preset> = OnceLock::new();
    PRESET_CELL.get_or_init(|| {
        let raw: RawPreset = toml::from_str(PRESET).expect("presets/yazi-plugin.toml parses");
        Preset {
            fetchers: build(raw.plugin.fetchers, &[]).expect("fetcher globs"),
            spotters: build(raw.plugin.spotters, &[]).expect("spotter globs"),
            preloaders: build(raw.plugin.preloaders, &[]).expect("preloader globs"),
            previewers: build(raw.plugin.previewers, FW_PREVIEWERS).expect("previewer globs"),
            file_workers: raw.tasks.file_workers.max(1),
        }
    })
}

pub fn previewer(mime: &str, url: &str) -> &'static str {
    preset().previewers.run(mime, url).unwrap_or("file")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_parses_with_yazi_counts() {
        let p = preset();
        assert_eq!(p.file_workers, 3);
        assert_eq!(p.fetchers.rules.len(), 4);
        assert!(p.previewers.rules.len() > 20);
        assert!(p.preloaders.rules.iter().any(|r| r.run == "pdf"));
    }

    #[test]
    fn mime_globs_follow_yazi_rules() {
        assert_eq!(previewer("text/x-rust", "a.rs"), "code");
        assert_eq!(previewer("application/json", "a.json"), "json");
        assert_eq!(previewer("image/svg+xml", "a.svg"), "svg");
        assert_eq!(previewer("image/heic", "a.heic"), "magick");
        assert_eq!(previewer("image/png", "a.png"), "image");
        assert_eq!(previewer("video/mp4", "a.mp4"), "video");
        assert_eq!(previewer("application/pdf", "a.pdf"), "pdf");
        assert_eq!(previewer("application/zip", "a.zip"), "archive");
        assert_eq!(previewer("application/7z-compressed", "a.7z"), "archive");
        assert_eq!(previewer("font/ttf", "a.ttf"), "font");
        assert_eq!(previewer("inode/empty", "a"), "empty");
    }

    #[test]
    fn directories_and_url_rules() {
        assert_eq!(previewer("inode/directory", "src/"), "folder");
        assert_eq!(previewer("application/octet-stream", "tool.AppImage"), "archive");
        assert_eq!(previewer("application/octet-stream", "disk.squashfs"), "archive");
    }

    #[test]
    fn fallback_is_file_and_fw_additions_apply() {
        assert_eq!(previewer("application/octet-stream", "blob"), "file");
        assert_eq!(previewer("application/toml", "Cargo.toml"), "code");
    }
}
