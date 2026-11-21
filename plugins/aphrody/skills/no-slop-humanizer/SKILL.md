---
name: no-slop-humanizer
description: "Detect and remove slop before it is produced, in prose, code, design and reasoning. Use before any reply, report, commit, README, UI copy, code change or plan"
---
> the checks are concrete patterns seen in real agent sessions, not style advice.


# No slop: writing, code, design, thinking

Run the four checks before producing anything, not after. Each item is a pattern that has actually shipped and had to be undone.

## 1. Thinking

- **Look before building.** Search what exists first: `fw --help`, `fw graph query "<topic>"`, `cargo metadata --no-deps`, grep the crates, list the Cloudflare Workers. Two implementations of one capability (two code graphs, a second Worker, a second Docker skill) is the most expensive slop.
- **Read the API, then write the call.** Query `fw docs query <lib> "<topic>"` or the crate source before using a function. Guessing (`Redirect::see_other`, a made-up `thiserror_lite::Error` derive) costs a build cycle each time.
- **Finish one thing before fanning out.** Launching agents, registering repos or adding catalog entries is not progress until something is wired, tested and verified live.
- **An agent's report is a lead, not a fact.** Check the commit, the test output or the live response before repeating it.

## 2. Code

- **Tests that cannot fail are slop.** `assert_eq!(s, X.min(Y).max(s))` always passes. Every assertion must fail on a plausible bug.
- **No invented items:** modules, derives, methods, flags, features or config keys that were not read in the source or docs this session.
- **Name collisions and shadowing:** a second `fn quota` on the same type, a helper that hides a public method.
- **State transitions:** for every flag a function sets, check every path that should clear it (trash/restore left `trashed_by_ancestor` set).
- **String literals that carry syntax:** raw strings containing `"#` need `r##`.
- **Shell on Windows:** strip `\r`, never feed backslash paths to `sed`/`xargs`, never `pkill -f` a pattern that matches the current command line.
- **Git on a shared tree:** check `git diff --cached --name-only` before every commit (another agent may have staged files); never replace a whole `Cargo.lock` from another checkout without diffing package names first.

## 3. Writing (replies, reports, commits, docs)

- **Write in the reader's language.** A French message gets a French reply.
- **Report shape:** what changed, the evidence, what is next; about ten lines. No tables or bold-label lists for a three-fact answer, no "Status:" preamble, no recap of earlier turns, no restating the plan.
- **No questions when the decision has a sensible default** and the user said to act; decide and say what was decided.
- **Claims match the state.** "Tracked" is not "integrated"; "uploaded" is not "live"; a number comes from a command run in this turn.
- **Prose tells:** binary contrasts ("not just X, but Y"), throat-clearing ("It is worth noting"), hype ("seamless", "blazingly fast") without a measured number, em-dash cadence, triads of synonyms, dramatic one-line closers.

## 4. Design

- **Measure first, then decide.** Record computed values from the reference (sizes, radii, colours) and write down which ones you changed and why (for example 32px nav items raised to 48px targets).
- **No placeholder glyphs or labels as content:** "—" in data cells, "Error" as a page title, an icon plus a generic sentence as every empty state. Say what is missing and what to do.
- **Tokens, never literals:** colours, radii and fonts come from the generated tokens (`fw-m3`), not hex typed into CSS.
- **Check the result rendered** at the target viewport sizes before calling it done.

## Rewrite pass

Replace abstractions with the concrete verb and the measured fact, cut what the reader already knows, keep technical terms exact, and never invent a metric.
