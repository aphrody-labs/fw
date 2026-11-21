# Prompt Assets

This directory owns first-party prompt segment metadata and preset layouts.
Current Niubash does not load them from the bundle; its own tables pin the
same segment ids and presets (see `docs/compatibility.md`).

- `segments.toml` maps exported segment names, such as `cwd` and `git`, to
  native Niubash segment IDs.
- `[presets.<name>]` tables define segment order, right-prompt elements,
  separators, and optional git prompt formatting.

## The `git` segment is live (niubash 1.4.1)

`[segments.git] id = "vcs"` resolves to niubash's native `SegmentId::Vcs`
(niubash#331). The host renders it in-process through git2 - branch, `branch*`
when the worktree is dirty, short SHA on a detached HEAD, empty outside a
repository - with **zero child processes** on the prompt path; no `git.exe`
fork per redraw (that is the PS1-embedded `$(...)` pattern niubash#604
retired).

Colours follow the theme contract every first-party theme already declares:
`[git_clean]` cyan, `[git_dirty]` yellow.

Preset `lean` (`[presets.lean]` in `segments.toml`) is `cwd`, `git`, `newline`,
`prompt_char`. Every shipped preset (`lean`, `classic`, `rainbow`, `pure`,
`robbyrussell`) already carries `git` in its left prompt.
