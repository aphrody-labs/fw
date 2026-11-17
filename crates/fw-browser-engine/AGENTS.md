# browser: agent contract

One Cargo workspace laid out like Chromium: `components/` (first-party crates), `third_party/` (everything that is not ours: one
directory per imported upstream, and the Bun runtime as a submodule), `build/`, `testing/`, `tools/`, `styleguide/`, `docs/`, `DEPS`, `PRESUBMIT.ts`. One `Cargo.lock`, one version per crate.
Where a thing lives is decided by [tools/layout.toml](tools/layout.toml) and nowhere else.
Decision record: [ARCHITECTURE.md](ARCHITECTURE.md).

## Rules

- Never build on a workstation: Rust and GN builds run on a build host (which one: private config, `bun testing/acceptance.ts telemetry`).
- No published V8, Skia or Blink bindings or prebuilts: engines come from the Chromium checkout (`build/engines`).
- No second copy of a crate: new code goes to the crate that owns the function; a vendored copy of an imported
  crate is deleted in the same commit.
- A fork with a build system of its own is a submodule (relative URL, one checkout), never an import: `third_party/bun`.
- First party in `components/<crate>` (a manifest with `[package.metadata.browser] native = true`), new crates named `browser-<unit>`;
  third party only in `third_party/<upstream>/`, never edited in place beyond `tools/flatten.py`, `tools/fixups.sh` and `tools/patches/`.
- Upstream code stays importable: do not hand-edit imported manifests, change `tools/flatten.py` instead.
- JavaScript and TypeScript are native Bun only ([styleguide/bun](styleguide/bun/README.md), proof from the fork's docs; rules in
  `bunfig.toml`); `bun PRESUBMIT.ts` runs every check before a push.
- Every `third_party/<name>/` has a generated `README.chromium` (`bun tools/third_party.ts`); `main` requires a linear history.
- Newest of everything (canary, nightly, rc), read live; pins live in `tools/upstreams.lock.json`.

## Test and measure

One tool, [testing/acceptance.ts](testing/acceptance.ts) (Bun only; data in `testing/acceptance.toml`): engines x live corpus over raw CDP,
hyperfine and JetStream benchmarks, upstream tracking, the live dashboard. Usage and rules: `.claude/skills/acceptance/SKILL.md`.

```sh
bun PRESUBMIT.ts                   # types, lint, format, tests, layout, README.chromium, Bun-native, docs proof
bun check && bun lint tools testing .claude && bun test testing    # the same, one by one
bun testing/acceptance.ts run        # on the build host for anything beyond a few pages
bun testing/acceptance.ts lessons    # where each engine fell short of the full one: the router's data
git submodule update --init third_party/bun   # only when Bun itself is needed (build/test it in its own checkout: `bun bd`)
```

## Sync an upstream (history preserved)

```sh
python3 tools/import.py . <scratch>/imp <name>     # fetch, rewrite to final paths, replay new commits linearly (no merge commit)
uv run --with tomlkit tools/flatten.py . --root-only          # members and [patch.crates-io] follow the lock (full run: only for a crate just imported)
bash tools/fixups.sh && bun tools/third_party.ts              # integration fixes, README.chromium
cargo update && cargo tree -d                                 # dedup check; who requires which version: cargo metadata --format-version 1 | python3 tools/directdups.py
```

## Gotchas

- `bun fmt <directory>` also reformats data (lock, reports, TOML): pass explicit `.ts` paths; the edit hook does.
- `tools/flatten.py` rewrites the members list from the lock, so `fixups.sh` (which drops obscura-*, wrench, webrender-examples) must run after it; the
  crate manifests are already flattened, so `--root-only` is the normal run.
- Tests are hermetic; the live or heavier ones are opt-in by environment: `TRACK_LIVE=1` (upstream APIs), `JETSTREAM_WS=<cdp ws>` (a real JetStream
  run), `REDIS_URL=redis://host:port` (the Redis/Valkey paths).
- The presubmit's docs check reads the Bun fork's docs: `BUN_DOCS=<fork>/docs`, else `third_party/bun/docs`, else `../bun/docs`.

## Private state, never written here

Machine, host, account and work facts (measurements, lessons, build failures that are not regressions, quirks of a runtime version, how
a host is reached) do not belong in tracked docs or agent files. They live in the git-ignored `.local/` and `.claude.local.md`, generated from
the store by `bun testing/acceptance.ts telemetry` and `docs`; add what you learn with `bun testing/acceptance.ts note "<text>" --kind gotcha`.
`bun PRESUBMIT.ts` fails when a tracked file names a host path, a user directory or a private host.

## Layout

| Path | Content |
|---|---|
| `third_party/bun` | the Bun runtime: a git submodule of aphrody-labs/bun (relative URL), its own workspace and build (`git submodule update --init third_party/bun`); change it in the fork, bump the pointer here |
| `third_party/kitesurf` | Cloudflare's Rust browser engine (`aphrody-labs/kitesurf`, submodule at `0e080e0988aaecd62ccdde88d4c67d8ac8acfd2f`): owns the 26 `browser-*` Rust engine crates (Blitz, Stylo, Taffy, Parley, Vello, Anyrender, Boa) behind CDP |
| `third_party/browse` | real-browser MCP server and Chromium extension (`aphrody-labs/anomalyco-browse`, submodule) |
| `third_party/mullvad` | WireGuard/DAITA/MASQUE tunnel stack (`aphrody-labs/mullvadvpn-app`, submodule) fused with `ayvpn` |
| `components/<crate>` | first-party crates: browser-detect (with shared `ssrf` guard), browser-index, browser-engine (`pick`, `escalate`, `KITESURF_CRATES`, `process`), browser-vpn (fused `ayvpn` per-context router & ABI v1), browser-browse (absorbed `anomalyco/browse` pairing, CDP pool, ARIA `@e1` snapshots, WebMCP, human handoff), browser-views (Chromium `//ui/views` model for GPUI & headless) |
| `third_party/<upstream>/<crate>` | Servo and its org crates, Mozilla, HarfBuzz and platform bindings, Obscura, agent-browser, lol-html, argon2; the upstream's other files beside them; `README.chromium` |
| `third_party/chromium` | the Chromium checkout of the build host (gclient), ignored by git |
| `testing` | the acceptance tool: engines x live corpus, benchmarks, dashboard (Chromium's `//testing`) |
| `styleguide/bun` | the JS/TS rules with their proof |
| `DEPS` | what is depended on without being imported (the Chromium checkout, pinned); imports are pinned in `tools/upstreams.lock.json` |
| `build/engines` | GN overlay and script: V8, Skia (static archive) and Blink (headless_shell) from the Chromium checkout |
| `tools` | layout, importer, flattener, upstream list and lock, patches |
