# How the plan keeps itself current

Three loops, from cheap and certain to expensive and judged. Each writes only through a commit with its evidence.

1. **Probe loop, no AI** (`.github/workflows/plan-sync.yml`, daily and on demand). `bun tools/claims.ts` runs the 24-plus
   probes of `docs/research/claims.toml` against live sources (kernel.org, AOSP, GitHub API, registries, Cloudflare docs, CI
   states) and writes `docs/research/claims.md` and `claims-status.json`; `bun tools/plan.ts` renders `docs/plan.md` from
   `docs/plan/work.toml` and those results. The job commits the three files when they changed.
2. **Triage loop, agent** (run from a Claude session on the PC or the test host when `docs/plan.md` lists a claim that does not hold, or on
   request). For each failed or unreadable claim:
   - read what changed upstream with a repository digest: `gitingest <repo-url>` (installed on the PC through uv) and, for a
     library or API, Context7 through the relay hub (`hub_call`, skill `aphrody:context7-deep-research`), live source before
     documentation;
   - research the new fact with `WebSearch` and `WebFetch`, keep the URL and the retrieval date, never a remembered value;
   - fix the document or code that relied on the claim, update the claim's expectation in `claims.toml` (the old one was
     wrong, or the world moved), commit by path with the evidence in the message.
3. **Fact-check loop, agent** (before a report to the owner). Every number or capability stated in a reply is either a row of
   `claims.toml` with a probe or is written as unverified, as the session review of 2026-11-17 did
   (`docs/research/session-review-2026-11-17.md`: 23 claims, 2 found false).

## The dig rule
Published version numbers are the least useful data. For every goal in `work.toml`, dig everything that can change the design before
building: local data (the repositories and their history, the legacy archive, memory and past session transcripts, fleet hosts) and
remote data (GitHub code, commits, issues and compare, kernel and AOSP git, mailing lists on lore.kernel.org, registries, vendor docs,
prior art in repository search), then write the findings with their sources and dates under `docs/research/` and turn the
design-deciding ones into claims. A goal's research note names what is still to dig. Example:
`docs/research/linux-android-goal.md` found that mainline removed the C binder, which makes Rust mandatory for Android.

Work enters the plan by editing `docs/plan/work.toml`; `bun run check` fails when `docs/plan.md` is not what the sources generate.
