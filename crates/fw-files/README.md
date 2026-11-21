# fw-files

File manager engine distilled from [yazi](https://github.com/sxyazi/yazi) (MIT, fork `aphrody-labs/yazi`), and the
`fw-files` binary that `fw files <command>` runs (the way `fw convert` runs `fw-video`). Every command takes paths
as arguments or on stdin and writes JSON lines, so it composes with `jq`, `fw tool` and the agent runtime.

```text
fw files ls <dir|sftp://[user@]host[:port]/dir> [--sort natural|alpha|mtime|size|ext|mime|none] [-r] [-a] [--mime] [--names]
fw files mime [path...]            no paths or `-`: one path per line on stdin; prints mime and the yazi previewer
fw files preview <path|sftp://...> [--lines N] [--bytes N]
fw files cp|mv <src>... <dst> [--jobs N] [--on-conflict fail|skip|overwrite|rename]
fw files rm <path>... [--jobs N]
fw files rules [previewers|preloaders|spotters|fetchers]
```

`cp`, `mv` and `rm` stream `queued`, `collected`, `progress`, `error` and `finished` events (one task per source,
`--jobs` workers, yazi's `tasks.file_workers = 3` by default) and exit non-zero unless every task finished `done`.

## What came from yazi

| Piece | yazi origin (v26.9.1) | Here |
|---|---|---|
| Rule table: previewers, preloaders, spotters, fetchers, `file_workers` | `yazi-config/preset/yazi-default.toml` | `presets/yazi-plugin.toml`, an excerpt kept verbatim; `rules.rs` matches it with yazi's glob syntax |
| MIME normalisation (`x-` and `vnd.` dropped so `application/vnd.rar` matches `rar`) | `yazi-plugin/preset/plugins/mime-local.lua` | `mime.rs`; detection is in-process (`infer`, `mime_guess`, text sniff) instead of `file --mime-type` |
| Previewer tools per type | the `Command(...)` calls in `yazi-plugin/preset/plugins/*.lua` | `preview::external_tools` |
| Sort modes, natural order, dirs first | `yazi-fs` sorter | `sort.rs` |
| File metadata (`Cha`) | `yazi-fs/src/cha` | `entry.rs` |
| Task queue with per-kind workers, collect then run, progress | `yazi-scheduler` | `tasks.rs` |
| SFTP client | `yazi-sftp` (crate used as is) | `sftp.rs` (feature `sftp`, default) |

`yazi-sftp` is a git dependency on the fork at tag `v26.9.1`, because crates.io stops at 26.5.6 for it (see below) and
the tag matches the rest of the pinned yazi state in `upstreams.toml`. Connection data comes from `ssh -G <host>` and the
host key is checked against `known_hosts` (`--accept-new` records an unknown key); yazi's own SFTP engine accepts any key.

## Survey of the yazi workspace

Read on 2026-11-21: latest release `v26.9.1` (published 2026-09-01), default branch `main`, licence MIT. Versions and
dates are from the crates.io API; "26.9.1" means that tag exists on crates.io. All crates in the workspace are MIT.

| Crate | crates.io | What it does | Usable as a library? | fw part that would gain | Decision |
|---|---|---|---|---|---|
| `yazi-sftp` | 26.5.6 | Async SFTP client (fork of russh-sftp: non-UTF-8 paths, nlink, owner) | **Yes**: depends on russh, tokio, typed-path, bitflags, hashbrown, parking_lot, serde only | `fw files`, upcoming `fw-drive` remote providers | **Taken** (`sftp` feature) |
| `yazi-fs` | 26.5.6 | `Cha`, sorting, `Url`/path types, file engines, trash, xdg, mounts | No: depends on `mlua` and `yazi-binding` | fw-bin | Ideas ported (`entry.rs`, `sort.rs`) |
| `yazi-shared` | 26.5.6 | URLs, strands, ids, natural sort, event bus | No: `mlua`, `yazi-term` | none | Skipped |
| `yazi-scheduler` | 26.5.6 | Task queue: file, fetch, preload, process, plugin workers with priorities | No: `yazi-config`, `yazi-dds`, `yazi-runner`, `yazi-term`, `yazi-vfs`, `mlua` | agent tooling, `fw-drive` jobs | Design ported (`tasks.rs`) |
| `yazi-config` | 26.5.6 | `yazi.toml`, `keymap.toml`, `theme.toml`, `vfs.toml`, flavors | No: `mlua`, ratatui | fw-bin | Rule table taken as data (`presets/yazi-plugin.toml`) |
| `yazi-plugin`, `yazi-runner`, `yazi-binding` | 26.5.6 | Lua runtime, preset plugins (code, json, image, video, pdf, archive, font, svg, magick, zoxide, fzf), Lua bindings | No: they are the Lua runtime | none | Skipped; tools recorded |
| `yazi-adapter` | 26.5.6 | Image protocols: kitty unicode placeholders and old protocol, iTerm2 inline images, sixel, Ueberzug++, chafa | No: `yazi-config`, `yazi-emulator`, `yazi-tty`, `yazi-widgets`, ratatui | Rio terminal, TUI previews | Skipped; protocol facts in the `yazi` skill |
| `yazi-dds` | 26.5.6 | Pub/sub between yazi instances over a local socket (`ya pub`, `ya emit`, static `@` messages persist) | No: `yazi-boot`, `yazi-binding`, `mlua` | cross-session bus | Skipped; fw has its own bus |
| `yazi-vfs` | 26.5.6 | Virtual filesystem engines: local, sftp, lua | No: `yazi-config`, `yazi-runner` | `fw-drive` | Skipped; SFTP through `yazi-sftp` |
| `yazi-watcher` | 26.5.6 | `notify`-based directory watching | No: `yazi-vfs`, `yazi-dds` | none | Skipped |
| `yazi-term`, `yazi-emulator`, `yazi-tty` | 26.5.6, 26.5.6, 26.9.1 | Virtual terminal, terminal capability database, TTY access | No: `mlua`, `yazi-shim` | Rio terminal | Skipped |
| `yazi-shim`, `yazi-ffi`, `yazi-macro`, `yazi-codegen` | 26.9.1 | Internal shims, platform FFI, macros, proc macros | Internal only | none | Skipped |
| `yazi-actor`, `yazi-core`, `yazi-parser`, `yazi-proxy`, `yazi-boot`, `yazi-widgets`, `yazi-fm`, `yazi-cli`, `yazi-build`, `yazi-packing` | 26.5.6 (`yazi-build` 26.9.1) | Application layer: actions, state, forms, the TUI, `yazi` and `ya` binaries | No | none | Skipped |
| `yazi-tui`, `yazi-version` | not on crates.io | TUI glue, version info | n/a | none | Skipped |

MIME detection is not a crate: yazi runs `file -bL --mime-type` (override with `YAZI_FILE_ONE`), which does not exist on
Windows, so `fw-files` detects in process.

yazi docs (`https://yazi-rs.github.io/docs/`, version 26.9.1) worth knowing: `configuration/vfs` (SFTP providers
registered as `[sftp.<name>]` with `host`, `user`, `port`, `key_file`, `key_passphrase`, `password`, `identity_agent`),
`dds` (pub/sub and `ya emit`), `image-preview` (terminal protocol table), `plugins/*` (Lua API), `flavors/overview`,
`dnd`, `term`, `tips`.

## Tests

`cargo test -p fw-files` runs offline: unit tests for every module (SFTP URL and `ssh -G` parsing and attribute mapping
included, no connection) and `tests/cli.rs`, which drives the binary on temporary directories.

## Licence

fw-files is MIT like the rest of fw. `presets/yazi-plugin.toml` is an excerpt of yazi (MIT, Copyright (c) 2023 - sxyazi);
see `THIRD_PARTY_NOTICES.md` at the repository root.
