---
name: yazi
description: "yazi (sxyazi/yazi, Rust terminal file manager, fork aphrody-labs/yazi) as used by fw: fw files (ls, mime. Use when working on file listing, MIME or preview logic, file-operation task queues, SFTP"
---
> preview, cp, mv, rm, rules over JSON lines, sftp:// paths), which yazi crates are usable as libraries and which are not, yazi's previewer rule table, image preview protocols, DDS, VFS providers. , or porting something from yazi.


# yazi in fw

Pinned state: release `v26.9.1` (published 2026-09-01), MIT, default branch `main`. Fork: `aphrody-labs/yazi`.
Tracked in `upstreams.toml` (id `yazi`, target `crates/fw-files`). Survey table with crates.io versions: `crates/fw-files/README.md`.

## fw files

`fw files` runs the sibling binary `fw-files` (`cargo build --release -p fw-files`, keep it next to `fw`). Paths in, JSON lines out.

| Command | Output |
|---|---|
| `fw files ls <dir\|sftp://host/dir> [--sort natural\|alpha\|mtime\|size\|ext\|mime\|none] [-r] [-a] [--mime] [--names]` | one entry per line: `path name kind size mtime_ms hidden readonly executable link broken mime` |
| `fw files mime [path...]` (stdin when none) | `{path, mime, previewer}` per path |
| `fw files preview <path\|sftp://...> [--lines N] [--bytes N]` | `{mime, previewer, kind: text\|json\|folder\|hex\|empty\|external, lines, truncated, tools}` |
| `fw files cp\|mv <src>... <dst> [--jobs N] [--on-conflict fail\|skip\|overwrite\|rename]` | events `queued`, `collected`, `progress`, `error`, `finished` |
| `fw files rm <path>...` | same events |
| `fw files rules [previewers\|preloaders\|spotters\|fetchers]` | yazi's rule table, one rule per line |

SFTP: `--identity`, `--passphrase-env VAR`, `--accept-new`. The target is resolved with `ssh -G <host>` (aliases work), the host key is
checked against `known_hosts`. Moves rename on one filesystem and copy then delete across; directories merge on conflict.

Code map (`crates/fw-files/src`): `entry.rs` metadata, `sort.rs`, `mime.rs`, `rules.rs` (+ `presets/yazi-plugin.toml`, verbatim from
`yazi-config/preset/yazi-default.toml`), `preview.rs`, `tasks.rs` (worker pool), `sftp.rs`. Tests: `cargo test -p fw-files` (offline).

## What yazi offers, and what is reusable

- Only `yazi-sftp` stands alone (no Lua). Every other `yazi-*` crate pulls `mlua` through `yazi-shared`, `yazi-shim`, `yazi-binding` or `yazi-term`,
  so port ideas, do not depend on them. crates.io lags the tag: most crates are at 26.5.6, only `yazi-build`, `yazi-codegen`, `yazi-ffi`, `yazi-macro`,
  `yazi-shim`, `yazi-tty` have 26.9.1; `yazi-tui` and `yazi-version` are not published. Re-check with the crates.io API before changing the pin.
- MIME: yazi runs `file -bL --mime-type` (env `YAZI_FILE_ONE`) and strips `x-` and `vnd.` from the subtype; rules match the normalised type.
  Directories are `folder/local` (`folder/remote` for sftp, `trash/folder/local`), empty files `inode/empty`.
- Previewers by name: `folder`, `code`, `json` (jq), `image`, `magick` (magick), `svg` (resvg), `video` (ffmpeg, ffprobe), `pdf` (pdftoppm),
  `archive` (7zz or 7z), `font` (magick), `empty`, `vfs`, `trash`, `null`, fallback `file`.
- Image preview protocols, chosen from `$TERM`, `$TERM_PROGRAM`, `$XDG_SESSION_TYPE`: kitty unicode placeholders (kitty >= 0.28, Ghostty),
  kitty old protocol (Konsole), iTerm2 inline images (iTerm2, WezTerm, Warp, Tabby, VSCode, Bobcat), sixel (foot, Windows Terminal >= 1.22, st, Black Box),
  Ueberzug++ on X11 or Wayland, chafa as ASCII fallback. tmux needs `allow-passthrough on`. Rio draws wrong sizes through the inline protocol.
- Scheduler (`yazi-scheduler`, config `[tasks]`): `file_workers 3`, `plugin_workers 5`, `fetch_workers 5`, `preload_workers 2`, `process_workers 5`,
  `bizarre_retry 3`, `image_alloc` 512 MiB, `image_bound` 10000x10000. File tasks collect first, then copy or move with progress; cross-device moves copy then delete.
- DDS (`yazi-dds`): pub/sub across instances without a server process, `ya pub`, `ya pub-to`, `ya emit`, stdout reporting; kinds starting with `@` persist.
- VFS (`vfs.toml`): `[sftp.<name>]` with `host`, `user`, `port`, `key_file`, `key_passphrase`, `password`, `identity_agent`; addressed as `sftp://<name>/...`.
  yazi's engine accepts any host key; fw does not.

Docs: https://yazi-rs.github.io/docs/ (`configuration/yazi`, `configuration/vfs`, `plugins/overview`, `dds`, `image-preview`, `flavors/overview`).
