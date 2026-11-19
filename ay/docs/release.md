# Release and download pipeline

One reusable workflow, `.github/workflows/release.yml`, publishes the artifacts of every product of the ecosystem. Each repository keeps
building its own artifacts in its own release workflow (it knows its targets); the publish job is shared. `ay` never checks out,
triggers or waits for another repository (docs/architecture.md, Independence): the dependency goes from the caller to this workflow.

```text
caller build jobs -> upload-artifact (one file per asset) -> job calling release.yml
   release.yml publish: classify by release.toml -> sha256 + size -> release.json, SHA256SUMS
                        -> provenance attestation -> R2 (immutable objects, channel pointer, manifest.json) -> GitHub Release
edge Worker (apps/edge): serves latest and versioned downloads per os and arch, and /manifest.json, from the R2 layout below
```

## Contract

- `release.toml` is the single matrix: per product (keys of ay.toml `[ecosystem].stack`) the artifact name pattern, `os`, `arch`, `abi`,
  `kind` (vocabulary of ay.toml `[sdk]` and `apps/edge/src/manifest.ts`) and the runner. A file in the artifact directory that no row
  names fails the publish; checksum files and subdirectories (the `manifests/` of aphrody) are ignored. A row with `ship = false` is checked by CI only.
- The R2 layout (bucket `[buckets].RELEASES` of `apps/edge/edge.toml`):

  | Key | Content | Mutable |
  |---|---|---|
  | `<product>/<version>/<asset>` | the artifact | no |
  | `<product>/<version>/SHA256SUMS` | `<sha256>  <asset>` lines | no |
  | `<product>/<version>/release.json` | `ReleaseManifest` (`ay.release.v1`) | no |
  | `<product>/channels/<channel>.json` | `ChannelPointer` to the latest version of the channel | yes |
  | `manifest.json` | `IndexManifest` (`ay.releases.v1`): every product and channel | yes |

  `tools/release-publish.ts` refuses to overwrite an immutable key with other content (`--force` overrides, an identical re-run is a no-op).
- Channels: `lts`, `stable`, `preview`, `canary`, `nightly`, `dev`. Only `lts` and `stable` produce a non-prerelease GitHub Release.
- Local dry run, no network and no secret: `bun tools/release-publish.ts publish --product <key> --version <v> --channel <c> --tag <t> --commit <sha> --dir <artifacts> --out <dir>`.
  `bun tools/release-publish.ts matrix --product <key>` prints the build matrix of a product as a GitHub Actions `include` list.

## Calling the workflow

Inputs: `product`, `version` (no leading `v`), `channel` (default `stable`), `tag`, `commit`, `artifacts-pattern` (names of the uploaded
artifacts, default `*`), `tools-repository` (the slug of the `ay` repository, `[repos].ay.slug` of ay.toml: the publisher and `release.toml` are
fetched from it with a sparse checkout, because a called workflow runs in the caller's repository), `tools-ref` (default `main`).
Secrets: `CLOUDFLARE_ACCOUNT_ID`, `R2_ACCESS_KEY_ID`, `R2_SECRET_ACCESS_KEY` (optional: without them the release goes to GitHub only and the
job warns). The calling job grants `contents: write`, `id-token: write`, `attestations: write`. The R2 credentials never appear in logs.

### ay (this repository)

Nothing to add: pushing a tag `v*`, or running the workflow by hand, builds the `ay` rows of `release.toml` (Bun-compiled CLI for linux
x86_64 and aarch64, windows x86_64, macos aarch64; corelib wasm32 `base`, `simd`, `atomics`, `component` with the LLVM pinned in
`docs/guides/assembly-and-wasm/listings.toml`) and publishes them.

### fw

`.github/workflows/release.yml` builds `fw-x86_64-linux-musl`, `fw-x86_64-windows.exe` and `fw-aarch64-macos`; the wasm module of the
`fw-wasm` crate (`cargo build --release --target wasm32-unknown-unknown -p fw-wasm`, already built by `ci.yml`) is the fourth row. The artifact must be a
file named like the asset, so the upload step stages it:

```yaml
      - name: Stage the asset under its release name
        shell: bash
        run: |
          mkdir -p dist
          cp "target/${{ matrix.target }}/release/${{ matrix.bin }}" "dist/${{ matrix.asset }}"
      - uses: actions/upload-artifact@<pin>
        with:
          name: release-${{ matrix.asset }}
          path: dist/${{ matrix.asset }}
```

```yaml
  publish:
    needs: build-artifacts
    permissions: { contents: write, id-token: write, attestations: write }
    uses: aphrody-dev/ay/.github/workflows/release.yml@main
    with:
      product: fw
      version: ${{ github.ref_name }}      # strip the v in a previous step when the tag is vX.Y.Z: ${GITHUB_REF_NAME#v}
      channel: stable
      tag: ${{ github.ref_name }}
      commit: ${{ github.sha }}
      artifacts-pattern: release-*
      tools-repository: aphrody-dev/ay
    secrets:
      CLOUDFLARE_ACCOUNT_ID: ${{ secrets.CLOUDFLARE_ACCOUNT_ID }}
      R2_ACCESS_KEY_ID: ${{ secrets.R2_ACCESS_KEY_ID }}
      R2_SECRET_ACCESS_KEY: ${{ secrets.R2_SECRET_ACCESS_KEY }}
```

The `fw` rows of `release.toml` already name `fw_wasm.wasm`; add a matrix entry building `fw-wasm` and staging `target/wasm32-unknown-unknown/release/fw_wasm.wasm`.

### aphrody

`aphrody-release.yml` already uploads `release-<os>-<arch>` artifacts whose directory holds the asset (`aphrody-x86_64.tar.gz`,
`aphrody-aarch64.tar.gz`, `Aphrody-Setup-x64.exe`, `Aphrody-arm64.dmg`), a `.sha256` and a `manifests/` directory: all three are handled (the last two are ignored).
Replace its `publish` job by the call, keeping its `resolve-metadata` outputs:

```yaml
  publish:
    needs: [resolve-metadata, build]
    permissions: { contents: write, id-token: write, attestations: write }
    uses: aphrody-dev/ay/.github/workflows/release.yml@main
    with:
      product: aphrody
      version: ${{ needs.resolve-metadata.outputs.version }}
      channel: ${{ needs.resolve-metadata.outputs.channel }}
      tag: ${{ needs.resolve-metadata.outputs.tag }}
      commit: ${{ github.sha }}
      artifacts-pattern: release-*
      tools-repository: aphrody-dev/ay
    secrets: inherit
```

Compatibility: the edge Worker keeps answering the legacy aphrody paths (`/api/releases/<channel>/latest`, `/assets/...`, `/install.sh`) while
the legacy objects (`channels/`, `manifests/`, `scripts/` in the same bucket) exist; new releases are read from the layout above.

### nie

`release.yml` uploads `release-linux` (`nie-v<version>-x86_64-unknown-linux-musl.tar.gz`) and `release-windows`
(`nie-v<version>-x86_64-pc-windows-msvc.zip`); the `{version}` of the patterns is the `version` input. Replace the `release` job (which runs
`just ci-release-publish`) by:

```yaml
  release:
    if: startsWith(github.ref, 'refs/tags/v')
    needs: [version, linux, windows]
    permissions: { contents: write, id-token: write, attestations: write }
    uses: aphrody-dev/ay/.github/workflows/release.yml@main
    with:
      product: nie
      version: ${{ needs.version.outputs.version }}
      channel: stable
      tag: ${{ github.ref_name }}
      commit: ${{ github.sha }}
      artifacts-pattern: release-*
      tools-repository: aphrody-dev/ay
    secrets: inherit
```

`nie-plugin` and `nie-abi` are verified for `wasm32-unknown-unknown` by `targets.yml` but no module is published (`ship = false`); change the row
and add the build when one should be.

### shenron

`release.yml` of shenron creates a draft release with `softprops/action-gh-release` and its own `SHA256SUMS`; its server and bot assets are described in
`release.toml` so that the same call works when its owner moves to the shared job. Studio installers are not classified yet.

## Why not the organization workflow

`aphrody-labs/.github` offers `release.yml` (GitHub Release from a semver tag, optional attestation). It runs in its own job on a clean runner, so it cannot
see artifacts produced by earlier jobs of the caller unless they are re-downloaded; it knows neither R2 nor the manifest. The publish job here does the same
Release and attestation steps with the same pinned actions after downloading the artifacts, and adds the R2 layout.

## Secrets

R2 credentials are repository or organization secrets, never in files or logs. Create an R2 API token limited to the releases bucket (Object Read and Write) and
set `CLOUDFLARE_ACCOUNT_ID`, `R2_ACCESS_KEY_ID`, `R2_SECRET_ACCESS_KEY`. `GITHUB_TOKEN` is the one of the calling repository.
