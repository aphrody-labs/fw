# browser: agent contract

One Cargo workspace laid out like Chromium: `components/` (first-party crates), `third_party/` (Chromium's checkout on the build host only;
every other upstream is pinned in `tools/upstreams.toml` / `tools/upstreams.lock.json` and forks in `tools/layout.toml` [forks]), `build/`, `testing/`, `tools/`, `styleguide/`, `docs/`, `DEPS`, `PRESUBMIT.ts`. One `Cargo.lock`, one version per crate.
Where a thing lives is decided by [tools/layout.toml](tools/layout.toml) and nowhere else.
Decision record: [ARCHITECTURE.md](ARCHITECTURE.md).

## Rules

- Never build on a workstation: Rust and GN builds run on a build host (which one: private config, `bun testing/acceptance.ts telemetry`).
- No published V8, Skia or Blink bindings or prebuilts: engines come from the Chromium checkout (`build/engines`).
- No second copy of a crate: new code goes to the crate that owns the function; a vendored copy of an imported
  crate is deleted in the same commit.
- A fork with a build system of its own (Bun, kitesurf, browse, mullvad) is pinned by commit in `tools/layout.toml` [forks] and built in
  its own checkout; it is never a submodule or an import here (one checkout per fork across the workspace).
- First party in `components/<crate>` (a manifest with `[package.metadata.browser] native = true`), new crates named `browser-<unit>`;
  upstream crates come through Cargo from their pinned revision, never copied into the tree.
- JavaScript and TypeScript are native Bun only ([styleguide/bun](styleguide/bun/README.md), proof from the fork's docs; rules in
  `bunfig.toml`); `bun PRESUBMIT.ts` runs every check before a push.
- Newest of everything (canary, nightly, rc), read live; pins live in `tools/upstreams.lock.json`.
- Commits, reviews and org-wide rules: [aphrody-labs/.github](https://github.com/aphrody-labs/.github) (`docs/standards/commit-format.md`, `CONTRIBUTING.md`); not repeated here.

## Test and measure

One tool, [testing/acceptance.ts](testing/acceptance.ts) (Bun only; data in `testing/acceptance.toml`): engines x live corpus over raw CDP,
hyperfine and JetStream benchmarks, upstream tracking, the live dashboard. Commands and flags: the header of the file.

```sh
bun PRESUBMIT.ts                   # types, lint, format, tests, layout, README.chromium, Bun-native, docs proof
bun check && bun lint tools testing && bun test testing tools    # the same, one by one
bun testing/acceptance.ts run        # on the build host for anything beyond a few pages
bun testing/acceptance.ts lessons    # where each engine fell short of the full one: the router's data
```

## Pin an upstream

Every upstream in `tools/upstreams.toml` is `mode = "pin"`: no tree on disk, Cargo fetches it from the pinned revision.

```sh
python3 tools/import.py . . <name>        # record the current branch head of the upstream in tools/upstreams.lock.json
bun testing/acceptance.ts track           # how far each pin is behind its upstream
```

## Gotchas

- `bun fmt <directory>` also reformats data (lock, reports, TOML): pass explicit `.ts` paths.
- Tests are hermetic; the live or heavier ones are opt-in by environment: `TRACK_LIVE=1` (upstream APIs), `JETSTREAM_WS=<cdp ws>` (a real JetStream
  run), `REDIS_URL=redis://host:port` (the Redis/Valkey paths).
- The presubmit's docs check reads the Bun fork's docs: `BUN_DOCS=<fork>/docs`, else the sibling `../bun/docs`.

## Private state, never written here

Machine, host, account and work facts (measurements, lessons, build failures that are not regressions, quirks of a runtime version, how
a host is reached) do not belong in tracked docs or agent files. They live in the git-ignored `.local/` and `.claude.local.md`, generated from
the store by `bun testing/acceptance.ts telemetry` and `docs`; add what you learn with `bun testing/acceptance.ts note "<text>" --kind gotcha`.
`bun PRESUBMIT.ts` fails when a tracked file names a host path, a user directory or a private host.

## Layout

| Path | Content |
|---|---|
| `tools/layout.toml` [forks] | bun, kitesurf, browse, mullvad (repo and commit per fork), not checked out here |
| `components/<crate>` | first-party crates: browser-detect (with shared `ssrf` guard), browser-index, browser-engine (`pick`, `escalate`, `KITESURF_CRATES`, `process`), browser-vpn (fused `ayvpn` per-context router & ABI v1), browser-browse (absorbed `anomalyco/browse` pairing, CDP pool, ARIA `@e1` snapshots, WebMCP, human handoff), browser-views (Chromium `//ui/views` model for GPUI & headless), browser-headless (headless contexts over the engines), browser-chromium (CEF from Rust and Bun; downloads CEF, so not a default member) |
| `tools/upstreams.toml` | tracked upstreams, all `mode = "pin"` (SHAs in `tools/upstreams.lock.json`), no tree on disk |
| `third_party/chromium` | the Chromium checkout of the build host (gclient), ignored by git |
| `testing` | the acceptance tool: engines x live corpus, benchmarks, dashboard (Chromium's `//testing`) |
| `styleguide/bun` | the JS/TS rules with their proof |
| `DEPS` | what is depended on without being imported (the Chromium checkout, pinned); imports are pinned in `tools/upstreams.lock.json` |
| `build/engines` | GN overlay and script: V8, Skia (static archive) and Blink (headless_shell) from the Chromium checkout |
| `tools` | layout check, pin recorder (`import.py`), upstream list and lock, README.chromium generator, kitesurf absorb and monorepo generators |
