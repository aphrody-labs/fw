//! Corpus ingest for `fw learn`: every source is one SQLite FTS5 database under the corpus root.
//! Adapters live in their own files (`sitemap`, `gitiles`, `github`, `rustdoc`, `render`) and share the
//! polite HTTP client (`http`), the chunker (`chunk`) and the store (`store`).

pub mod chunk;
pub mod github;
pub mod gitiles;
pub mod html;
pub mod http;
pub mod render;
pub mod rustdoc;
pub mod sitemap;
pub mod sources;
pub mod store;

use std::path::PathBuf;

/// Corpus root: `--root`, `FW_CORPUS_ROOT`, `/srv/xbuild/corpus` (fleet XFS) when present, else `~/corpus`.
pub fn resolve_root(flag: Option<&str>) -> PathBuf {
    if let Some(path) = flag {
        return PathBuf::from(path);
    }
    if let Ok(path) = std::env::var("FW_CORPUS_ROOT")
        && !path.trim().is_empty()
    {
        return PathBuf::from(path);
    }
    let xfs = PathBuf::from("/srv/xbuild/corpus");
    if xfs.is_dir() {
        return xfs;
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
    home.unwrap_or_else(|| PathBuf::from(".")).join("corpus")
}

pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

/// ISO-8601 UTC from a unix timestamp (civil-from-days, no time crate).
pub fn iso(timestamp: i64) -> String {
    let days = timestamp.div_euclid(86_400);
    let seconds = timestamp.rem_euclid(86_400);
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 { month_index + 3 } else { month_index - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        seconds / 3600,
        seconds % 3600 / 60,
        seconds % 60
    )
}

/// Decode the HTML entities that appear in sitemaps and gitiles listings.
pub fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let Some(end) = rest.find(';').filter(|end| *end <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix('#')
                .and_then(|number| match number.strip_prefix(['x', 'X']) {
                    Some(hex) => u32::from_str_radix(hex, 16).ok(),
                    None => number.parse().ok(),
                })
                .and_then(char::from_u32),
        };
        match decoded {
            Some(character) => {
                out.push(character);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_epoch_and_leap() {
        assert_eq!(iso(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso(1_709_164_800), "2024-02-29T00:00:00Z");
    }

    #[test]
    fn unescape_entities() {
        assert_eq!(unescape("a &amp; b &#39;c&#x21; &bogus d"), "a & b 'c! &bogus d");
    }
}
