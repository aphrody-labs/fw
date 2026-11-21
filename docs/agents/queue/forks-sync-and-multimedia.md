# forks-sync-and-multimedia: keep the media and engine forks merged with upstream (queued)

Phase 1 (forks as pinned dependencies and relative-URL submodules). Owner: the workspace-unification worker for the pin policy, then unassigned for the pipeline. Release v2026.11.0: not blocking.

## Goal
agy conversation f9401f71 (2026-11-20) wrote a migration plan for yt-dlp, hyperframes, remotion, watch-skill and three.js and a rule that forks stay as auto-merged GitHub forks instead of being copied into the monorepo (they total more than 6.7 GB). The pipeline that auto-merges upstream into these forks does not exist yet; the Bun fork's sync is its own lot.

## Facts checked 2026-11-21 (`gh api`)
- Forks that exist in `aphrody-labs`: yt-dlp (parent yt-dlp/yt-dlp), remotion (parent remotion-dev/remotion), three.js (parent mrdoob/three.js), watch-skill (parent oxbshw/watch-skill); hyperframes exists and is archived. The agy statement that remotion still had to be forked is stale.
- Upstream commits not yet in the fork: yt-dlp 1, remotion 54, three.js 0 (the fork holds 6 commits upstream lacks).

## Do
1. One typed Bun workflow (`tooling/bun/`) that, per fork listed in `upstreams.toml`, opens a merge from upstream, runs the consumer's check, and pushes only on green; report the ahead and behind counts as a table.
2. Decide per fork: pinned Cargo git dependency, relative-URL submodule, or plain reference; each fork has one owner repo (`ay.toml` rule).
3. Port the media features the plan names (YouTube and YouTube Music inspection, video composition) to `crates/` only where a feature-matrix row is open; do not copy upstream trees.

## Acceptance
A scheduled run keeps the four forks at 0 behind upstream, with the check result recorded per merge.
