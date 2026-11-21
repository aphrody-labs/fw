# agy consolidation (2026-11-21)

Scope: the 587 Antigravity (agy) conversations exported to the private repo `aphrody-labs/agent-sessions` (commit a40160a, `sessions/agy/`), checked against the live state of the repositories on 2026-11-21 and against the roadmap `docs/roadmap/2026-os-monorepo.md`. agy is retired; this document and the lots it adds are the only carry-over. It holds no transcript text: conversations are cited by the last 8 hex characters of their id and by date. The sources are private and stay private.

## Method and what was verified

- Every one of the 587 files was classified. Work conversations were read through their user turns, the opening and closing agent turns and a search for commit ids and URLs: batches 1 to 7 by seven extraction agents, batches 8 and 9 by hand for the 26 work conversations; 404 reviews and 70 probes, 9 chats and 8 research files were classified from title and first turns only.
- Kinds: work 96, review 404, probe 70, chat 9, research 8. The date range is 2026-11-01 to 2026-11-20. Most reviews are strict commit reviews that agy produced for other sessions on 2026-11-13 to 2026-11-16 against the pre-split `src` monorepo; their findings concern commits that no longer exist in that form.
- Commits: 384 hash-like strings were collected from the work conversations (some are conversation-id fragments, so the unresolved share is inflated). Checked by `git rev-parse` and `git merge-base --is-ancestor <sha> origin/main` in the 13 local clones after `git fetch`, and by `gh api repos/<org>/<repo>/compare/<sha>...HEAD` for the repositories the conversations name: 124 are ancestors of `origin/main` of a live repository (fw, aphrody, ay, nie, shenron and others), 65 exist in repositories of `aphrody-labs` that are now archived, 59 exist in repositories outside the checked clones (the `yolo-rs` organization, `aphrody-dev`, the rosegriffon archive), 9 are on a diverged branch, 2 exist locally but not on main, 1 is on the default branch of a live repository and 124 could not be resolved (the repositories are gone or the token is not a commit).
- Artefacts: checked by `git ls-tree` on `origin/main` and by reading the live URLs (listed per item).

## Summary by status (work conversations: 96)

| Status | Count | Conversations | Basis |
|---|---|---|---|
| done-verified | 10 | 0cfb5ed4 a1acdb1c 98c8f07b 00864ae4 218f9b77 f0d6a000 5c551807 72d50cfe cfc9c526 b8cf2305 | Commits are ancestors of `origin/main` or the artefact exists live: shenron pure-Bun logger and Dragon Ball site fixes (0cfb5ed4, a1acdb1c, 98c8f07b, 00864ae4), fw Kotlin Material 3 library `core/kotlin/fw-material3` (cfc9c526; the package moved from `dev.aphrody` to `com.aphrody`), fw `fw bench` sandbox `20234bd0`, the CachyOS and sched_ext OS standard `eb05ceee` (72d50cfe), the HTTP/2 flag fix now in the global rules (5c551807), the `xh` skill (f0d6a000), the `C:\src` reorg finished by a Claude session (b8cf2305), the org inventory handoff (218f9b77). |
| done-unverified | 4 | 7d198649 bc13db2f deda54e0 e5ca9fe5 | Local PC changes with no commit (disk cleanup, scratch archives, eza duplicate scan). Not checkable from origin; no action. |
| in-progress | 9 | 55538917 cc63bbda 09b6b6e0 31b9eccf bbce9f27 c3380648 e20772ae e5ed984f f9401f71 | Unfinished and still relevant; each maps to a roadmap phase below. |
| abandoned | 19 | 77a5bbb5 6b13e5e9 e5a2ba3a 02dbbfeb c796f89a 1d5f7ec8 347e311f d6e873a6 4baad84e 18611a6e 8789d9eb 8dbfb82a 97dbcb5e e33581fd 1ec676b6 1f701adf 27390557 924e04db b505e68f | No deliverable recorded, or every tool call was blocked by the agy `PreToolUse` hook bug (27390557, 1f701adf, 924e04db, 1ec676b6, b505e68f). |
| superseded | 54 | 05f11ab3 8da054df 9009c130 9dbf515e 0bf1acde 82f38d08 9686fc8c df9f2297 f0ecf65e cd7ba482 1586a635 3365a1cf 3d78b806 81896789 90270c4a 964111a7 a9b1a56b d91585f1 e684ccd3 0fc83f5b 358742bf 4349e2a8 628f56f0 72668c23 eb59798d ebfb0862 f98e4c9f 085ec849 3a1f5107 cb38f5e7 f8fa322d 0468072a f2ee831b 05148740 094c356c 5fa1868c af143759 de67369a 2e33c12c 214b9a5f 24519707 17772f63 448c9605 54941b30 81e861fe e9184427 65122bce cc2adca3 2bb1e98c 32b4a09a 5d43a832 9938969a d302b943 9812f58e | Replaced by later work. Reasons below. |

Superseded, by reason:
- The yolo and bun-os era (2026-11-01 to 2026-11-12, 30 conversations): the `yolo-*`, `gemini-super-app` and `bun-os` repositories are archived (`gh repo list aphrody-labs`: `isArchived` true, last push 2026-11-13 to 2026-11-17), after the 2026-11-16 rebuild that renamed and purged the legacy trees (agy memory `ay-legacy-rebuild`). The commits still exist in the archived repositories.
- The single `src` monorepo era (2026-11-11 to 2026-11-18: af143759, 2e33c12c, 24519707, 214b9a5f, 448c9605, 17772f63, 54941b30, 81e861fe, 5d43a832, cc2adca3, e9184427, 32b4a09a): split into the six canonical repositories on 2026-11-20 and now merging into fw (roadmap decision 1). rosegriffon items (5d43a832, 81e861fe) are also stopped (`ay.toml` `[retired]`: project stopped on 2026-11-19).
- Browser and config merges now owned elsewhere: d302b943 and 9938969a (fw `fw-browser-merge`), 2bb1e98c (`aphrody-labs/claude` archived; the plugin lives in fw).
- agy-only configuration (05f11ab3, 8da054df, 9009c130, 9dbf515e, 05148740, 65122bce, 9812f58e, 81896789, d91585f1): the artefact lived in `~/.gemini`; 81896789 uninstalled Bun, which is installed again on the PC.

## Open items mapped to roadmap phases

Items already covered by a lot in `docs/agents/queue/`: browser merge and kitesurf to `fw-browser-merge`; Vertex, billing and credits (358742bf) to `gcloud-credits`; Bun fork merge and AOT/LTO (82f38d08, 24519707) to `bun-fork-merge-optimise`; pinned wasm toolchain to `wasm-toolchain-pin-optimize`; Google fonts and WOFF2 (448c9605) to `material-cdn-a-la-carte`; terminal and WebGPU benchmarks (448c9605, 4baad84e) to `gpu-bench`; the Lucy and Byron avatar and Blender research files (8 research conversations) to `assets-lab`; docs, skills and memory generated from code to `derive-names-docs-from-code`; Android to `android-lab`.

New lots added with this document, in priority order for the release train:

| # | Lot | Phase | Owner | In v2026.11.0 |
|---|---|---|---|---|
| 1 | `fw-mcp-conformance` | 2 | Claude Desktop session "Audit et déploiement du framework" | blocks (fw CLI and MCP artifact) |
| 2 | `cloudflare-estate-and-shenron-cutover` | 0 then 2 | background worker: fleet build cluster and production to Cloudflare | blocks (DNS, subdomains, tunnels, buckets) |
| 3 | `aphrody-gpui-m3-landing` | 1 then 2 | background worker: fw workspace unification | belongs (UI framework and design system) |
| 4 | `agy-retirement` | 0 | Claude Desktop session "Audit et déploiement du framework" | not blocking; do before the plugin freeze |
| 5 | `agent-sessions-secret-scan` | 0 | unassigned | gate for any use of the sessions repo |
| 6 | `forks-sync-and-multimedia` | 1 | unassigned (pin policy: unification worker) | no |
| 7 | `os-nix-vs-cachyos` | 3 | unassigned until the Phase 1 merge | no |
| 8 | `nie-ievr-data-pipeline` | 2 and later | unassigned | no |

Other open items without a lot, because the work belongs to a running worker or is moot: `depot_tools` and `ay-build` parity (agy 2bb1e98c audit, 54941b30 failing `depot_gclient::deps::parse`) belongs to the `fw depot` worker; Vercel in Rust (8dbfb82a, abandoned) is replaced by `crates/fw-edge` and `crates/fw-http` (Phase 2); the whole yolo era backlog is dropped.

## Conflicts and recommended resolutions

| # | Conflict | Evidence | Resolution |
|---|---|---|---|
| 1 | agy rules route coordination through a relay hub; the relay was removed on 2026-11-16 | 40 files in fw `plugins`, `configs`, `tools`, `docs` mention it, 13 under `configs/gemini` | Replace with `claude_inject` and `notify.ts` at the source; add a `--check` (lot `agy-retirement`) |
| 2 | agy rules, skills and the live global `CLAUDE.md` name fr2 as tests and research host; fr2 is out of service | 23 fw files mention fr2 (11 outside `configs/gemini`); fw `configs/claude/CLAUDE.md` already dropped it, the live copy did not | Re-sync the live rules from source; fix the skills `fleet` and `ai-os-orchestrator`, `plugins/aphrody/rules/AGENTS.md`, `docs/architecture/os-strategy-2026.md` |
| 3 | agy fleet roles: prd is production only, no build tooling; the cluster decision makes prd a build and agent host in a slice while production moves to Cloudflare | cluster doc 2026-11-21, roadmap Phase 0 | Cluster doc wins; update the `fleet` skill |
| 4 | agy rule of 2026-11-17: repositories are independent, linked only by published versions; the roadmap merges ay and aphrody into fw | roadmap decision 1 and 4 | Roadmap wins; the rule becomes the enforced layer check (`cargo deny` bans plus `xtask`) |
| 5 | agy rule: aphrody is the source of truth, ay an independent SDK; `ay.toml` still says only the os repository defines the system | `ay.toml` `[repos]` comment | After the merge, `product.toml` at the fw root is the single identity file; rewrite that comment in the merge |
| 6 | Nix flake and host modules (agy 72d50cfe) against the CachyOS image (roadmap Phase 3, cluster doc rejects Talos) | `flake.nix`, `nix/modules/vps-common.nix` on fw `origin/main` | Record an ADR: CachyOS image is the OS artifact; keep Nix only where a job uses it (lot `os-nix-vs-cachyos`) |
| 7 | User rule of 2026-11-17: no PC, VPS or account data in tracked docs; public fw queue docs carry billing and project identifiers | `docs/agents/queue/gcloud-credits.md` holds one billing account id on 5 lines, plus project and bucket names | The newer roadmap deliberately records measured sizes: keep sizes by role. Move identifiers (billing, project, bucket, account names) to the private sessions repo and leave a pointer. The owner of that lot does it |
| 8 | Two learning stores: `learn.aphrody.com` (agy loop, revision 21, 1 pc) and `fw learn` with `fw-corpus` | live JSON read 2026-11-21; `plugins/aphrody/skills/learn/SKILL.md` | One store: fw. Repoint or retire the Worker (lot `agy-retirement`) |
| 9 | Two memory stores: `configs/gemini/memory` (46 files, mirrors agy `ws-*`) against the Claude memory directory | fw tree; memory index | Claude memory is the single store; delete the mirror with `configs/gemini` |
| 10 | Two toolchain configs: `.github/templates/toolchain.toml` (agy memory `toolchain-policy-and-ay-tools`) and `fw/configs/claude/toolchain.toml` | both exist on origin/main | fw file is canonical (roadmap Phase 3); make `.github` template a generated copy or delete it |
| 11 | agy-era model id `gemini-decision-1` and endpoint `v1beta/decisions` in `cluster/cloudflare-gcloud.toml`, `fw-proto`, `fw-wasm`; `gemini-4-neon` in the agy-knowledge references | commits 9b74733d and be78c591 (2026-11-19, the fairwind period of conversation 72d50cfe) | Verify against the live model list; delete what is absent |
| 12 | agy rule from 2026-11-20 "pwsh only, Bun and Rust uninstalled" against the PC today | `bun.exe` present under `D:\cache\bun\bin` | Drop the rule; shells sync via `bun $WORKSPACE_ROOT/ay/tools/shells.ts` |
| 13 | agy rules "agy audits and researches only; never launch agy.exe; aphrody owned by agy" | agy memory `agy-coordination` | Moot; remove with the retirement |
| 14 | Duplicate efforts with running actors: browser merge (fw-browser-merge), plugin unification (Claude Desktop session), Cloudflare estate and prod (fleet worker), depot_tools in Rust (`fw depot` worker), unification of ay and aphrody (workspace worker) | agy 9938969a, d302b943, 2bb1e98c, 31b9eccf, 54941b30 | Do not redo; the lots above hand the unfinished slice to the owning actor |

## Memory

Added to the Claude memory directory (11 files, one fact each, indexed in `MEMORY.md`): `no-private-data-in-tracked-files`, `secrets-handling-style`, `licenses-never-gate-architecture`, `build-not-inventory`, `english-source-baseline`, `goals-not-version-numbers`, `pc-cleanup-keep-list`, `fast-userland-clis`, `track-canary-upstreams`, `hosting-cost-policy`, `human-steps-low-vision`.

Not copied, listed as stale (they contradict the facts above):
- relay hub rules: agy `fleet-and-relay`, `cross-session-coordination`, `agy-coordination`, the relay lines of `learned-auto`, `learned-cloud` and `GEMINI.md`.
- fr2 as host: `fleet-and-relay`, `fleet-disks` (NFS on fr2), `budget-and-hosting` (rpbey.fr to fr2), `autonomy-nhitl` (staged rollout proven on fr2).
- prd as production only: `fleet-and-relay`.
- independent repositories: `repo-independence`; aphrody as source of truth: `ay-hub-source-of-truth`; checkout map and org layout: `git-and-workspace-map`, `ay-legacy-rebuild`, `ai-os-repo` (ai-os archived), `kitesurf-engine-workspace` (kitesurf archived, merged into fw), `npm-trusted-publishing` (rosegriffon stopped).
- toolchain pointer to the `.github` template and the `ay-tools` repo (archived): `toolchain-policy-and-ay-tools`.
- agy-tool hygiene with no meaning after retirement: `windows-cmd-quoting`, `modern-shell-windows`, `antigravity-plugins`, `gemini-isolation`, `edit-and-sqlite-hygiene`, the `*-brain` indexes, `learned-*`, the `wt-agent-hooks` notes, and `journal.jsonl` (162 tool-error lines).
- early-November product rules: no HTML, CSS or JS files; the `yolo-rs` organization as remote; Byron Love as only logo; "Bun is Rust only without Zig"; Wii and Budokai product goals.

`~/.gemini` was not edited.
