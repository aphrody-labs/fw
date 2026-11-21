# reference-measure: re-measure the reference and fail on drift (queued)

Spec: matrix row 26, G16. Roadmap: M6.

## Goal
`tools/reference-measure.ts` repeats the measurements recorded in `google-design.md` against the live reference (document bytes, asset counts, video ladder, headers, lazy-loading behaviour) and compares them with the recorded values.

## Done
- The measurement method is recorded in `google-design.md` (Resource Timing, `HEAD` from the page for Content-Length, `curl` headers, `ffprobe`).

## Remaining
1. Script (Bun, Chrome through CDP) that outputs a JSON of the same facts; `--check` compares with `docs/product/reference.json` and prints the drift.
2. Weekly run on the cluster; a drift opens a note in the private store, not a commit to design references.
3. Re-run on `googlebook.google` pages for other locales to confirm the ladder is locale-independent.

## Proof
`--check` green on the day it lands, and red on a deliberately altered recorded value.
