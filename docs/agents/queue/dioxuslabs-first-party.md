# Lot: DioxusLabs is as vital as GPUI. All crates first-party, patched and current

Owner order, 2026-11-21: every DioxusLabs crate becomes first-party in the same way as GPUI. No half measures through crates.io releases or third-party pins. The crates are:
- patched with what Zed and Cloudflare patch;
- tuned for Google Design / Material 3 and Android;
- kept at minimal build time.

## Done

- **Forks in aphrody-labs**, kept current hourly by `forks-follow.yml`:
  - already present: dioxus, taffy, blitz, anyrender, rustd, accessibility-cli, dioxus-components, dioxus-sdk, vello;
  - forked today: stylo (from DioxusLabs/stylo), parley and resvg (from DioxusLabs).
- **Audit:** `docs/research/dioxus-taffy-blitz-compose-2026-11-21.md`.

## Facts that drive the work

- **Taffy is split across two versions.** Zed's GPUI pins `taffy = "=0.15.0"` from crates.io, Blitz pins a git rev, and fw's lock has 0.15.1. One first-party taffy at upstream HEAD, patched for every consumer, removes the split.
- **Dioxus:** 0.8.0-alpha.3 rewrote the rendering core and still regresses. Blitz beta tracks it, and the native path fixes RefCell panics (#5943).
- **`crates/fw-ui-blitz` is not Blitz.** Its only dependencies are flate2 and serde. fw-bin uses it in two places:
  - the `fw ui` command, which draws a fake "cluster HUD" from the fictional topology into `target/blitz-proof.png`;
  - `m3_agent_tui.rs`, which encodes a PNG of hardcoded colours.

  All three are to be deleted, and replaced by real Blitz rendering where a HTML render is needed.

## Work

1. **Wire the forks.** In `layout.toml` (the crate map), add a `[patch.crates-io]` and git entry for every DioxusLabs crate in the graph (taffy, dioxus-*, blitz-*, anyrender*, parley, stylo, vello, resvg). Point each at our fork at upstream HEAD, the same way the kube patch was proven. Then run `bun tools/layout.ts sync` and `cargo check --workspace`.
2. **Unify the pin.** One taffy for GPUI, Blitz, nsh-core and ay-design. Upstream any GPUI compile fix to the aphrody fork of zed.
3. **Bring in patches.**
   - A workflow lists open upstream PRs and issues with fixes, and the patches Zed and Cloudflare carry: Zed's Cargo.toml `[patch]` and forks, and Cloudflare's crates.
   - It applies the green ones onto our forks automatically: rebase, CI gate, then merge to our fork's main.
   - forks-follow keeps HEAD current.
4. **Google Design and Android.** Blitz and Dioxus render with the M3 tokens from `fw m3 sync` (lot `m3-is-the-brand.md`). Android goes through the dioxus mobile renderer or Compose, per the audit's per-surface decision.
5. **Build time.** Measure clean and incremental build of the Dioxus/Blitz members before and after: features trimmed, codegen-units, the linker per the mold audit. Keep only measured wins.
6. **Purge `fw-ui-blitz`:**
   - delete the crate, the `fw ui` arm and import in `crates/fw-bin/src/main.rs`, and the PNG writer in `m3_agent_tui.rs`;
   - remove the entries in `layout.toml`, `product.toml` `crates_io_publish_order` and the root Cargo.toml;
   - regenerate the docs pages under `apps/docs` (`crates/fw-ui-blitz`, the lists, llms, the sitemap);
   - verify that `cargo check -p fw-bin` passes on a node.
