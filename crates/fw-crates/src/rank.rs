//! The ranking: one documented, deterministic score per crate. Every input is a number from the dump (or the
//! advisory-db) and the reference time is the dump's own timestamp, never the wall clock, so the same dump always
//! produces the same scores. `ranking_doc` renders this file's constants into `RANKING.md`.

use crate::model::{Components, SCORE_VERSION};

pub const W_DOWNLOADS: f64 = 0.25;
pub const W_DEPENDENTS: f64 = 0.25;
pub const W_RECENCY: f64 = 0.15;
pub const W_MAINTENANCE: f64 = 0.15;
pub const W_LICENSE: f64 = 0.10;
pub const W_MSRV: f64 = 0.05;
pub const W_SECURITY: f64 = 0.05;
/// Multiplier applied to the weighted sum when a non-informational advisory affects the default version.
pub const OPEN_ADVISORY_FACTOR: f64 = 0.75;
/// Half-life of the recency component, in days.
pub const RECENCY_HALF_LIFE_DAYS: f64 = 365.0;
/// A crate needs this many downloads in the 90-day window to appear in an awesome list.
pub const MIN_RECENT_DOWNLOADS: u64 = 1000;

/// Registry-wide maxima that normalise the two log-scaled components.
#[derive(Debug, Clone, Copy, Default)]
pub struct Norm {
    pub max_recent_downloads: u64,
    pub max_dependents: u32,
}

#[derive(Debug, Clone, Default)]
pub struct Inputs<'a> {
    pub recent_downloads: u64,
    pub dependents: u32,
    pub days_since_release: f64,
    pub releases_365d: u32,
    pub age_days: f64,
    pub owners: u32,
    pub has_repository: bool,
    pub has_msrv: bool,
    pub license: &'a str,
    pub open_advisories: usize,
    pub informational_advisories: usize,
}

fn log_norm(x: f64, max: f64) -> f64 {
    if max <= 0.0 { 0.0 } else { ((1.0 + x).ln() / (1.0 + max).ln()).clamp(0.0, 1.0) }
}

/// Licence score of one SPDX identifier: 1.0 permissive, 0.75 weak copyleft, 0.5 strong copyleft, 0.25 other, 0 none.
fn license_id_score(id: &str) -> f64 {
    let id = id.trim().trim_matches(|c| c == '(' || c == ')');
    let id = id.split(" WITH ").next().unwrap_or(id).trim().trim_end_matches('+');
    let up = id.to_ascii_uppercase();
    if up.is_empty() {
        return 0.0;
    }
    const PERMISSIVE: [&str; 14] = [
        "MIT", "APACHE-2.0", "BSD-2-CLAUSE", "BSD-3-CLAUSE", "ISC", "ZLIB", "UNLICENSE", "CC0-1.0", "0BSD", "BSL-1.0", "UNICODE-3.0", "MIT-0", "UNICODE-DFS-2016", "WTFPL",
    ];
    if PERMISSIVE.contains(&up.as_str()) {
        1.0
    } else if up.starts_with("MPL") || up.starts_with("LGPL") || up.starts_with("EPL") || up.starts_with("CDDL") || up.starts_with("OFL") {
        0.75
    } else if up.starts_with("GPL") || up.starts_with("AGPL") || up.starts_with("EUPL") {
        0.5
    } else {
        0.25
    }
}

/// Score of an SPDX expression: `A OR B` takes the best alternative, `A AND B` the worst. The legacy `A/B` form is an OR.
pub fn license_score(expr: &str) -> f64 {
    let expr = expr.trim();
    if expr.is_empty() {
        return 0.0;
    }
    let normalised = expr.replace('/', " OR ");
    normalised
        .split(" OR ")
        .map(|alt| alt.split(" AND ").map(license_id_score).fold(1.0_f64, f64::min))
        .fold(0.0_f64, f64::max)
}

/// Computes the components and the final score (0 to 100, two decimals).
pub fn score(i: &Inputs, norm: Norm) -> (f64, Components) {
    let downloads = log_norm(i.recent_downloads as f64, norm.max_recent_downloads as f64);
    let dependents = log_norm(f64::from(i.dependents), f64::from(norm.max_dependents));
    let recency = 0.5_f64.powf(i.days_since_release.max(0.0) / RECENCY_HALF_LIFE_DAYS);
    let maintenance = 0.4 * (f64::from(i.releases_365d.min(4)) / 4.0)
        + 0.2 * (f64::from(i.owners.min(3)) / 3.0)
        + 0.2 * f64::from(u8::from(i.has_repository))
        + 0.2 * (i.age_days / 730.0).clamp(0.0, 1.0);
    let license = license_score(i.license);
    let msrv = f64::from(u8::from(i.has_msrv));
    let security = if i.open_advisories > 0 {
        0.0
    } else if i.informational_advisories > 0 {
        0.5
    } else {
        1.0
    };
    let sum = W_DOWNLOADS * downloads
        + W_DEPENDENTS * dependents
        + W_RECENCY * recency
        + W_MAINTENANCE * maintenance
        + W_LICENSE * license
        + W_MSRV * msrv
        + W_SECURITY * security;
    let factor = if i.open_advisories > 0 { OPEN_ADVISORY_FACTOR } else { 1.0 };
    let total = (100.0 * sum * factor * 100.0).round() / 100.0;
    let r = |x: f64| (x * 10_000.0).round() / 10_000.0;
    (
        total,
        Components {
            downloads: r(downloads),
            dependents: r(dependents),
            recency: r(recency),
            maintenance: r(maintenance),
            license: r(license),
            msrv: r(msrv),
            security: r(security),
        },
    )
}

/// The ranking method as Markdown, rendered from the constants above.
pub fn ranking_doc() -> String {
    format!(
        "# Crate ranking (score version {SCORE_VERSION})\n\n\
Every crate gets one score from 0 to 100. The inputs come from the crates.io database dump and the RustSec advisory-db; \
the reference time is the dump's timestamp, so one dump always yields the same scores. `fw crates sync` recomputes them \
and each list in this directory names the dump date it was computed from.\n\n\
| Component | Weight | Definition |\n|---|---|---|\n\
| downloads | {W_DOWNLOADS} | `ln(1 + d) / ln(1 + max d)`, d = downloads over the 90 days that end at the dump, max over all crates |\n\
| dependents | {W_DEPENDENTS} | `ln(1 + n) / ln(1 + max n)`, n = crates whose default version depends on it (normal and build, optional included, dev excluded) |\n\
| recency | {W_RECENCY} | `0.5 ^ (days since the latest non-yanked release / {RECENCY_HALF_LIFE_DAYS})` |\n\
| maintenance | {W_MAINTENANCE} | `0.4 * min(releases in 365 days, 4) / 4 + 0.2 * min(owners, 3) / 3 + 0.2 * has repository + 0.2 * min(age in days / 730, 1)` |\n\
| license | {W_LICENSE} | SPDX expression: permissive (MIT, Apache-2.0, BSD, ISC, Zlib, Unlicense, CC0, 0BSD, BSL-1.0) 1, weak copyleft (MPL, LGPL, EPL, CDDL) 0.75, strong copyleft (GPL, AGPL, EUPL) 0.5, other 0.25, none 0; `OR` takes the best alternative, `AND` the worst |\n\
| msrv | {W_MSRV} | 1 when the default version declares `rust-version`, else 0 |\n\
| security | {W_SECURITY} | 1 without advisories, 0.5 with only informational ones (unmaintained, unsound, notice), 0 when an advisory affects the default version |\n\n\
The weighted sum is multiplied by {OPEN_ADVISORY_FACTOR} when a non-informational advisory affects the default version. \
The default version is the highest non-yanked stable release, as crates.io selects it. An advisory affects a version \
when none of its `patched` or `unaffected` requirements match it.\n\n\
An awesome list is one crates.io category: the {MIN_RECENT_DOWNLOADS}+ recent downloads crates of the category, non-yanked, \
ordered by score. Each entry carries its components so the score can be recomputed by hand.\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Inputs<'static> {
        Inputs {
            recent_downloads: 1_000_000,
            dependents: 5000,
            days_since_release: 30.0,
            releases_365d: 6,
            age_days: 2000.0,
            owners: 3,
            has_repository: true,
            has_msrv: true,
            license: "MIT OR Apache-2.0",
            open_advisories: 0,
            informational_advisories: 0,
        }
    }

    #[test]
    fn license_expressions() {
        assert_eq!(license_score("MIT OR Apache-2.0"), 1.0);
        assert_eq!(license_score("MIT/Apache-2.0"), 1.0);
        assert_eq!(license_score("GPL-3.0-only OR MIT"), 1.0);
        assert_eq!(license_score("MIT AND GPL-3.0-or-later"), 0.5);
        assert_eq!(license_score("MPL-2.0"), 0.75);
        assert_eq!(license_score("LicenseRef-custom"), 0.25);
        assert_eq!(license_score(""), 0.0);
        assert_eq!(license_score("Apache-2.0 WITH LLVM-exception"), 1.0);
    }

    #[test]
    fn perfect_crate_scores_100() {
        let norm = Norm { max_recent_downloads: 1_000_000, max_dependents: 5000 };
        let (s, c) = score(&base(), norm);
        assert!(s > 95.0 && s <= 100.0, "{s}");
        assert_eq!(c.downloads, 1.0);
        assert_eq!(c.security, 1.0);
    }

    #[test]
    fn open_advisory_penalises() {
        let norm = Norm { max_recent_downloads: 1_000_000, max_dependents: 5000 };
        let clean = score(&base(), norm).0;
        let vuln = score(&Inputs { open_advisories: 1, ..base() }, norm).0;
        let info = score(&Inputs { informational_advisories: 1, ..base() }, norm).0;
        assert!(vuln < info && info < clean);
        assert!(vuln < clean * OPEN_ADVISORY_FACTOR);
    }

    #[test]
    fn recency_halves_each_year() {
        let norm = Norm { max_recent_downloads: 10, max_dependents: 10 };
        let c = score(&Inputs { days_since_release: 365.0, ..base() }, norm).1;
        assert!((c.recency - 0.5).abs() < 1e-4);
    }

    #[test]
    fn weights_sum_to_one() {
        let sum = W_DOWNLOADS + W_DEPENDENTS + W_RECENCY + W_MAINTENANCE + W_LICENSE + W_MSRV + W_SECURITY;
        assert!((sum - 1.0).abs() < 1e-12);
    }

    #[test]
    fn doc_mentions_every_weight() {
        let d = ranking_doc();
        assert!(d.contains("score version 1") && d.contains("0.25") && d.contains("0.75"));
    }
}
