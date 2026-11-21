# Lot: import the DioxusLabs material and study Dioxus against GPUI

Order of the owner, 2026-11-21: "import this new content into your pipeline and global workflow, and study Dioxus and GPUI".

The owner's material was a Python script. It wrote two files:

- `dioxuslabs-ecosystem-2026.json`:
  - the org metadata and 78 repositories;
  - the dioxus architecture notes 01-CORE to 10-WASM-SPLIT;
  - 2026 commits, PRs, issues and discussions;
  - the AGENTS.md rules of dioxus, blitz and accessibility-cli;
  - the six banned Dioxus anti-patterns AP-01 to AP-06;
  - the concurrency rules;
  - a WASM size profile.
- `dioxuslabs-reference-2026.rs`: the reference app, kept verbatim in `inputs/`.

## Spot-checked live on 2026-11-21

- **DioxusLabs:** 78 public repositories.
- **dioxus:** 39,866 stars. Releases v0.8.0-alpha.3 (2026-10-16) and v0.7.10 stable (2026-07-30). The `dioxus` crate is at max 0.8.0-alpha.3, stable 0.7.10.
- **Other repos:** blitz 4,497 stars, taffy 3,802, rustd 249.
- **GPUI:** the `gpui` crate is at 0.2.2.

Every other figure in the paste stays unverified until it is read live.

## Work

1. **Snapshot generator, not a copy.** A generator (Rust, following the pure-Rust order) rebuilds the snapshot from live sources: `gh api` for the org, repos, releases, PRs, issues, discussions and AGENTS.md files, plus crates.io. It writes JSON and a summary to `docs/research/`, refreshes on the upstream research schedule, and drops any field it cannot source.
2. **Upstream tracking.** Track the DioxusLabs upstreams we use or should use in the fw upstream catalog: dioxus, blitz, taffy, anyrender, the stylo/parley/vello forks, rustd, accessibility-cli, dioxus-components, sdk. Fork into aphrody-labs only what our code depends on.
3. **A `dioxus` skill.** It covers the anti-patterns, the concurrency rules (no Signal guard across `.await`, #6058), the blitz AGENTS.md rules, the architecture-notes map and the release facts. The WASM size profile is verified on our own build, with measured sizes.
4. **Real build of the reference app.** Compile `inputs/dioxuslabs-reference-2026.rs` on the cluster against dioxus 0.7.10 and 0.8.0-alpha.3, then run its tests and `dx` web build and measure the wasm size. Fix what does not compile; keep it as an example only if it builds.
5. **Study Dioxus against GPUI** for every product surface:
   - the aphrody desktop/OS shell (GPUI), NSH, the web/WASM surfaces, and Blitz in fw-ui-blitz;
   - compare rendering, layout (both on Taffy), reactivity, fullstack, hot reload, mobile, accessibility, WASM size and startup, and ecosystem velocity;
   - measure the same small app in both on Windows 11, Linux and wasm32 (macOS is out of scope);
   - output `docs/research/dioxus-vs-gpui-2026-11.md` with a decision per surface, and follow-up lots.
