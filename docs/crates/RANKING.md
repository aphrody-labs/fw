# Crate ranking (score version 1)

Every crate gets one score from 0 to 100. The inputs come from the crates.io database dump and the RustSec advisory-db; the reference time is the dump's timestamp, so one dump always yields the same scores. `fw crates sync` recomputes them and each list in this directory names the dump date it was computed from.

| Component | Weight | Definition |
|---|---|---|
| downloads | 0.25 | `ln(1 + d) / ln(1 + max d)`, d = downloads over the 90 days that end at the dump, max over all crates |
| dependents | 0.25 | `ln(1 + n) / ln(1 + max n)`, n = crates whose default version depends on it (normal and build, optional included, dev excluded) |
| recency | 0.15 | `0.5 ^ (days since the latest non-yanked release / 365)` |
| maintenance | 0.15 | `0.4 * min(releases in 365 days, 4) / 4 + 0.2 * min(owners, 3) / 3 + 0.2 * has repository + 0.2 * min(age in days / 730, 1)` |
| license | 0.1 | SPDX expression: permissive (MIT, Apache-2.0, BSD, ISC, Zlib, Unlicense, CC0, 0BSD, BSL-1.0) 1, weak copyleft (MPL, LGPL, EPL, CDDL) 0.75, strong copyleft (GPL, AGPL, EUPL) 0.5, other 0.25, none 0; `OR` takes the best alternative, `AND` the worst |
| msrv | 0.05 | 1 when the default version declares `rust-version`, else 0 |
| security | 0.05 | 1 without advisories, 0.5 with only informational ones (unmaintained, unsound, notice), 0 when an advisory affects the default version |

The weighted sum is multiplied by 0.75 when a non-informational advisory affects the default version. The default version is the highest non-yanked stable release, as crates.io selects it. An advisory affects a version when none of its `patched` or `unaffected` requirements match it.

An awesome list is one crates.io category: the 1000+ recent downloads crates of the category, non-yanked, ordered by score. Each entry carries its components so the score can be recomputed by hand.
