# Becoming reviewers and team members upstream

Facts below were read live on 2026-11-17 (GitHub API, raw files). Anything marked "unverified" was not found in a primary source.

**Hard rule: nominations require existing team members. Every team adds members by consensus or vote of current members; there is no application form, no payment and no shortcut. No process here can be bypassed, and none of this plan tries to.**

## 0. Cross-cutting constraints

- **LLM policy (binding).** `rust-lang/rust`, cargo, rust-clippy and rustfmt adopt https://forge.rust-lang.org/policies/llm-usage.html: LLM-written comments, issue bodies, PR descriptions and docs are banned; LLM-created code is allowed only as an "experiment" that is pre-arranged with the same reviewer who will review the PR, non-critical, disclosed and labeled `llm-assisted`; new contributors cannot do it without that talk first. Other uses need disclosure. Miri: https://github.com/rust-lang/miri/blob/master/CONTRIBUTING.md (LLM code only if a maintainer already agreed to mentor you). rust-analyzer: https://github.com/rust-lang/rust-analyzer/blob/master/AI_POLICY.md (no autonomous agents opening PRs or issues, comments written by humans, no AI-authored code on E-easy+E-has-instructions, rustc citation required for analysis crates; a PR was closed for violating it: #23645). crates.io: disclosure appreciated, not required (https://github.com/rust-lang/crates.io/blob/master/docs/AI-TOOLS.md). Bun: no policy found in `CONTRIBUTING.md`, `docs/project/contributing.mdx` or `.github`; its own bot `robobun` opens PRs (unverified policy).
- Consequence for Aphrody: every upstream PR, comment and issue is written and understood by a named human; agents may be used privately for reading code and reviewing our own drafts. An agent-driven account would be closed and would block any later membership.
- Zulip is the main discussion channel (https://rust-lang.zulipchat.com) for all rust-lang repos.
- Review rotation lives in each repo's `triagebot.toml` `[assign.owners]`; adding a person there is a step of "full team membership" (https://github.com/rust-lang/rust-forge/blob/master/src/infra/team-maintenance.md).
- Triagebot commands: `@rustbot claim` / `release-assignment` (https://forge.rust-lang.org/triagebot/issue-assignment.html), `@rustbot ready` / `author` (PR status), `r? @user` (request a reviewer), `@rustbot label +x`. Non-members may only set labels allowed in `[relabel] allow-unauthenticated`.
- Team membership is a PR to https://github.com/rust-lang/team (`teams/*.toml`); changes that grant permissions need a team lead's approval (team-maintenance.md above).

## 1. rust-lang/rust (compiler team)

- **Path** (https://github.com/rust-lang/rust-forge/blob/master/src/compiler/membership.md): start with "one off" issues or a working group, assigned via triagebot, usually with a mentor. Member criteria: "staying power", "independence and familiarity" (could mentor simple PRs), "cordiality"; "most important criterion ... regular and consistent participation"; not only code (triage, bisecting, docs, rustc-dev-guide count). Promotion: nominated by a member or you ask the leads; leads check with the team; invitation if no objections, answer aimed within a week. Maintainer role only after one year as member.
- **Privileges**: `r+` via bors, perf/crater bots, rust-lang org membership, listed on the website. Members are then asked whether to join the reviewer rotation (`[assign.owners]` in https://github.com/rust-lang/rust/blob/master/triagebot.toml, e.g. `"/compiler" = ["compiler"]`). The `compiler` team (leads davidtwco, BoxyUwU) has `bors.rust.review = true`.
- **Labels**: E-easy, E-mentor, E-help-wanted; official search in https://github.com/rust-lang/rustc-dev-guide/blob/master/src/getting-started.md. State of work: unclaimed E-easy/E-mentor issues are rare (see issues.md).
- **Test**: `./x test` (builds stage 1 compiler; slow), `./x test tests/ui`, `./x test tidy`; library/doc changes avoid compiler builds. Needs a Linux build host (`<build-host>` role).
- **Merge**: bors (`@bors r+`) by team members only; others get reviewed by assigned reviewer.

## 2. rust-lang/cargo (team `cargo`, parent `devtools`)

- **Team**: https://github.com/rust-lang/team/blob/master/teams/cargo.toml (leads Eh2406, weihanglo; 8 members). Contributor guide: https://doc.crates.io/contrib/ ; team chapter https://github.com/rust-lang/cargo/blob/master/doc/contrib/src/team.md.
- **Becoming a member**: "requesting a review or being nominated by one of the existing members", added by "unanimous consent of the team", moderation team consulted. Expectations: participate in some of meetings, reviewing contributions, triage, mentoring, shepherding, maintenance.
- **Rotation**: `[assign.owners] "*" = ["@epage", "@weihanglo", "@Muscraft"]` (https://github.com/rust-lang/cargo/blob/master/triagebot.toml).
- **Rules**: only issues marked `S-accepted` are reviewed (CONTRIBUTING.md); claim with a comment or `@rustbot claim`; labels E-easy/E-medium/E-hard, S-needs-mentor; office hours https://github.com/rust-lang/cargo/wiki/Office-Hours. Cargo uses the GitHub merge queue (`[merge] type = "merge-queue"`), not `r+` by outsiders.
- **Test**: `cargo test` (nightly recommended; `rustup component add rustc-dev llvm-tools-preview`), targeted: `cargo test --test testsuite -- check::check_success`; `CFG_DISABLE_CROSS_TESTS=1`; `cargo fmt`; typos check. https://github.com/rust-lang/cargo/blob/master/doc/contrib/src/tests/running.md
- Repo layout now `src/{compiler,context,resolver,ops,workspace,sources,util}` (verified).

## 3. rust-lang/rust-clippy (team `clippy`, parent `devtools`)

- **Team**: https://github.com/rust-lang/team/blob/master/teams/clippy.toml (leads Manishearth, flip1995, 12 members, `bors.rust.review`). Lower tier `clippy-contributors` (6 members: triage help, "no direct onboarding process").
- **Path** (Clippy book "The team", https://github.com/rust-lang/rust-clippy/blob/master/book/src/development/the_team.md): contributors first; after being active "we'll probably reach out" and ask you to help with reviews; you review assigned PRs with a mentor, who co-r+s; when the mentor is confident they start an informal vote among active members ("usually unanimous"). Duties: timely PR responses, Zulip, bi-weekly meeting, rustc sync, changelog. Inactive three months moves to alumni.
- **Community reviews**: `[assign.community_reviews] minimum_approvals = 2` (label `S-waiting-on-community-reviews`): PRs need two community reviews before auto-assignment; reviewing others' PRs is the explicit on-ramp (CONTRIBUTING.md).
- **Rotation**: `[assign.owners] "*"` = Manishearth, llogiq, Alexendoo, dswij, Jarcho, y21, samueltardieu, ada4a, blyxyas.
- **Labels**: `good first issue` (no LLMs there), E-medium, T-AST, I-false-positive, `@rustbot claim`.
- **Test**: `cargo test`; `cargo uitest`; `TESTNAME=foo cargo uitest`; `cargo bless`; `cargo dev fmt`, `cargo dev dogfood`, `cargo dev update_lints`, `cargo dev new_lint` (https://github.com/rust-lang/rust-clippy/blob/master/book/src/development/basics.md). Every PR needs a `changelog:` line.

## 4. rust-lang/rustfmt (team `rustfmt`, parent `devtools`; style defined by team `style`)

- **Team**: https://github.com/rust-lang/team/blob/master/teams/rustfmt.toml (lead ytmimi; members calebcartwright, ytmimi, jieyouxu). Tier below: `rustfmt-contributors` ("Helping the Rustfmt team with reviews and planning", 5 members). Style-guide decisions belong to the `style` team (lead calebcartwright), not here.
- **Path**: no written membership doc found (unverified); realistic path is regular reviews/triage on Zulip `#t-rustfmt`, then an invitation from the lead.
- **Rules** (https://github.com/rust-lang/rustfmt/blob/master/Contributing.md): PR against `main`, `r? @user` only if they asked; `@rustbot ready`/`author`; LLM policy as above; gate new behavior by style edition/unstable option.
- **Test**: `cargo test` (nightly, rustc-dev); snapshot tests in `tests/source` + `tests/target`; Diff Check CI by reviewers.

## 5. rust-lang/miri (team `miri`, parent `compiler`)

- **Team**: https://github.com/rust-lang/team/blob/master/teams/miri.toml (leads RalfJung, oli-obk; 3 members; only `bors.rust.try`).
- **Path**: no written membership document found (unverified). Larger work goes through the 3-step goal/design/implementation process in CONTRIBUTING.md; sustained shim/diagnostic contributions are the evidence.
- **Rules**: green `E-good-first-issue` labels; `@rustbot ready` after changes; `./miri squash` to one commit; 1-2 week review latency stated in triagebot message. Reviewers are the three members.
- **Test**: `./miri toolchain`, `./miri build`, `./miri test` (`--bless` to update stderr), `./miri run file.rs`, `./miri fmt`, `./miri clippy`; `MIRI_SKIP_UI_CHECKS=1`. https://github.com/rust-lang/miri/blob/master/CONTRIBUTING.md

## 6. rust-lang/rust-analyzer (team `rust-analyzer`, parent `compiler`)

- **Team**: https://github.com/rust-lang/team/blob/master/teams/rust-analyzer.toml (lead Veykril; 9 members, `bors.rust.review`). Tier below: `rust-analyzer-contributors` ("Contributing ... on a regular basis", 5 members).
- **Path**: no written membership process found (unverified); contributions to `crates/ide*`, `hir*`, plus mentoring on issues; ask via Zulip `#t-compiler/rust-analyzer`. Feature freeze for new IDE assists is in force (CONTRIBUTING.md).
- **Rules**: bugs `C-bug`, E-easy + E-has-instructions are mentored learning issues (no AI code), analysis crates (`parser`, `mbe`, `hir-expand`, `hir-def`, `hir-ty`) need a cited rustc reference; triagebot has no `[assign.owners]` rotation (reviews are ad hoc).
- **Test**: `cargo test` (or `cargo test -p ide-diagnostics`); `cargo xtask codegen`/`tidy` (unverified, check the book's testing page); book: https://github.com/rust-lang/rust-analyzer/tree/master/docs/book/src/contributing (README, architecture, testing, style).

## 7. rust-lang/crates.io (team `crates-io`, parent `devtools`)

- **Team**: https://github.com/rust-lang/team/blob/master/teams/crates-io.toml (leads jtgeibel, Turbo87; 6 members, `crates-io-admin`). Notably **`crates-io-reviewers`** (https://github.com/rust-lang/team/blob/master/teams/crates-io-reviewers.toml): marker team "of people who review crates.io pull requests. It is not limited to members of the crates.io team" (5 members incl. jlizen, who is not on the main team). This is the most concrete outside-in reviewer path in the whole list.
- **Path**: weekly team meeting, Fridays 11:00 US east, open to everyone (CONTRIBUTING.md); review other people's PRs per https://github.com/rust-lang/crates.io/blob/master/docs/PR-REVIEW.md; reviewer addition is by the team (process not written; unverified).
- **Rules**: `E-has-mentor` / `E-help-wanted` labels (0 currently open and unassigned), doc comments on new items, ESLint/clippy/rustfmt in CI.
- **Test**: backend `cargo test`, `cargo insta accept --workspace`, `cargo fmt --all --check`, `cargo clippy --all-targets --all-features --workspace` (needs Postgres 16, `TEST_DATABASE_URL`; devcontainer recommended); frontend `pnpm --filter crates.io-svelte test`, `pnpm e2e:svelte` (docs/AGENTS.md, docs/CONTRIBUTING.md).

## 8. oven-sh/bun

- **Path**: none documented (verified absent in `CONTRIBUTING.md`, `docs/project/contributing.mdx`, `REVIEW.md`; `.github/CODEOWNERS` lists three paths owned by two people). Merge rights are held by oven-sh staff; reviewer status is by employment or invitation (unverified). Realistic goal: merged PRs that follow `REVIEW.md`, not membership.
- **Quality bar**: https://github.com/oven-sh/bun/blob/main/REVIEW.md and `.claude/docs/landing-prs.md`: every behavior change ships an automated test that fails on the unfixed build (`USE_SYSTEM_BUN=1 bun test <file>` fails, `bun bd test <file>` passes), one concern per PR, run `rust:check-all` (CLAUDE.md), cross-platform checks.
- **Test**: `bun bd` (debug build, no timeout), `bun bd test test/js/bun/http/serve.test.ts -t "name"`; ~10 GB disk; Windows: `docs/project/building-windows.mdx`. Bun is now Rust (workspace of ~200 crates, `src/runtime`, `src/jsc`, `src/install`) with C++ JSC bindings.
- **Labels**: `good first issue`, `confirmed bug`; no `help wanted` issues open. Competition is extreme: bot PRs appear within hours; verify no linked PR immediately before starting.

## 9. Staged plan for Aphrody (6 steps)

1. **Weeks 0-2, orientation.** Pick one named human per project. Join Zulip streams (`#t-cargo`, `#clippy`, `#t-rustfmt`, `#miri`, `#t-compiler/rust-analyzer`, `#t-crates-io`). Read the project's contributor doc and LLM policy. Build and run the test suite locally (Linux <build-host>/<test-host>, not the PC). Write nothing upstream with agent help.
2. **Weeks 2-6, first merged PRs.** Start with the six patch candidates in `issues.md` (clippy, cargo, rustfmt, miri). First comment on the issue in your own words, ask for `S-accepted` where cargo requires it, `@rustbot claim`, one small PR each with tests, disclosing any tool use and asking the reviewer first if any code was machine-drafted. Goal: 3-6 merged PRs across 2-3 repos; accept review feedback fast.
3. **Weeks 6-16, consistent contributions.** Keep to 1-2 repos where reviewers already know you (suggested: clippy and cargo, or crates.io). Aim for steady weekly activity, not volume: the compiler team's stated criterion is "regular and consistent participation", Clippy's is "active for some time".
4. **Months 3-6, non-code and review work.** Triage issues (reproduce, bisect, minimize), answer on Zulip, fix docs, attend the cargo or crates.io weekly meeting, and review other contributors' PRs (Clippy community reviews: 2 needed before assignment; crates.io `PR-REVIEW.md`). This is the evidence the docs ask for ("mentor others on simple PRs", "triage and respond", "reviewing contributions").
5. **Months 6-12, reviewer-request criteria.** Only after a sustained record and when a member already knows your work: ask that person (or the lead) whether a reviewer/contributor-tier role fits (`crates-io-reviewers`, `clippy-contributors`, `rustfmt-contributors`, `rust-analyzer-contributors` are the lower tiers; Clippy then onboards via mentored reviews). Expected bar from the docs: independence, familiarity, cordiality, still-active (Clippy drops after 3 inactive months).
6. **After 12 months, team nomination.** Team membership (r+ rights, triagebot rotation, team repo PR) happens only if an existing member nominates you or the leads agree after consulting the team; cargo needs unanimous consent plus a moderation check, Clippy an informal vote, compiler team a no-objection check. Aphrody cannot request, accelerate or substitute this. Bun: keep contributing against `REVIEW.md`; no documented membership route exists.
