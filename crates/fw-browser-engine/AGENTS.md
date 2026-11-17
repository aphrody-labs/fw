# browser: agent contract

One Cargo workspace, Bun layout: every crate is `src/<package>`, one `Cargo.lock`, one version per crate.
Decision record: [ARCHITECTURE.md](ARCHITECTURE.md). Report: [docs/dedup.md](docs/dedup.md).

## Rules

- Never build on a workstation: Rust and GN builds run on the build host (`ssh fr1`, `/srv/xbuild/scratch`).
- No published V8, Skia or Blink bindings or prebuilts: engines come from the Chromium checkout (`build/engines`).
- No second copy of a crate: new code goes to the crate that owns the function; a vendored copy of an imported
  crate is deleted in the same commit.
- Upstream code stays importable: do not hand-edit imported manifests, change `tools/flatten.py` instead.
- Newest of everything (canary, nightly, rc), read live; pins live in `tools/upstreams.lock.json`.

## Test and measure

One tool, [tools/acceptance.ts](tools/acceptance.ts) (Bun only; data in `tools/acceptance.toml`): engines x live corpus over raw CDP,
hyperfine and JetStream benchmarks, upstream tracking, the live dashboard. Usage and rules: `.claude/skills/acceptance/SKILL.md`.

```sh
bun check -p tools && bun lint tools/acceptance.ts tools/acceptance && bun test tools/acceptance   # native gates
bun tools/acceptance.ts run        # on the build host for anything beyond a few pages
```

## Sync an upstream (history preserved)

```sh
python3 tools/import.py . /srv/xbuild/scratch/imp <name>     # fetch, rewrite, merge new commits only
uv run --with tomlkit tools/flatten.py .                      # re-flatten manifests, patches, root workspace
cargo update && cargo tree -d                                 # dedup check
```

## Layout

| Path | Content |
|---|---|
| `src/<package>` | every crate (Bun, Servo and its org crates, Obscura, agent-browser) |
| `upstream/<name>` | non-crate files of each upstream: CI, docs, resources, tests |
| `build/engines` | GN overlay and script: V8, Skia (static archive) and Blink (headless_shell) from the Chromium checkout |
| `tools` | importer, flattener, upstream list and lock, patches, benchmarks, acceptance probes |
