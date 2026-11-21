//! Kernel version numbers as they appear in kernel.org releases and git tags.

use std::cmp::Ordering;
use std::fmt;

/// `7.4-rc3`, `7.3.5`, `6.18` (mainline names a patch level of zero without it).
#[derive(Clone, Copy, Debug)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: Option<u32>,
    pub release_candidate: Option<u32>,
}

impl Version {
    /// Parses `7.3.5`, `v7.3.5`, `7.4-rc3` and `6.18`; anything else (`next-20261120`) is not a version.
    pub fn parse(text: &str) -> Option<Version> {
        let text = text.strip_prefix('v').unwrap_or(text);
        let (numbers, release_candidate) = match text.split_once("-rc") {
            Some((numbers, candidate)) => (numbers, Some(candidate.parse().ok()?)),
            None => (text, None),
        };
        let mut parts = numbers.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        let patch = match parts.next() {
            Some(patch) => Some(patch.parse().ok()?),
            None => None,
        };
        if parts.next().is_some() || (patch.is_some() && release_candidate.is_some()) {
            return None;
        }
        Some(Version { major, minor, patch, release_candidate })
    }

    /// The git tag of this version in the kernel trees.
    pub fn tag(&self) -> String {
        format!("v{self}")
    }

    pub fn is_release_candidate(&self) -> bool {
        self.release_candidate.is_some()
    }

    /// `major.minor`: the stable and longterm series a version belongs to.
    pub fn series(&self) -> (u32, u32) {
        (self.major, self.minor)
    }

    fn key(&self) -> (u32, u32, u32, u32) {
        (self.major, self.minor, self.patch.unwrap_or(0), self.release_candidate.unwrap_or(u32::MAX))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}", self.major, self.minor)?;
        if let Some(patch) = self.patch {
            write!(formatter, ".{patch}")?;
        }
        if let Some(candidate) = self.release_candidate {
            write!(formatter, "-rc{candidate}")?;
        }
        Ok(())
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Version) -> bool {
        self.key() == other.key()
    }
}

impl Eq for Version {}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Version) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Version) -> Ordering {
        self.key().cmp(&other.key())
    }
}

/// A CachyOS kernel tag: `cachyos-7.3.5-2` (kernel version, then the CachyOS release number).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CachyTag {
    pub version: Version,
    pub release: u32,
}

impl CachyTag {
    pub fn parse(tag: &str) -> Option<CachyTag> {
        let rest = tag.strip_prefix("cachyos-")?;
        let (version, release) = rest.rsplit_once('-')?;
        Some(CachyTag { version: Version::parse(version)?, release: release.parse().ok()? })
    }

    pub fn tag(&self) -> String {
        format!("cachyos-{}-{}", self.version, self.release)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(text: &str) -> Version {
        Version::parse(text).expect("version")
    }

    #[test]
    fn release_candidates_sort_before_their_release() {
        assert!(version("7.4-rc3") < version("7.4"));
        assert!(version("7.4-rc3") > version("7.4-rc2"));
        assert!(version("7.3.5") < version("7.4-rc1"));
        assert!(version("6.18.64") > version("6.18.9"));
        assert_eq!(version("6.18"), version("6.18.0"));
    }

    #[test]
    fn tags_round_trip() {
        for text in ["7.4-rc3", "7.3.5", "6.18"] {
            assert_eq!(version(text).to_string(), text);
            assert_eq!(version(&version(text).tag()), version(text));
        }
        assert!(Version::parse("next-20261120").is_none());
        assert!(Version::parse("7.3.5-rc1").is_none());
    }

    #[test]
    fn cachyos_tags_order_by_version_then_release() {
        let first = CachyTag::parse("cachyos-7.3.5-2").expect("tag");
        assert_eq!(first.version, version("7.3.5"));
        assert_eq!(first.tag(), "cachyos-7.3.5-2");
        assert!(CachyTag::parse("cachyos-7.3.5-10").expect("tag") > first);
        assert!(CachyTag::parse("cachyos-7.4-rc3-1").expect("tag") > first);
        assert!(CachyTag::parse("v7.3.5").is_none());
    }
}
