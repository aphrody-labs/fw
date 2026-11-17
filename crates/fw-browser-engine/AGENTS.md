# browser: agent contract

One Cargo workspace laid out like Chromium: `components/` (first-party crates), `third_party/` (everything that is not ours: one
directory per imported upstream, and the Bun runtime as a submodule), `build/`, `testing/`, `tools/`, `styleguide/`, `docs/`, `DEPS`, `PRESUBMIT.ts`. One `Cargo.lock`, one version per crate.
Where a thing lives is decided by [tools/layout.toml](tools/layout.toml) and nowhere else.
Decision record: [ARCHITECTURE.md](ARCHITECTURE.md).

## Rules

- Never build on a workstation: Rust and GN builds run on the build host (`ssh fr1`, `/srv/xbuild/scratch`).
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
python3 tools/import.py . /srv/xbuild/scratch/imp <name>     # fetch, rewrite to final paths, replay new commits linearly (no merge commit)
uv run --with tomlkit tools/flatten.py . --root-only          # members and [patch.crates-io] follow the lock (full run: only for a crate just imported)
bash tools/fixups.sh && bun tools/third_party.ts              # integration fixes, README.chromium
cargo update && cargo tree -d                                 # dedup check; who requires which version: cargo metadata --format-version 1 | python3 tools/directdups.py
```

## Gotchas

- `bun fmt <directory>` also reformats data (lock, reports, TOML): pass explicit `.ts` paths; the edit hook does.
- `tools/flatten.py` rewrites the members list from the lock, so `fixups.sh` (which drops obscura-*, wrench, webrender-examples) must run after it; the
  crate manifests are already flattened, so `--root-only` is the normal run.
- Pre-existing build failures, not regressions: `lol_html` and `obscura-dom` (cssparser 0.38 / selectors 0.43 against code written for older APIs),
  `servo-malloc-size-of` (`webrender::FastTransform`). Detail and handled cases: `docs/third-party-lessons.md`.
- Compare acceptance runs only on a quiet build host (`uptime`): a busy one made Obscura hang on 8 pages instead of 3; `[page] retries` absorbs some of it.
- `scp` does not work from this PC's shell: `tar cf - dir | ssh fr1 'tar xf - -C /tmp/x'`. A shared checkout with someone else's unstaged
  changes cannot `pull --rebase` (no stash): `git fetch && git merge --ff-only origin/main`, then push.
- The presubmit's docs check reads the Bun fork's docs: `BUN_DOCS=<fork>/docs`, else `third_party/bun/docs`, else `../bun/docs`.

## Layout

| Path | Content |
|---|---|
| `third_party/bun` | the Bun runtime: a git submodule of aphrody-labs/bun (relative URL), its own workspace and build (`git submodule update --init third_party/bun`); change it in the fork, bump the pointer here |
| `components/<crate>` | first-party crates: browser-detect, browser-index, browser-process, browser-engine (the engine catalog with `pick` and `escalate`) |
| `third_party/<upstream>/<crate>` | Servo and its org crates, Mozilla, HarfBuzz and platform bindings, Obscura, agent-browser, lol-html, argon2; the upstream's other files beside them; `README.chromium` |
| `third_party/chromium` | the Chromium checkout of the build host (gclient), ignored by git |
| `testing` | the acceptance tool: engines x live corpus, benchmarks, dashboard (Chromium's `//testing`) |
| `styleguide/bun` | the JS/TS rules with their proof |
| `DEPS` | what is depended on without being imported (the Chromium checkout, pinned); imports are pinned in `tools/upstreams.lock.json` |
| `build/engines` | GN overlay and script: V8, Skia (static archive) and Blink (headless_shell) from the Chromium checkout |
| `tools` | layout, importer, flattener, upstream list and lock, patches |
