# fw-crates

Crate intelligence over [crates.io](https://crates.io): one engine behind `fw crates` (the `fw-crates` binary that `fw` runs like `fw-files`),
the `fw serve` HTTP and MCP endpoints, and the Bun plugin (`src/crates.ts`). Results are JSON values with an `op` field; `--json` prints
them, `--mdx` renders an MDX page, the default is a text table. Everything is a file: the data is JSON and JSONL under the fw data dir,
the published snapshot is under `docs/crates`.

```text
fw crates search <words...> [--category slug] [-n N]       ranked full-text search
fw crates info <crate> [--offline]                          score components, usage, releases, advisories, ranks, live sparse-index record
fw crates compare <crate> <crate>...                        side by side, best crate per metric
fw crates rank [--category slug] [-n N]                     top crates overall or in a category
fw crates awesome [category] [--write DIR]                  per-category lists (all categories when none is given)
fw crates stack <goal in words> [--out DIR] [--check]       choose crates for a goal, write a Cargo skeleton, cargo check it
fw crates bench <list|web> [--only a,b] [--out DIR] ...     build and load-test a suite (run it on fr1)
fw crates sync [--publish docs/crates]                      download the dump and the advisory-db, rank, write the data
fw crates status                                            where the data is, which dump it is from
```

## Data and policy

| Source | Used for | How it is read |
|---|---|---|
| `https://static.crates.io/db-dump.tar.gz` | crates, versions, downloads (90 days), categories, keywords, dependencies, owners | one streaming pass, never stored; columns by header name; join order = archive order |
| `https://index.crates.io` | live latest version in `info` (`fw info cargo`'s resolver in fw-apm) | one request, throttled |
| rustsec/advisory-db (GitHub tarball at the latest commit) | open advisories, unmaintained and unsound notices | front-matter TOML, `semver` requirements |

The crawler policy is followed: a User-Agent naming the client (`fw-crates/<version> (generic fw client; +https://github.com/aphrody-labs/fw)`),
at most one request per second (`http::throttle`), the dump for bulk data, no crates.io API crawling.

Working data (`ranked.jsonl`, all ranked crates) lives in `FW_CRATES_HOME`, else `<FX_HOME>/crates`, else `~/.fx/crates`. Without it the commands read the
committed snapshot in `docs/crates` (found from `FW_ROOT` or the current directory upward): `index.jsonl`, `meta.json`, `categories.json`,
`awesome/*.json|mdx`, `RANKING.md`. Every number names the dump it came from (`dump_date` in every result).

## Ranking

`src/rank.rs` is the method; `docs/crates/RANKING.md` is rendered from the same constants. The reference time is the dump's timestamp, so one dump
always yields the same scores (the pipeline test syncs twice and compares the bytes).

## HTTP and MCP

`fw serve` answers `GET /v1/crates/<search|info|compare|rank|awesome|stack>[/<arg>]?<args>` with JSON and lists five MCP tools in `/mcp`
(`crates_search`, `crates_info`, `crates_compare`, `crates_awesome`, `crates_stack`). The Bun plugin MCP server (`fw_cli.ts mcp`) exposes the same five
by running this binary; `tests/plugin_parity.rs` keeps the two declarations identical.

## Benchmarks

`bench/<category>/suite.toml` plus one source file per contestant. `web` serves `/hello` and `/json` from axum, actix-web, salvo, poem, rocket, warp, ntex
and may-minihttp, built in one release workspace and driven by `oha`. `tools/crates/bench-fr1.ts` runs it on fr1 with the servers and the load generator on
disjoint CPU sets and copies `docs/crates/bench/<category>/*.json|mdx` back. A new category (serializers, HTTP clients, async runtimes) is a new directory
and one line in `bench.rs`.

## Stack generator

`stack.toml` holds roles (pools of candidate crates), profiles (which roles a kind of project needs), add-ons (technologies named in the goal) and per-crate
hints (features, starter code). The choice among candidates is the ranking; the output lists the winner, why, and the alternatives, then writes a project
whose `rust-version` is the highest MSRV among the picks.

## Tests

```sh
cargo test -p fw-crates                                   # fixtures only, no network
cargo test -p fw-crates --no-default-features --features sync
cargo clippy -p fw-crates --all-targets -- -D warnings
```
