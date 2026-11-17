# How the plan keeps itself current

Machine- and work-specific data never enters git. Tracked files hold code, schemas, specs and standards; everything an agent
generates about this machine, the fleet or the state of the work lives in the git-ignored private store (`.private/`, or the
directory named by `AY_PRIVATE_DIR`; `tools/local.ts`) and local overrides live in the git-ignored `ay.local.toml`
(`ay.local.example.toml` shows the shape with placeholders).

What is private: the work ledger and the plan page, the claim probes and their results, the inventories (organization, legacy,
recovery), audits and telemetry of the fleet, session reviews, research notes about the goals, learning digests.

Three loops write there; none commits.
1. **Probe loop, no AI**: `bun tools/claims.ts` runs the probes of `.private/claims.toml` against live sources and writes the results;
   `bun tools/plan.ts` renders `.private/plan.md` from `.private/plan/work.toml` and those results. Schedule it on the fleet's
   build host (cron or a systemd timer), not in CI: a public repository's workflow output is public.
2. **Triage loop, agent**: for each claim that fails or cannot be read, read what changed upstream with a repository digest
   (`gitingest <repo-url>`) and, for a library or an API, Context7 through the relay hub (skill `aphrody:context7-deep-research`); research
   the new fact with WebSearch and WebFetch, keep URL and retrieval date; fix the private note or the tracked code that relied on it.
3. **Fact-check loop, agent**: before a report to the owner, every number or capability is a probe or is stated as unverified.

The dig rule: published version numbers are the least useful data. For every goal, dig everything that can change the design
before building (local repositories and history, the legacy archive, memory and past transcripts, fleet hosts; GitHub code,
commits and compare, kernel and AOSP git, mailing lists, registries, vendor docs, prior art in repository search), write the
findings with sources and dates into `.private/research/`, and turn the design-deciding ones into probes.
