# release-pages: downloads, security and feed pages from release.toml (queued)

Spec: matrix rows 21 to 24; section 2 (Downloads, Security). Roadmap: M7.

## Goal
A downloads page per product built from `release.toml` and the release manifest (`ay/apps/edge/src/manifest.ts`): assets by os, arch and abi with real sizes and SHA-256, the channel table of the release train, a security page of real counts, and an Atom feed; no address collection.

## Done
- `release.toml` has rows for fw, aphrody, nie, ay and shenron; the milestone dates come from `tools/release-plan.ts`.

## Remaining
1. Page generator reading `release.toml` and the manifest; channels from `[channels]`.
2. Security page: SBOM count from `fw export`, `deny.toml` ban count, `SHA256SUMS` presence per release, each read at build time.
3. Atom feed of releases; no newsletter form.
4. `[products.bun]` rows for the Bun fork (assets decided with the Bun fork lot, `bun-fork-merge-optimise.md`).

## Proof
Generated pages compared with the live manifest response; every link answers with the byte size shown.
