---
name: dioxus
description: "Dioxus 0.7 and 0.8: rsx, signals, router, #[server], Manganis, Subsecond, dx CLI, Blitz. Use before writing or reviewing Dioxus code, choosing Dioxus or GPUI, or sizing a Dioxus wasm build."
---

# Dioxus

Live facts (stars, releases, crates.io versions, AGENTS.md text, issue states) are not written here: read `docs/research/dioxuslabs-ecosystem.json` (summary `docs/research/dioxuslabs-ecosystem.md`), rebuilt by `tools/dioxus-snapshot` every 6 hours by the sync-upstreams workflow. The decision per surface against GPUI is `docs/research/dioxus-vs-gpui-2026-11.md`. A buildable example that follows every rule below is `tools/dioxus-reference` (the owner's reference app; `check.sh` runs clippy, tests and the dx web build on 0.7.10 and 0.8.0-alpha.3).

## Pick the stack

| Need | Use |
| --- | --- |
| Web SPA or SSR with server functions | `dioxus` with `web` and `fullstack` features, `dx build --release --web --fullstack true`; server side is Axum through `server` |
| Desktop with a webview | `desktop` feature (Wry and Tao) |
| Native GPU or CPU rendering without a webview | `native` feature, renderer Blitz (Stylo, Taffy, Parley, anyrender) |
| Hot reload | `dx serve --hotpatch` (Subsecond, jump-table patching of Rust code) and RSX template diffing |

## Anti-patterns as the reference app encodes them

Numbered as in the reference app (`AP-01` to `AP-06`); the owner's original table is not in this repository, only these usages are, so each rule is stated by what the reference does.

| Id | Rule | Where it shows |
| --- | --- | --- |
| AP-01 | List items carry a stable business key, never the loop index | `RepoCard { key: "{repo.slug}" }` |
| AP-02, AP-06 | Child props are plain data: `Clone + PartialEq`, no `Mutex` or `RefCell`; the child reports through `EventHandler` | `RepoCard` |
| AP-03 | Derived state is a `use_memo`, never `.set()` in the render body | `filtered_metrics` |
| AP-04 | One signal per independent fact, not one signal holding a struct | `search_query`, `only_agents_md`, `selected_category` |
| AP-05 | Identifiers and one-time values come from `use_hook`, never recomputed on re-render | `session_badge_id` in `AppShell` |

## Rust 1.100.0

`#[derive(Routable)]` warns "unreachable call" on a `String` dynamic segment from Rust 1.100.0 (DioxusLabs/dioxus#6095, open); an item-level `#[allow]` does not reach the derive output, the crate-level `#![allow(unreachable_code)]` does. fw is on 1.100.0.

## Concurrency rules

1. Copy a signal value out before an `.await`: `let query = search_query();` then `async move { ... query ... }`. A `read()` or `peek()` guard (`GenerationalRef`) must not live across an await; the generic case is DioxusLabs/dioxus#4124 ("Already Borrowed").
2. A guard that is a temporary in a call's arguments lives to the end of the statement, so a callee that writes the same signal through its own `Copy` handle panics with `AlreadyBorrowed`. This is DioxusLabs/dioxus#6058. It compiles, passes clippy and panics at runtime. The two safe forms are `let next = *count.peek() + 1; store(count, next);` and `store(count, count() + 1);`. The test `signal_guard_in_call_arguments_panics_and_copying_first_is_safe` in the reference reproduces both on the pinned dioxus versions.
3. Cancel safety: work that can be dropped at an `.await` (component unmount) releases resources in `Drop`, as `InFlightGuard` does.

## Security

Static and cache path segments are validated before use (`validate_safe_slug`): the ISR file cache path traversal is DioxusLabs/dioxus#6046 with the fix proposed in dioxus#6047. State and titles of both are in the snapshot.

## Blitz (Dioxus Native) rules from its AGENTS.md

Read `tracked.blitz.agents_md.text` in the snapshot before touching Blitz. Rules that bite: no whole-tree passes on hot paths (`resolve()`); an attribute that layout reads directly must be added to `read_by_layout` in `restyle_for_attribute_change`; WPT results are the correctness signal; headless machines have no GPU adapter (render to an image or use the softbuffer CPU renderer); fixes for Taffy, Parley, anyrender and Stylo go upstream, not into Blitz; no AI attribution trailers.

## Architecture notes map

`notes/architecture/` of DioxusLabs/dioxus (the list with byte sizes is in the snapshot, `architecture_notes`). Read the note that matches the work:

| Work | Note |
| --- | --- |
| Overview | `00-OVERVIEW.md` |
| VirtualDOM, components, diffing, events | `01-CORE.md` |
| CLI, build, bundling, dev server | `02-CLI.md` |
| RSX parsing and formatting | `03-RSX.md` |
| Signals and state | `04-SIGNALS.md` |
| Server functions, SSR, hydration | `05-FULLSTACK.md` |
| Web, desktop, native, liveview renderers | `06-RENDERERS.md` |
| Hot-reload and hot-patching | `07-HOTRELOAD.md` |
| Assets and Manganis | `08-ASSETS.md` |
| Router | `09-ROUTER.md` |
| WASM code splitting | `10-WASM-SPLIT.md` |
| Native plugin FFI | `11-NATIVE-PLUGIN-FFI.md` |
| Manifest system | `12-MANIFEST-SYSTEM.md` |

The owner's paste listed notes 01 to 10; the live tree has 00 to 12.

## Build and size

Release profile of the reference: `opt-level = "z"`, `lto = true`, `codegen-units = 1`, `panic = "abort"`, `incremental = false`, and `strip = "debuginfo"`. The owner's profile said `strip = true`; with it `dx build --web` fails on dx 0.7.10 with wasm-bindgen 0.2.130 ("failed to read wasm-bindgen data": `strip = true` removes the wasm-bindgen custom sections). Let `wasm-opt` strip, not cargo.

Measured on prd (Arch, rustc 1.100.0, `dx build --release --web --fullstack true`, wasm-bindgen 0.2.130, wasm-opt run by dx), reference app, bytes:

| dioxus | wasm | gzip -9 | brotli -q 11 | js glue | server binary |
| --- | ---: | ---: | ---: | ---: | ---: |
| 0.7.10 | 966,921 | 384,497 | 301,057 | 58,621 | 3,864,408 |
| 0.8.0-alpha.3 | 1,015,381 | 407,045 | 319,017 | 56,420 | 4,496,568 |

Both servers answered the SSR page (`Étoiles cumulées (sélection) : 49452 | PRs ouvertes : 755`) and the `#[server]` endpoint (slug `../x` refused with the CWE-22 message, HTTP 500; slug `blitz` returned the record, HTTP 200). Sizes of the same small app in GPUI and Dioxus on three targets are in `docs/research/dioxus-vs-gpui-2026-11.md`; do not quote sizes from memory.

Builds run on the cluster nodes, never on the PC: fr1 `/srv/xbuild`, prd `/srv/build`. `tools/dioxus-reference/check.sh` runs the whole check per version; `dx` comes from the GitHub release (`dx-x86_64-unknown-linux-gnu.tar.gz` of the tag) or `cargo binstall --root <dir> dioxus-cli@<version>`, so 0.7.10 and 0.8.0-alpha.3 coexist. A busy node makes a cold build take tens of minutes (the clippy pass of the reference took 10 min 49 s at load 40 to 50 on 8 cores), so run it in the background.
