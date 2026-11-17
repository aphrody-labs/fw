# What the third-party browsers taught us

Each line is something an engine or an upstream actually did in this repository's runs (fr1, 2026-11-17, corpus of 101 pages,
Chrome for Testing 160.0.8164.0 and Obscura 0.2.7), and what was done about it. The runtime part is kept up to date by the tool:
`bun testing/acceptance.ts lessons` (and the dashboard panel) lists, per host and engine, where an engine fell short of the full
one and for how many runs; a lesson disappears when the engine catches up. That table is the data of the router's escalation rule.

## Engines

| Seen | Cause | Handled by |
|---|---|---|
| Chrome headless: 403 on abc.xyz and linkedin.com, 429 on news.google.com, rate-limit page on sec.gov | the sites refuse a headless client; Google answers `/sorry/` | verdict `refused`: the server's answer, never counted against the engine (`[walls]`) |
| Obscura answered 200 to the same Cloudflare challenge ("Just a moment...") and counted as a pass | the status of a challenge differs by engine | `[walls] challenge`: titles, case-insensitive, whatever the status: `refused` |
| Obscura: `Page.navigate` never answers on fi.google.com/about, tasks.google.com, workspace.google.com | engine hang after 30 s | verdict `fail`, lesson `hang`: the router escalates these hosts to Blink |
| Obscura: 104 HTTP errors on the corpus against 29 for Chrome | fewer sub-resources succeed | visible per page (`http_errors`), parity counts elements and text |
| Obscura refuses loopback (SSRF guard); `--allow-private-network` had no effect on 0.2.7, `OBSCURA_ALLOW_PRIVATE_NETWORK=1` did | default policy | the JetStream runner returns the engine's message as `error` |
| Obscura forbids `blob:` URLs, so no JetStream workload runs | missing scheme | clean `error` after the timeout, no process left behind |
| `chrome.exe` on Windows hands over to another process and exits 0 | launcher, not a crash | the launcher exit code 0 is not a death; `bench` timings of such a launcher mean nothing |
| `Bun.WebView` cannot start Chrome or Edge on Windows 11 (`Chrome process closed the pipe`) | runtime issue on that OS; works on Linux | front reported `SKIP`, not a page failure |
| `Bun.connect` TLS exposes the certificate and cipher but no protocol string, ALPN as a boolean | API gap | the negotiated protocol comes from the engine's CDP (`response.protocol`), HTTP/3 verified that way |
| hyperfine 2.x changed its JSON export (`measurements`/`summary`) | schema | `readHyperfine` reads both |
| A second run on a busy host: Obscura hung on 8 pages instead of 3, one Chrome load died on `ERR_NETWORK_CHANGED` | CPU starvation and a network change look like engine failures | `[page] retries`: a transient failure is tried again before it counts; a lesson counts runs, so one run proves little |
| "Before you continue" on 3 Google hosts | consent wall before the page | verdict `consent`, not a failure |

## Third-party sources

| Seen | Cause | Handled by |
|---|---|---|
| `webrender` did not compile (9 files) | `time` 0.3 dropped `precise_time_ns` | one shim in the crate, `tools/fixups.sh` |
| `webrender` "cannot find attribute `serde`" | `serde(skip)` gated on the implicit `serde` feature, the derives on `capture`/`replay` | attributes follow the derives, `tools/fixups.sh` |
| `servo-malloc-size-of`: no `webrender::FastTransform` (next in the chain) | Servo main against webrender's 0.70 line | open |
| `lol_html` (18 errors) and `obscura-dom` (4): `ParserInput`, `PseudoElement::Impl`, `FeaturelessHostMatches` | `[patch.crates-io]` gives them cssparser 0.38 and selectors 0.43 (newest main), they were written for older APIs | open: port the two crates, or give them their registry versions at the price of a duplicate |
| `wrench` second `fontconfig-sys` (`links` conflict) | two crates link the same native library | dropped from the workspace (`tools/fixups.sh`) |
| `surfman` feature `sm-angle-default` gone | upstream rename | rewritten to `sm-angle-builtin` (`tools/fixups.sh`) |
| `hyperfine` fails under the workspace | edition 2018 = resolver 1, workspace = resolver 2: clap features shared | features kept apart (`tools/fixups.sh`) |
| Cherry-picking an import across a directory move conflicts on renames inside the moved tree | rename/rename | the importer writes final paths directly |

## Rules that came out of it

1. A verdict is about what the engine did, not what the server answered: separate `refused`, `auth`, `consent` from `fail`.
2. Never trust a status code for a wall: read the title and the final URL.
3. After any upstream sync, compile every dependent: one version per crate means a newer crate breaks the code written for the
   older one, and the first failing crate hides the next.
4. An engine that hangs is a result, with a timeout and a lesson, never a stuck run.
5. What an engine cannot do is recorded where the router reads it, and forgotten the day it can.
