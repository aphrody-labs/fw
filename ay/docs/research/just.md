# just: the command runner and its ecosystem

Retrieved 2026-11-16 from live sources (gh api, crates.io, npm, PyPI, JetBrains Marketplace, the repository at tag 1.59.0, the local `just` binary). Every row in [just.toml](just.toml) carries its source. Anything not read from a source is listed under `unverified` in the dataset.

## Summary

- `just` 1.59.0 was released on 2026-10-15 (CHANGELOG date 2026-10-14). It is CC0-1.0, written in Rust (edition 2024, MSRV 1.89.0), with 36,720 stars, 172 open issues and 0 open pull requests. Source: <https://api.github.com/repos/casey/just>, <https://crates.io/api/v1/crates/just>.
- Aphrody already uses it: the legacy `ay` repo has 157 recipes across 23 imported `.just` files, and `shenron` (62), `nie` (41), `rosegriffon` (41) and `ay/apps/nt` (21) have their own. All of them list and dump cleanly with just 1.59.0.
- Recommendation: adopt `just` as the single task entry for `ay-next`, with one-line recipes that call `tools/*.ts` and `cargo`. Keep all logic in Bun typed scripts. Do not fork `casey/just` or `tree-sitter-just`.

## casey/just

| Fact | Value | Source |
|---|---|---|
| Latest release | 1.59.0, 2026-10-15; 20 releases between 1.44.0 (2025-12-07) and 1.59.0 | <https://api.github.com/repos/casey/just/releases> |
| crates.io downloads | 2,972,593 all time, 566,525 in the last 90 days, 112,018 for 1.59.0 | <https://crates.io/api/v1/crates/just> |
| Licence | CC0-1.0 | `Cargo.toml` at 1.59.0 |
| Repository | 36,720 stars, 943 forks, 172 open issues, 0 open PRs, 213 contributors | <https://api.github.com/repos/casey/just> |
| MSRV | `rust-version = "1.89.0"`; the README policy is current stable | <https://raw.githubusercontent.com/casey/just/1.59.0/Cargo.toml> |
| Features | None (`[features]` is absent) | same |
| Dependencies | 32 direct runtime crates, `nix` on Unix, `ctrlc` on Windows, 2 dev crates; 155 packages in `Cargo.lock` | same |
| Release profile | `lto = true`, `codegen-units = 1` | same |

Notable direct dependencies: `clap` 4 with `clap_complete` pinned at `=4.6.11` (dynamic completions), `blake3` (recipe cache and `blake3()`), `regex`, `serde_json` (`--dump-format json`), `pulldown-cmark` (markdown justfiles), `dotenvy`, `chrono`, `snafu`. The full list with versions is in the dataset.

### Install channels

- Prebuilt archives for 10 targets: Linux musl (x86_64, aarch64, armv7, arm, riscv64gc, loongarch64), macOS (x86_64, aarch64), Windows MSVC (x86_64, aarch64), plus `SHA256SUMS`.
- `curl --proto '=https' --tlsv1.2 -sSf https://just.systems/install.sh | bash -s -- --to DEST`; pass `--tag` on shared CI addresses.
- `cargo install just`, `cargo binstall just`, `ghcr.io/casey/just` (Docker), a Nix flake, nixpkgs, winget `Casey.Just`, scoop, choco, brew, MacPorts, apk, pacman, apt (Debian trixie), dnf, zypper, xbps, FreeBSD pkg, OpenBSD pkg_add, conda-forge, asdf, spack, snap.
- Wrappers that are not published by the author: npm `rust-just` 1.59.0 (290,364 downloads in 30 days), PyPI `rust-just` 1.59.0 (about 2.57 million downloads in the last month; `pipx install rust-just`, `uv tool install rust-just`), and npm `just-install` 2.0.2 (last published 2024-07-31, so it can lag).

### Language

- Recipes with positional parameters, defaults and variadics; prior and subsequent dependencies; `[parallel]` capped by `--jobs`; shebang and `[script]` recipes; `--long` and `-s` options and flags per recipe through `[arg(...)]` (1.46.0 to 1.56.0).
- `mod` modules (stable since 1.31.0), `import` and `import?` (single global namespace), module aliases (1.55.0).
- Attributes: `[private]`, `[script]`, `[parallel]`, `[group]`, `[confirm]`, `[working-directory]`, `[default]`, `[doc]`, `[env]`, `[no-cd]`, `[timestamp]`, `[continue]`, `[shell]`, `[metadata]`, plus platform gates `[linux]`, `[macos]`, `[windows]`, `[unix]`, the BSDs and `[android]`.
- Settings: `shell`, `windows-shell`, `dotenv-*` (including `dotenv-command` since 1.54.0), `export`, `fallback`, `minimum-version` (1.55.0), `no-cd`, `working-directory`, `default-list`, `default-script`, `indentation`, `lazy`, `guards`, `positional-arguments`, `quiet`, `tempdir`.
- Built-in functions cover system information, environment, `which()` and `require()`, path and string manipulation, case conversion, hashing (`blake3`, `sha256`), `datetime`, `semver_matches`, `style()` and user directories. The README is the reference: <https://just.systems/man/en/>.
- Command line: `--list`, `--groups`, `--summary`, `--usage`, `--show`, `--evaluate`, `--dump --dump-format json` (also `--json`), `--fmt` and `--fmt --check`, `--choose` (default chooser `fzf`), `--completions` for bash, elvish, fish, nushell, powershell and zsh, `--command`, `--dry-run`, `--yes`, `--time`, `--clean`.
- Unstable in 1.59.0 (need `--unstable`, `set unstable` or `JUST_UNSTABLE`): `[cache]` cached recipes, `set lists` and user-defined functions. Source: `src/unstable_feature.rs` at 1.59.0.

### Changes in the last 12 months

Source: <https://raw.githubusercontent.com/casey/just/1.59.0/CHANGELOG.md>.

| Version | Date | Change |
|---|---|---|
| 1.59.0 | 2026-10-14 | Breaking: Unicode paths required. Added `[cache(environment)]`, `--usage` for a module, `--working-directory` without `--justfile`. |
| 1.58.0 | 2026-08-03 | Added `[timestamp]`. `--fmt` now writes `{{ x }}` with spaces, so earlier formatted files fail `--fmt --check`. |
| 1.57.0 | 2026-07-18 | Added `len()`; many strictness fixes; markdown justfiles found by name. |
| 1.56.0 | 2026-07-09 | Added `--jobs`, `num_jobs()`, `set indentation`, `[arg(min)]` and `[arg(max)]`, platform attributes on any item. |
| 1.55.0 | 2026-06-29 | Added module aliases, `set minimum-version`, `just_version()`, `[arg(multiple)]`. |
| 1.54.0 | 2026-06-22 | Added unstable `[cache]`, `[continue]`, `dotenv-command`. |
| 1.53.0 | 2026-06-16 | Added markdown justfiles, `recipe_name()`, unstable `set lists`. |
| 1.52.0 | 2026-06-08 | Added `default-script`, `[shell]`, `default-list`. |
| 1.51.0 | 2026-05-09 | Added `set no-cd`; `--justfile -` reads standard input. |
| 1.50.0 | 2026-04-19 | `--fmt` stabilized; added `[android]`, `module_path()`. |
| 1.49.0 | 2026-04-04 | Added user-defined functions (unstable), a skill for agents (`skills/just`), `--time`, a Nix flake. |
| 1.48.0 | 2026-03-23 | Lazy evaluation stabilized; dynamic completions; `--json`; Docker images. |
| 1.47.0 | 2026-03-14 | Added `lazy`, the `?` guard, `--group`, `[env(NAME, VALUE)]`. |
| 1.46.0 | 2026-01-01 | Recipes take `--long` options and flags; `--usage`. |
| 1.45.0 | 2025-12-10 | `[arg(pattern=...)]`. |
| 1.44.0 | 2025-12-06 | `[script]` stabilized; `f'{format}'` strings. |

## Ecosystem

| Area | Project | Facts | Source |
|---|---|---|---|
| Grammar | casey/tree-sitter-just | 197 stars, Apache-2.0, crate 0.2.0; moved from IndianBoy42 | <https://api.github.com/repos/casey/tree-sitter-just> |
| VS Code | nefrob/vscode-just | 280 stars, MIT, 0.10.2 (2026-06-10); the two older extensions are archived | <https://api.github.com/repos/nefrob/vscode-just> |
| Vim and Neovim | NoahTheDuke/vim-just | 121 stars; built-in highlighting in Vim 9.1.1042 and Neovim 0.11 | <https://api.github.com/repos/NoahTheDuke/vim-just> |
| Zed | jackTabsCode/zed-just | v0.2.0 (2026-01-09), MIT | <https://api.github.com/repos/jackTabsCode/zed-just> |
| Emacs | just-mode.el, justl.el | GPL-3.0; justl lists and runs recipes | <https://github.com/psibi/justl.el> |
| JetBrains | linux-china Just plugin | 0.6.58, 566,166 downloads | <https://plugins.jetbrains.com/plugin/18658-just> |
| Helix, Kakoune, Micro | built in | Helix since 23.05 (README) | <https://raw.githubusercontent.com/casey/just/1.59.0/README.md> |
| Language server | terror/just-lsp | 0.11.2 (2026-11-16), CC0-1.0, 359 stars; completions, hover, diagnostics, rename, formatting | <https://api.github.com/repos/terror/just-lsp> |
| MCP | PromptExecution/just-mcp | v0.1.5 (2025-11-15), 60 stars, self-described 67 percent complete; toolprint/just-mcp is smaller and older | <https://api.github.com/repos/PromptExecution/just-mcp> |
| Agent skill | `skills/just` in casey/just | since 1.49.0; `npx skills add casey/just --global` | <https://github.com/casey/just/tree/master/skills/just> |
| GitHub Actions | extractions/setup-just | v4 (2026-04-05) | <https://api.github.com/repos/extractions/setup-just> |
| GitHub Actions | taiki-e/install-action | `@just`, manifest at 1.59.0 | <https://api.github.com/repos/taiki-e/install-action> |
| Formatter | `just --fmt` | no separate formatter or linter found by search | dataset note |

Adoption: `gh api search/code?q=filename:justfile` returns 101,632 files (case-insensitive; counts files, includes vendored copies). Probing 100 candidate repositories found these roots with a justfile: zoxide (40,610 stars), casey/just, nix-community/home-manager (10,524), cloudflare/workerd (9,202), nextest (3,396), near/nearcore (2,622), Uniswap/v4-core (2,565), PaulRBerg/prb-math (1,008). That list is a floor, not a ranking.

### Alternatives

| Tool | Format and language | Latest | Stars | Note |
|---|---|---|---|---|
| make | Makefile | n/a | n/a | 17.6 million Makefile files; incremental by mtime; just avoids its tab and phony-target quirks |
| go-task/task | YAML, Go | v3.54.0 (2026-10-01) | 16,354 | checksum-based up-to-date checks |
| cargo-make | Makefile.toml, Rust | 0.37.24 (2025-01-18) | 2,956 | no release for 22 months |
| cargo xtask | Rust crate by convention | n/a | 1,360 | compiles to run a task |
| mise tasks | mise.toml, Rust | v2026.11.9 | 36,219 | tool manager plus tasks, with caching |
| moon | moon.yml, Rust | v2.6.5 | 4,191 | monorepo graph and remote cache |
| nx | nx.json, TypeScript | 24.0.6 | 29,475 | JavaScript monorepo build system |
| bun and npm scripts | package.json, `bun run`, `bun exec` | Bun 1.4.5-canary.1 locally | n/a | per package; no cross-language catalogue |

## Aphrody fit

Local `just` is installed: 1.59.0 (winget), equal to the latest release. A shell exists on Windows (`niu.exe`, winuxcmd `sh.exe`).

| Repository | Justfiles | Recipes | Result |
|---|---|---|---|
| `C:\src\ay` (legacy) | `justfile` plus 23 imported `.just` | 157 in 22 groups; 73 call `bun`, 65 reference `.ts` | `--list`, `--dump --dump-format json` OK, 0 warnings; `just doctor` runs with `SRC_ROOT` unset |
| `C:\src\ay\apps\nt` | 1 | 21 | `--list` OK |
| `C:\src\ay\apps\open` | 1 | 8 | `--list` OK |
| `C:\src\nie` | `justfile` plus 5 in `infra/just` | 41 | `--list`, dump OK |
| `C:\src\rosegriffon` | same layout as nie | 41 | `--list`, dump OK |
| `C:\src\shenron` | 1 (334 lines), shell `bun exec` | 62; 43 call `bun` | `--list`, dump, `--dry-run check` OK; `test-scripts` runs and 5 repo tests fail |
| `C:\src\aphrody` | vendored upstream only (cosmic, blitz, taffy, anyrender) | not run | n/a |
| `C:\src\bun`, `C:\src\ay-next`, `C:\src\browser` | none | n/a | n/a |

Defects found:

1. The user environment variable `SRC_ROOT` is `C:\aphrody`, which does not exist. The legacy justfile does `root := env("SRC_ROOT", justfile_directory())`, so `just agents status` fails with `Module not found`. Removing the variable fixes it. A justfile should use `justfile_directory()` without an environment override.
2. `just --fmt --check` exits 1 in ay, nie, rosegriffon and shenron. The diff is only the 1.58.0 change to `{{ x }}` spacing (95 lines in ay).
3. No justfile sets `minimum-version`, so an old `just` fails with a confusing parse error on newer syntax.

`aphrody-labs/just`, `aphrody-labs/tree-sitter-just` and `aphrody-dev/just` all return HTTP 404: there is no fork. The only `just` match in `aphrody-labs` is `just-bash`, a fork of `vercel-labs/just-bash`, which is unrelated.

### Recommendation

Adopt `just` as the single entry point of `ay-next`. `ay-next` currently has `tools/*.ts` with tests and two `package.json` scripts, and no justfile.

1. Recipes are one line: `bun tools/<name>.ts {{args}}` or `cargo ...`. Logic and tests stay in Bun typed scripts (`bun test ./tools/`).
2. The header sets `set minimum-version := "1.59.0"` and one shell per repo (`bun exec` as in shenron, or `sh -cu` with `windows-shell niu -cu` as in ay).
3. `check` includes `just --fmt --check`. Reformat once for the 1.58.0 spacing.
4. Use `[group]` and doc comments; switch to the ay pattern (root file holds settings and `import` lines) beyond about 20 recipes; use `mod` if names collide.
5. Install through winget, the Nix flake and `taiki-e/install-action@just`. Do not use the unstable `[cache]`, `lists` or user-defined functions.
6. For agents, prefer `just --json` and the in-repo skill over `just-mcp`.

Reasons tied to the data: five Aphrody repos already converged on this layout and it works with 1.59.0; one flat catalogue covers Bun, cargo, Nix and depot_tools, which `package.json` scripts cannot; it is a single static binary with 20 releases in 12 months and a CC0 licence; the alternatives either add a runtime or model (`mise`, `nx`, `moon`, `go-task`), have gone quiet (`cargo-make`), or compile code to run a task (`cargo xtask`). Keep `mise` in mind only if tool-version pinning outside Nix is wanted later.

Gaps: the VS Code Marketplace install count was not retrievable; Helix and Zed behaviour beyond the README is unverified. The index in `README.md` of this folder was not updated.
