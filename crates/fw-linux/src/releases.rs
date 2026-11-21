//! The kernel.org releases feed (`releases.json`).

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result, read_to_string};
use crate::version::Version;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Release {
    pub moniker: String,
    pub version: String,
    #[serde(default)]
    pub iseol: bool,
    #[serde(default)]
    pub released: Option<String>,
}

impl Release {
    pub fn parsed(&self) -> Option<Version> {
        Version::parse(&self.version)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Releases {
    pub source: String,
    pub latest_stable: Option<String>,
    pub releases: Vec<Release>,
}

#[derive(Deserialize)]
struct RawFeed {
    #[serde(default)]
    latest_stable: Option<RawLatest>,
    releases: Vec<RawRelease>,
}

#[derive(Deserialize)]
struct RawLatest {
    version: String,
}

#[derive(Deserialize)]
struct RawRelease {
    moniker: String,
    version: String,
    #[serde(default)]
    iseol: bool,
    #[serde(default)]
    released: Option<RawReleased>,
}

#[derive(Deserialize)]
struct RawReleased {
    #[serde(default)]
    isodate: Option<String>,
}

impl Releases {
    /// Reads the feed from a URL, a `file://` URL or a plain path.
    pub fn fetch(source: &str) -> Result<Releases> {
        let body = match source.strip_prefix("file://") {
            Some(path) => read_to_string(std::path::Path::new(path))?,
            None if source.starts_with("http://") || source.starts_with("https://") => http_get(source)?,
            None => read_to_string(std::path::Path::new(source))?,
        };
        Releases::parse(source, &body)
    }

    pub fn parse(source: &str, body: &str) -> Result<Releases> {
        let feed: RawFeed = serde_json::from_str(body)?;
        Ok(Releases {
            source: source.to_owned(),
            latest_stable: feed.latest_stable.map(|latest| latest.version),
            releases: feed
                .releases
                .into_iter()
                .map(|raw| Release { moniker: raw.moniker, version: raw.version, iseol: raw.iseol, released: raw.released.and_then(|released| released.isodate) })
                .collect(),
        })
    }

    pub fn by_moniker<'a>(&'a self, moniker: &'a str) -> impl Iterator<Item = &'a Release> {
        self.releases.iter().filter(move |release| release.moniker == moniker)
    }

    pub fn mainline(&self) -> Option<&Release> {
        self.by_moniker("mainline").next()
    }

    pub fn linux_next(&self) -> Option<&Release> {
        self.by_moniker("linux-next").next()
    }

    /// The newest release of a moniker, or of one series of it.
    pub fn newest<'a>(&'a self, moniker: &'a str, series: Option<(u32, u32)>) -> Option<&'a Release> {
        self.by_moniker(moniker)
            .filter(|release| series.is_none_or(|wanted| release.parsed().is_some_and(|version| version.series() == wanted)))
            .max_by_key(|release| release.parsed())
    }
}

pub fn http_get(url: &str) -> Result<String> {
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(30))).build().into();
    let mut response = agent.get(url).header("User-Agent", "fw-linux").call().map_err(|error| Error::Http { url: url.to_owned(), detail: error.to_string() })?;
    response.body_mut().read_to_string().map_err(|error| Error::Http { url: url.to_owned(), detail: error.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FEED: &str = r#"{"latest_stable":{"version":"7.3.5"},"releases":[
        {"moniker":"mainline","version":"7.4-rc3","iseol":false,"released":{"timestamp":1,"isodate":"2026-11-15"}},
        {"moniker":"stable","version":"7.3.5","iseol":false,"released":{"timestamp":1,"isodate":"2026-11-17"}},
        {"moniker":"stable","version":"7.2.15","iseol":false,"released":null},
        {"moniker":"longterm","version":"6.18.64","iseol":false},
        {"moniker":"longterm","version":"6.12.118","iseol":false},
        {"moniker":"linux-next","version":"next-20261120","iseol":false}]}"#;

    #[test]
    fn newest_per_moniker_and_series() {
        let releases = Releases::parse("test", FEED).expect("feed");
        assert_eq!(releases.latest_stable.as_deref(), Some("7.3.5"));
        assert_eq!(releases.mainline().map(|r| r.version.as_str()), Some("7.4-rc3"));
        assert_eq!(releases.newest("stable", None).map(|r| r.version.as_str()), Some("7.3.5"));
        assert_eq!(releases.newest("stable", Some((7, 2))).map(|r| r.version.as_str()), Some("7.2.15"));
        assert_eq!(releases.newest("longterm", Some((6, 12))).map(|r| r.version.as_str()), Some("6.12.118"));
        assert_eq!(releases.newest("longterm", Some((5, 4))), None);
        assert_eq!(releases.linux_next().map(|r| r.version.as_str()), Some("next-20261120"));
    }
}
