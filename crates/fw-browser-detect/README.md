# browser-detect

Deterministic technology and anti-bot detection from HTTP and DNS evidence.

- `detect_response(DetectionInput)` needs no network: it matches response headers, body markers and
  resolved addresses / CNAME / NS records against native rules (CDNs, servers, frameworks, CMS, analytics)
  and recognises bot-challenge pages (`Challenge`, `ChallengeProvider`, `Confidence`, `Recommendation`).
- `suggest_strategy` turns a detection into a scrape plan (`Strategy`, `ScrapeProfile`, `WaitFor`).
- `detect_url` (feature `fetch`, on by default) fetches a bounded response (`MAX_BODY_BYTES`) and resolves
  A/AAAA records before calling the same core.

Legacy published name: `bun_browser_detect` (crates.io, 1.4.8). This crate is the restored source under the
repository's own name; it is not published.

## Test

```sh
cargo test -p browser-detect
cargo clippy -p browser-detect --all-targets -- -D warnings
```
