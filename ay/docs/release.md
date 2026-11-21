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

### fw, aphrody, nie

All three already call the workflow from their own release workflow: `.github/workflows/release.yml` in `fw` and `nie`,
`.github/workflows/aphrody-release.yml` in `aphrody`. Each build job uploads one artifact per asset named `release-*`, the file named like the asset, and
a `publish` job calls this workflow (shown for `fw`):

```yaml
  publish:
    needs: build-artifacts
    permissions: { contents: write, id-token: write, attestations: write }
    uses: aphrody-labs/ay/.github/workflows/release.yml@main
    with:
      product: fw
      version: ${{ needs.meta.outputs.version }}
      channel: ${{ needs.meta.outputs.channel }}
      tag: ${{ needs.meta.outputs.tag }}
      commit: ${{ github.sha }}
      artifacts-pattern: release-*
      tools-repository: aphrody-labs/ay
    secrets: inherit
```

Asset names per product are the rows of `release.toml`. `aphrody` also uploads a `.sha256` and a `manifests/` directory per artifact; the publisher ignores both.
The edge Worker keeps answering the legacy aphrody paths (`/api/releases/<channel>/latest`, `/assets/...`, `/install.sh`) while the legacy objects
(`channels/`, `manifests/`, `scripts/` in the same bucket) exist; new releases are read from the layout above. `nie-plugin` and `nie-abi` are verified for
`wasm32-unknown-unknown` by `targets.yml` but no module is published (`ship = false` in `release.toml`).

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
