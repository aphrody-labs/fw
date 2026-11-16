# Architecture

The tree follows the conventions of the upstreams this project builds on, not a layer model copied from another OS:
Zed's flat `crates/`, Bun's `packages/*` workspaces, Nix for reproducible toolchains and packages, and product
folders the way SerenityOS and Ladybird keep an OS and a browser in one repository. Decisions are in `docs/adr/`.

```
ay.toml          the only identity and configuration source
flake.nix        pinned toolchains, dev shells, CI checks and OS package builds (ADR-2)
Cargo.toml       Rust workspace: members = ["crates/*"], shared [workspace.dependencies]
package.json     Bun workspace: private, workspaces = ["packages/*", "apps/*"], no root dependencies
apps/            products a person runs, one folder each, any language: browser, search, cloud
crates/          every Rust crate, flat, named ay_<area>_<thing>
corelib/         the SDK corelib: freestanding C23 + assembly + WAT, the C ABI header, conformance hosts and proofs
packages/        every Bun/TypeScript package, flat, named @aphrody/<name>
os/              the distribution as data: bootstrap (LFS stage 0), pkgs (Nix), kernel, installer, image
brand/           identity dataset and asset pipeline
tools/           the ay CLI glue and typed scripts for every repeatable flow
docs/            spec, architecture, ADRs, feature matrix
third_party/     upstreams as git submodules with relative URLs, one checkout each
```

Rules:
- A folder exists only with a working, tested unit; manifests list only members that build.
- Code shared by two products lives in `crates/` or `packages/`; a product folder holds only that product.
- Upstreams are submodules, never copies. Chromium is fetched with depot_tools `gclient`.
- Nothing generated is committed (logs, reports, dumps); generators are committed instead.
- Hosts are named by role in `ay.toml`: builds on `build`, tests on `test`, production on `prod`.

## Toolchain
One LLVM: clang and lld with ThinLTO for C and C++, Rust built against the same LLVM with cross-language LTO,
`bun compile` (AOT) for typed Bun binaries, depot_tools `autoninja` for GN/ninja upstreams, tree-sitter as the one
parsing layer for code intelligence. See ADR-2.

## Products and their stacks
| Product | Stack | ADR |
|---|---|---|
| os | LFS stage-0 bootstrap, then Nix packages; kernel.org + CachyOS patches; BLFS Wayland desktop; the native desktop from aphrody-dev/aphrody; Waydroid | ADR-4 |
| browser | Chromium with our UI, agent panel and WebMCP | ADR-3 |
| search | Rust crawler, index and ranker; Bun front end; AI mode over the OpenAI-compatible model API | ADR-5 |
| cloud | Bun edge runtime with a Workers-compatible API; deploys, domains, TLS, storage, logs | ADR-6 |
| craft | engine: aphrody-labs/craft-anything (Rust, consumed by aphrody-dev/aphrody as `vendor/craft-anything`), its CLI and MCP server; native GUI: aphrody-dev/aphrody | ADR-1 |
| brand | character dataset, logo and token pipeline, Google Sans Flex, Material 3 | ADR-1 |

## Native UI lives in aphrody-dev/aphrody
The native stack is owned by its own repository, `aphrody-dev/aphrody` (a Zed fork). That covers GPUI and the
gpui-kit components, COSMIC and iced, the desktop shell (`gpui_app_shell`) and the Craft native UI (`os_craft`).
Since 2026-11-17 aphrody is the full-Rust OS and source of truth and this repository is its C / assembly and Bun
SDK; the dependency points from aphrody to ay, so ay embeds no aphrody checkout (one local checkout per repo). The OS image packages its shell; the browser
and the web products don't depend on it. No GPUI or COSMIC code is written here.
