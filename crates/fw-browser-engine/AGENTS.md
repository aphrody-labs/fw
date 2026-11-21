# fw-browser-*: agent contract

The browser crates of fw: `crates/fw-browser-{engine,headless,browse,detect,index,views,vpn,chromium}`. History before the merge is kept (git-filter-repo path rename of `aphrody-labs/browser`, which now only points here). Decision record: [ARCHITECTURE.md](ARCHITECTURE.md). Usage: `fw browser --help`.

## Rules

- Engines are pins, never copies: Blink comes from a Chromium checkout (`DEPS`), the lite engine from the `aphrody-labs/obscura` fork (commit in `tools/layout.toml` [forks]), everything else in `tools/upstreams.toml` and `tools/upstreams.lock.json`. No vendored Chromium, V8, Skia or Servo sources, no submodules.
- The lite engine needs `panic = "unwind"` and links V8, so it is built from its own checkout (`build/lite.sh`), not as a member of this workspace (fw releases use `panic = "abort"`). `fw browser` drives it as a sibling binary (`fw-browser-lite`, or `obscura`), the way `fw cluster k8s` runs `fw-k8s`.
- `fw-browser-chromium` downloads CEF in `build.rs`: it is `exclude`d from the workspace and builds from its own directory.
- WebGPU goes through wgpu (`deno_webgpu` on `wgpu-core`), never Dawn, in the lite engine. Blink keeps its own Dawn.
- New code goes to the crate that owns the function; a second copy of a crate is deleted in the same commit. New crates are `fw-browser-<unit>`.
- Dependency direction: fw-browser-* depend on nothing above fw. `aphrody` and `ay` consume them by path (`../fw/crates/fw-browser-*`).
- Never build engines on a workstation: `ssh fr1` (outputs under `/srv/xbuild/<task>/`) or Cloud Build.
- Measurements live in `testing/compare.ts` output, not in prose; a number in a doc names the command that produced it.

## Test and measure

```sh
cargo test -p fw-browser-headless -p fw-browser-engine -p fw-browser-detect -p fw-browser-browse -p fw-browser-vpn -p fw-browser-views -p fw-browser-index
bun crates/fw-browser-engine/testing/compare.ts --chrome <chrome> --lite <fw-browser-lite> --pages crates/fw-browser-engine/testing/pages   # lite engine against Chrome headless
bun crates/fw-browser-engine/testing/acceptance.ts run                                                                                       # every engine x the live corpus
```

## Layout

| Path | Content |
|---|---|
| `crates/fw-browser-engine` | engine catalog (`pick`, `escalate`), pins (`DEPS`, `tools/`), measurement (`testing/`), `build/` |
| `crates/fw-browser-headless` | `fw-browser` binary (`fw browser`): open, screenshot, eval, pdf, CDP server, MCP; SDK over the sibling crates |
| `crates/fw-browser-browse` | CDP session pool, ARIA `@e1` snapshots, WebMCP, human handoff |
| `crates/fw-browser-detect` | technology and challenge detection, shared SSRF guard |
| `crates/fw-browser-vpn` | per-context exit routing, DNS shield |
| `crates/fw-browser-views` | Chromium views model for GPUI and headless hosts |
| `crates/fw-browser-index` | HNSW index with int8 quantisation |
| `crates/fw-browser-chromium` | CEF from Rust and Bun, Chromium release datasets (`apps/`, `packages/`) |
