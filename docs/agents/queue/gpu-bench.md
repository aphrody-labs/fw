# Queue: gpu-bench (Dawn vs wgpu, three.js vs Bevy)

## Goal
Complete benchmark in `aphrody-labs/gpu-bench` (PC: `C:\src\gpu-bench`): Suite A Dawn vs wgpu (Bun, Deno, native C++/Rust, Chrome, Firefox; D3D12 and Vulkan), Suite B three.js WebGPURenderer vs Bevy (native + wasm WebGPU; N = 1k/10k/100k cubes + DamagedHelmet). Results as JSON + generated `RESULTS.md`, README, docs, skill `gpu-bench`, fw source entry.

## Done (all verified on origin)
- gpu-bench repo public, `main`, submodules `vendor/{dawn,wgpu,three.js,bevy}` shallow, relative URLs. Pins: dawn `b18147774e331c5ec1b34fb3d7b87c95371e377f` (release v20261119.211954), wgpu `2b38d994b38dd001157d31fd3321111103a8dd80` (wgpu-v30.0.2), bevy `a0577b5c6a7b208719dd761c5c6a7a539e472a97` (v0.20.1), three.js `0245b6dad700ecc32f145dfa495c55e41fc75db7` (fork main).
- gpu-bench HEAD = origin/main = `02f8b4bdd9ea6d87d514c4a48b8cf5f0f16dbc89`. Key commits: submodules `931b2f3`, WGSL+JS core+wgpu bench `99f0221`, Dawn C++ bench `80aef00`, Bevy/three scenes `56776d1`/`eae0834`, harness `cda63df`, workflow `6c5e3bc`, Cargo.lock pins `cc2c8f4`, docs+skill `fdc347d`, warmup/quantization fix `2d1da64`, DLL staging `b4822db`.
- fw: `plugins/aphrody/sources.toml` entry `gpu-bench` pushed as `e180235bc5809b81af11072bdc4af679a70ed7c8` on origin/main (only that file). A duplicate local-only commit `d34ab024` exists in the fw working repo; a `git pull --rebase` drops it (same patch).
- Windows binaries built on GitHub Actions (fr1 was at load 100+): wgpu bench run 44862554784, Dawn bench run 44862549117; both smoke-tested on the RTX 4070: native wgpu D3D12 and Vulkan run, checks pass; Dawn native Vulkan runs once `vulkan-1.dll` sits next to the exe.
- Bun (Dawn via `webgpu@0.6.2`) and Deno (wgpu) hosts run end to end. bun:wgpu does not exist in aphrody-labs/bun, so wgpu is measured through Deno.
- Firefox 160.0.1 portable extracted to `.tools/firefox` (WebGPU not yet tried).

## Findings to keep (each reproduced)
- Dawn quantizes timestamp queries (steps of 0.0655 ms); disabled by `disable-dawn-features=timestamp_quantization` (Node), `timestamp_quantization` toggle (C++), `--enable-webgpu-developer-features` (Chrome, unverified).
- Dawn native loads DLLs only from the exe dir (else error 87): stage `vulkan-1.dll`, `dxcompiler.dll`, `dxil.dll`, `d3dcompiler_47.dll` into `.bin/` (`bun scripts/run.ts setup` does it). Native Dawn D3D12 failed without dxcompiler.dll (Dawn defaults to DXC).
- `webgpu` npm (Dawn-node) cannot load Vulkan on this PC (error 87, also under Node): Dawn-node is D3D12 only here.
- Deno ignores WGPU_BACKEND; default adapter is Vulkan; D3D12 forced by invalid `VK_ICD_FILENAMES` (runner does it).
- wgpu defaults to FXC, Dawn to DXC: add `WGPU_DX12_COMPILER=dxc` variant (needs dxcompiler.dll+dxil.dll on PATH or in cwd) and a Dawn `use_dxc` disabled variant for matched compilers.
- Machine: i7-13700F, RTX 4070 driver 621.14, display 2560x1440@144 at 125% (DPR 1.25), Ultimate Performance plan. Run-to-run GPU clock ramp caused cv ~20% in a first short-warmup pass (since fixed: 1000 ms warmup).

## Remaining
1. Rebuild Bevy: last Actions attempt failed (`Scene`, `SceneRoot` missing; fixed in `02f8b4b` by feature `scene`). Compile errors may remain (Bevy 0.20 API: `RenderAdapterInfo` deref, `MessageWriter<AppExit>`, `Exposure`, `DiagnosticsStore` paths, `AssetMetaCheck`). Iterate with `gh workflow run build.yml -f target=bevy-native` / `bevy-web`, read `gh run view <id> --log-failed`.
2. Download artifacts: `bun scripts/run.ts fetch <wgpu|dawn|bevy-native|bevy-web> <run-id>` (currently in `.bin`: wgpu, dawn exes; dawn/wgpu must be rebuilt only if bench sources change; current ones include the warmup/toggle fixes).
3. Add DXC/FXC variants to `A_CONFIGS` in `scripts/run.ts` (see findings), run `bun scripts/run.ts setup` (stages DLLs), then `bun scripts/run.ts bench a` (10 runs each) with the PC GPU otherwise idle; then `bench b`. Test Firefox page `/browser?browser=firefox&engine=wgpu`; Chrome kiosk run; check `crossOriginIsolated` and screenshot-inspect the three/Bevy scenes (camera framing, helmet visible) before trusting suite B.
4. `bun scripts/run.ts sizes && bun scripts/run.ts report`, commit `results/**` and `RESULTS.md` by path, record rustc (`rustc 1.100.0 (0ef1bc386 2026-11-09)` on Actions windows-latest), wasm-opt version, Dawn release tag.
5. Optionally run `claude plugin update aphrody@fw` after fw's unify cron absorbs the skill.

## Resume commands (PC, bash)
```
cd /c/src/gpu-bench && git pull
bun scripts/run.ts setup
gh workflow run build.yml --repo aphrody-labs/gpu-bench --ref main -f target=bevy-native
bun scripts/run.ts fetch bevy-native <run-id>
bun scripts/run.ts bench a && bun scripts/run.ts bench b && bun scripts/run.ts sizes && bun scripts/run.ts report
```
Note: another session also commits to this repo (`raster/`, `vendor/skia`, `vendor/obscura`); commit explicit paths only.

## Directories created
| Where | Path | Size | Safe to delete |
| --- | --- | --- | --- |
| PC | `C:\src\gpu-bench` (repo + submodules `vendor/`) | vendor 1.4 GB | keep until finished |
| PC | `C:\src\gpu-bench\.tools\firefox` (+ `firefox-dl` installer) | 454 MB | yes (`firefox-dl` any time; `firefox` only if Firefox runs are dropped) |
| PC | `C:\src\gpu-bench\.bin` (exes, staged DLLs) | 17 MB | yes, regenerated by fetch/setup |
| PC | `C:\src\gpu-bench\js\node_modules` | 174 MB | yes (`bun install`) |
| PC | `C:\src\gpu-bench\.tmp` (scratch, stale pre-warmup JSON in `.tmp/stale`) | 0.5 MB | yes |
| fr1 | `/srv/xbuild/gpu-bench/repo` (clone, wgpu submodule only) | 731 MB | yes |
| fr1 | `/srv/xbuild/gpu-bench/tools/dawn-rel` (Dawn Windows release extract, ~895 MB) | 895 MB | yes (Actions builds Dawn) |
| fr1 | `/srv/xbuild/gpu-bench/xwin-cache` (MS CRT/SDK splat for cargo-xwin) | 1.1 GB | yes if fr1 builds are not resumed |
| fr1 | `~/gpu-bench` | moved to /srv/xbuild | already removed |
| fr1 | `~/.cargo/bin/cargo-xwin` (installed) | small | keep or `cargo uninstall cargo-xwin` |
| GitHub | Actions caches/artifacts of aphrody-labs/gpu-bench | n/a | expire on their own |

## Long jobs
None running from this task: Actions run 44861703453 (stale Bevy native) cancelled; no local benchmark process is running; fr1 cargo build killed earlier.
