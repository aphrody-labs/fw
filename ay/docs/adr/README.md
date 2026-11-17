# Architecture decision records

Index updated 2026-11-17. A record is never rewritten; when a decision changes, the new record says what it supersedes and
this table carries the current state. ADR-0009 (monorepo tree and build orchestration) is the newest; ADR-0008 (SDK languages and toolchain) governs the languages.

| ADR | Decision | Status | Current state (2026-11-17) |
|---|---|---|---|
| [0001](0001-tree.md) | Product-first tree on upstream conventions | accepted 2026-11-16 | Partly superseded by 0008; the tree is in `docs/architecture.md` |
| [0002](0002-build.md) | Nix for toolchains, native workspaces, one LLVM | accepted 2026-11-16 | Partly superseded by 0008: the LLVM rule stands; the Cargo workspace and Rust pins belong to `aphrody`; `flake.nix` is not in this tree |
| [0003](0003-browser.md) | Browser on CEF with cef-rs, native UI from `aphrody` | proposed 2026-11-16 | Product owned by `aphrody-labs/browser`; the record stays until that repository carries its own copy |
| [0004](0004-os.md) | OS on an LFS bootstrap, Nix packages, kernel.org + CachyOS | accepted 2026-11-16 | Product owned by `aphrody` (Rust OS), `anu` (Unix userland) and `aydroid` |
| [0005](0005-search.md) | Search from upstream Rust components | proposed 2026-11-16 | Product owned by `aphrody`; accepted only after the index benchmark |
| [0006](0006-cloud.md) | Cloud on a Bun edge runtime | proposed 2026-11-16 | Product owned by `aphrody` on the `bun` fork; the Cloudflare policy is in the organization standard |
| [0007](0007-browser-automation.md) | One browser-automation ecosystem, `ay browser` | proposed 2026-11-16 | Evidence in `docs/research/browser-automation.*`; implementation moves with the browser to `aphrody-labs/browser` |
| [0008](0008-sdk-languages-toolchain.md) | SDK languages and toolchain | accepted 2026-11-17 | In force |
| [0009](0009-monorepo-tree.md) | Monorepo tree and native build orchestration, template for any Aphrody repository | accepted 2026-11-17 | In force; supersedes the tree of 0001 and the native-workspaces part of 0002 |
