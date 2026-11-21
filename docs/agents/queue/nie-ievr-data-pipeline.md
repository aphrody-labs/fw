# nie-ievr-data-pipeline: Inazuma Eleven data export and the browser build of the nie UI (queued)

Phase 2 and later (nie is stack item 4, outside the v2026.11.0 release train). Owner: unassigned. Release: not blocking.

## Goal
Finish the work agy started in `nie` that never landed on origin: an end-to-end export of Inazuma Eleven Victory Road data (game packs, locales, characters, the zukan and wiki pages) into one archive, a pack inspector that renders through WebAssembly and WebGPU, and a nie WASM UI whose menus and gamepad flow match the nie executable.

## Facts checked 2026-11-21
- agy conversations 214b9a5f, 4baad84e, d6e873a6 (2026-11-13 and 2026-11-14) claim a `nie-archive.ts` pipeline and a `packs-inspector.ts` (Bun server, SQLite resume, WASM and WebGPU view). Neither path exists on `origin/main` of `aphrody-labs/nie` (`git ls-tree -r` has no match for `nie-archive`, `packs-inspector` or `zukan`).
- cc63bbda and 55538917 (2026-11-11) list a rebuild of the nie WASM, UI and runtime from launcher to menus, driven by gamepad, as an open request; 14 commits of that conversation are on origin (see the consolidation doc).
- Rules from the same conversations: tests run against the real Chrome, the real game and the same DB as the nie CLI; assets stay private (an assets-lab style private store); no stubs.

## Do
1. Inventory what `nie` already has (`crates`, `apps`, `gui`, `docs`) and what the nie CLI can dump; write the missing exporter as a typed Bun script under `nie`.
2. Run it on the PC against the local game install; keep the archive out of git (private store).
3. Pack inspector in WebGPU with a Canvas2D fallback only for tests; verify in Chrome with screenshots.

## Acceptance
Exporter reproducible from a clean clone; archive manifest hash recorded; inspector screenshot with real pack data.
