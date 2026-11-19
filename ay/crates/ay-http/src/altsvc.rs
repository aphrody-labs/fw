//! `Alt-Svc` (RFC 7838) parsing and the per-origin cache that drives the h3 upgrade of the client.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Default freshness of an alternative service when the header has no `ma` parameter (RFC 7838 section 3.1).
pub const DEFAULT_MAX_AGE: Duration = Duration::from_secs(86_400);

/// How long an origin whose QUIC attempt failed is kept on TCP.
pub const BROKEN_BACKOFF: Duration = Duration::from_secs(300);

/// One parsed alternative service entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alternative {
    /// ALPN protocol id, for example `h3`.
    pub protocol: String,
    /// Alternative host, `None` when the authority is only a port (`":443"`: same host).
    pub host: Option<String>,
    /// Alternative port.
    pub port: u16,
    /// Freshness lifetime.
    pub max_age: Duration,
}

/// The value of an `Alt-Svc` header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AltSvc {
    /// `clear`: forget every alternative of the origin.
    Clear,
    /// The advertised alternatives, in header order.
    Alternatives(Vec<Alternative>),
}

/// Parses one `Alt-Svc` header value. Unparseable entries are skipped.
#[must_use]
pub fn parse(value: &str) -> AltSvc {
    let value = value.trim();
    if value.eq_ignore_ascii_case("clear") {
        return AltSvc::Clear;
    }
    let mut out = Vec::new();
    for entry in split_entries(value) {
        let mut parts = entry.split(';').map(str::trim);
        let Some((protocol, authority)) = parts.next().and_then(|first| first.split_once('=')) else {
            continue;
        };
        let authority = authority.trim().trim_matches('"');
        let Some((host, port)) = authority.rsplit_once(':') else { continue };
        let Ok(port) = port.parse::<u16>() else { continue };
        let mut max_age = DEFAULT_MAX_AGE;
        for param in parts {
            if let Some((name, v)) = param.split_once('=')
                && name.trim().eq_ignore_ascii_case("ma")
                && let Ok(secs) = v.trim().trim_matches('"').parse::<u64>()
            {
                max_age = Duration::from_secs(secs);
            }
        }
        out.push(Alternative {
            protocol: protocol.trim().to_owned(),
            host: (!host.is_empty()).then(|| host.to_owned()),
            port,
            max_age,
        });
    }
    AltSvc::Alternatives(out)
}

/// Splits on commas that are outside double quotes.
fn split_entries(value: &str) -> impl Iterator<Item = &str> {
    let mut start = 0;
    let mut quoted = false;
    let mut cut = Vec::new();
    for (i, c) in value.char_indices() {
        match c {
            '"' => quoted = !quoted,
            ',' if !quoted => {
                cut.push((start, i));
                start = i + 1;
            }
            _ => {}
        }
    }
    cut.push((start, value.len()));
    cut.into_iter().map(move |(a, b)| value[a..b].trim()).filter(|s| !s.is_empty())
}

/// An origin: scheme, host and port.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Origin {
    host: String,
    port: u16,
}

impl Origin {
    /// The origin of an https URL; `None` for any other scheme or a URL without a host.
    #[must_use]
    pub fn of_https(url: &reqwest::Url) -> Option<Self> {
        (url.scheme() == "https").then_some(())?;
        Some(Self { host: url.host_str()?.to_ascii_lowercase(), port: url.port_or_known_default()? })
    }
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    expires: Instant,
    broken_until: Option<Instant>,
}

/// Origins that advertised an h3 service on their own port, with their expiry.
///
/// Only same-host, same-port alternatives are used: the pool and TLS name stay those of the origin.
#[derive(Debug, Default)]
pub struct AltSvcCache {
    entries: Mutex<HashMap<Origin, Entry>>,
}

impl AltSvcCache {
    /// Records the `Alt-Svc` headers of a response from `origin`.
    pub fn observe(&self, origin: &Origin, headers: &reqwest::header::HeaderMap) {
        let mut saw = false;
        let mut best: Option<Duration> = None;
        let mut clear = false;
        for value in headers.get_all(reqwest::header::ALT_SVC) {
            let Ok(value) = value.to_str() else { continue };
            saw = true;
            match parse(value) {
                AltSvc::Clear => clear = true,
                AltSvc::Alternatives(list) => {
                    for alt in list {
                        if alt.protocol == "h3" && alt.host.is_none() && alt.port == origin.port {
                            best = Some(best.map_or(alt.max_age, |b| b.max(alt.max_age)));
                        }
                    }
                }
            }
        }
        if !saw {
            return;
        }
        let Ok(mut entries) = self.entries.lock() else { return };
        match (clear, best) {
            (_, Some(max_age)) => {
                let broken_until = entries.get(origin).and_then(|e| e.broken_until);
                entries.insert(origin.clone(), Entry { expires: Instant::now() + max_age, broken_until });
            }
            (true, None) => {
                entries.remove(origin);
            }
            (false, None) => {}
        }
    }

    /// Whether the next request to `origin` should try h3.
    #[must_use]
    pub fn h3_usable(&self, origin: &Origin) -> bool {
        let Ok(mut entries) = self.entries.lock() else { return false };
        let now = Instant::now();
        match entries.get(origin) {
            Some(e) if e.expires <= now => {
                entries.remove(origin);
                false
            }
            Some(e) => e.broken_until.is_none_or(|until| until <= now),
            None => false,
        }
    }

    /// Marks the QUIC path of `origin` as failing; it stays on TCP for [`BROKEN_BACKOFF`].
    pub fn mark_broken(&self, origin: &Origin) {
        if let Ok(mut entries) = self.entries.lock()
            && let Some(e) = entries.get_mut(origin)
        {
            e.broken_until = Some(Instant::now() + BROKEN_BACKOFF);
        }
    }

    /// Seeds an origin as h3 capable (used when the deployment already knows, and by the tests).
    pub fn insert(&self, origin: Origin, max_age: Duration) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.insert(origin, Entry { expires: Instant::now() + max_age, broken_until: None });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_h3_with_max_age_and_ignores_other_protocols() {
        let AltSvc::Alternatives(list) = parse(r#"h3=":443"; ma=3600, h3-29=":443", h2="alt.example.com:8443""#) else {
            panic!("alternatives expected")
        };
        assert_eq!(list.len(), 3);
        assert_eq!(list[0], Alternative { protocol: "h3".into(), host: None, port: 443, max_age: Duration::from_secs(3600) });
        assert_eq!(list[2].host.as_deref(), Some("alt.example.com"));
        assert_eq!(list[2].max_age, DEFAULT_MAX_AGE);
    }

    #[test]
    fn clear_and_garbage() {
        assert_eq!(parse("clear"), AltSvc::Clear);
        assert_eq!(parse("nonsense"), AltSvc::Alternatives(vec![]));
        assert_eq!(parse(r#"h3="x:notaport""#), AltSvc::Alternatives(vec![]));
    }

    #[test]
    fn cache_follows_header_clear_and_breakage() {
        let cache = AltSvcCache::default();
        let url = reqwest::Url::parse("https://example.com/").unwrap();
        let origin = Origin::of_https(&url).unwrap();
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(reqwest::header::ALT_SVC, r#"h3=":443"; ma=60"#.parse().unwrap());
        assert!(!cache.h3_usable(&origin));
        cache.observe(&origin, &headers);
        assert!(cache.h3_usable(&origin));
        cache.mark_broken(&origin);
        assert!(!cache.h3_usable(&origin));
        headers.insert(reqwest::header::ALT_SVC, "clear".parse().unwrap());
        cache.observe(&origin, &headers);
        assert!(!cache.h3_usable(&origin));
    }

    #[test]
    fn other_port_alternatives_are_not_used() {
        let cache = AltSvcCache::default();
        let origin = Origin::of_https(&reqwest::Url::parse("https://example.com/").unwrap()).unwrap();
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(reqwest::header::ALT_SVC, r#"h3=":8443""#.parse().unwrap());
        cache.observe(&origin, &headers);
        assert!(!cache.h3_usable(&origin));
    }
}
