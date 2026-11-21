# aphrody-gpui-m3-landing: keep the GPUI, NSH and Material 3 work that agy pushed alive through the aphrody-into-fw merge (queued)

Phase 1 (merge, owner: the fw workspace unification worker) then Phase 2 (UI framework and design system). Release v2026.11.0: belongs, the roadmap lists GPUI, gpui-kit and fw-m3 published with docs and examples.

## Goal
Make sure the unfinished agy work in `aphrody` survives the merge and gets an independent gate, instead of being recreated.

## Facts checked 2026-11-21
- Commit `9173880c58` ("chore(ci,tooling,release): repo-native automation, cross-build CLI ...", agy conversation 2bb1e98c, 2026-11-18) is an ancestor of `origin/main` of `aphrody-labs/aphrody`.
- agy conversations e20772ae and e5ed984f (2026-11-18) merged our GPUI with Zed's for NSH and gpui-kit, vendored `egui`, `wry`, `craft-libs`, `craft-anything`, `mullvad` and browser under `vendor/`, and left cargo fixes (`gpui-component` native menu and popup menu, `has_srgb_suffix` in the vendored egui renderer) whose verification was still running. `aphrody` has no `vendor/` directory today (its rule: forks are pinned Cargo git dependencies), so those trees were replaced; check the fixes survived as patches or upstream PRs.
- agy conversation f9401f71 (2026-11-20) reports Material 3 1.5.0 tests for `gpui-component`, `gpui_ui --features material3`, the `m3_compose15_showcase` example and `cargo xtask brand check` passing, and an `aphrody_cli suite --json` command; it was integrating `material-colors 0.5.0`, `md-icons 0.4.0`, `backdrop-blur-core 0.2.0` and `window-vibrancy 0.8.1`.

## Do
1. On the Phase 1 scratch merge, run those suites and `brand check`; record pass or fail per suite.
2. Any failing or missing piece becomes a handoff row here with the commit that introduced it.
3. Publish gpui-kit and fw-m3 per Phase 2 with a rendered screenshot per component (the independent gate renders them).

## Acceptance
All named suites pass on the merged SHA; the independent gate screenshots exist for the showcase example.
