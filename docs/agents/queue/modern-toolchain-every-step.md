# Lot: modern tools and libraries at every step, from SSH to the browser

Order of the owner, 2026-11-21: optimize the ignore files, compression and archive libraries, parsing, tooling, all VCS and CI/CD. Replace every legacy tool or library with its modern late-November-2026 equivalent, at every step:

- SSH and git;
- builds and containers;
- the final in-browser product for the end user;
- the CLIs the agents use during development.

Automate, modernize and optimize every step and detail.

This lot works inside the fw monorepo (`fw-is-the-monorepo`) and after the crate-layout lot's moves. It follows the derive-from-code rule and the "prefer the best upstream crate or package over in-house code" rule.

## Baseline read 2026-11-21 on the PC

- **Tools:** git 2.56.0.windows.4 (niugit build); OpenSSH_for_Windows 10.0p2 with LibreSSL 4.2.0.
- **Git status:** `git status` takes 0.11 s on fw (4,840 files), 0.18 s on aphrody (9,748) and 0.11 s on ay (1,891).
- **Git config:** `feature.manyFiles` and the untracked cache are on everywhere. fsmonitor is on in aphrody and ay, off in fw.

## Steps and what to decide by measurement

For each step, list the tool or library in use (from the code, manifests and workflows), the modern candidates read live, a measured comparison, and the switch.

1. **SSH:**
   - Connection reuse: Windows OpenSSH has no ControlMaster, so measure the alternatives, for example a persistent multiplexed tunnel or a Rust SSH client.
   - Key types and agent forwarding hygiene.
   - The fleet SSH config, generated from `cluster/build.toml` and renamed by the unified-cluster lot.
2. **Git and VCS:**
   - Partial and blobless clones in CI and on the cluster.
   - Commit-graph and `git maintenance` on every checkout.
   - Sparse checkout for the kernel and the Zed trees.
   - Scalar where it helps.
   - jj (Jujutsu) or other modern VCS front ends, measured against plain git for the agents' commit-by-path flow.
   - The git LFS or Xet alternative for large assets.
3. **Ignore files:**
   - One generated set, `.gitignore`, `.dockerignore`, `.npmignore` and the cargo `exclude`/`include` lists, derived from the layout map.
   - A check that no tracked path is ignored and no build output is tracked.
4. **Compression and archives:**
   - Releases: zstd and brotli, with tar plus zstd for Linux archives.
   - Crates: the `zip`, `tar`, `flate2` and `zstd` crates against the newest (for example `zip` 2.x/3.x, `async-compression`, `lzma-rs` or `xz2`); measure ratio and speed on our real artifacts.
   - The R2 and CDN encodings.
5. **Parsing:** JSON, TOML, YAML, Markdown, source code, CSV.
   - Use the fastest maintained crate or Bun built-in each time: simd-json or sonic-rs against serde_json, `toml_edit`, Bun.TOML and Bun.YAML, tree-sitter, `pulldown-cmark` or `comrak`.
   - Remove duplicate parsers.
6. **Build tooling:**
   - Linkers: wild or mold, already measured by the crate lot.
   - sccache-dist on every node, nextest, `cargo-hakari`-style feature unification decided by measurement.
   - Bun build and bundler, the uv workspace, Gradle configuration cache, CMake presets with Ninja.
7. **CI/CD:**
   - Fewer workflows, each job measured.
   - Runners: the cluster pool, with GitHub-hosted runners only for pull requests and Windows.
   - Caches on the nodes.
   - Path filters so a change runs only the gates it touches.
   - One release workflow on the release train.
   - Deploys through the estate file of the edge-estate lot.
8. **Containers:**
   - Distroless or Alpine/Wolfi images, built multi-stage on the cluster with podman or buildah.
   - SBOM and provenance.
   - The Linux musl binaries as the image payload.
9. **Final product in the browser:**
   - WebGPU plus Wasm builds of the UI (GPUI web, NSH web).
   - Brotli and zstd on the CDN, modern image formats (AVIF), font subsetting (the material CDN), service worker caching.
   - Measured with Lighthouse and the web-quality skill.
10. **CLIs the agents use:**
    - Replace slow or legacy userland tools with the fast Rust and Go ones, following the fast-userland memory: rg, fd, jaq, sd, xh, uutils and others.
    - Generate their install from one manifest on the PC and on every node.
    - Measure the common agent loops: git commit by path, gh API calls, SSH round trips.

## Output

`docs/research/modern-toolchain-2026-11.md` holds one row per step: current, candidates, numbers, decision, commit. A check prevents a removed legacy tool from coming back.
