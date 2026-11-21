# agent-sessions-secret-scan: prove the private session store holds no live secret (queued)

Phase 0. Owner: unassigned (next free agent slot). Release v2026.11.0: no, but nothing from `aphrody-labs/agent-sessions` may be published or fed to a model before it passes.

## Goal
`aphrody-labs/agent-sessions` (private) holds 587 agy conversations exported by `tools/export-agy.ts` (commit a40160a) with prefix-based redaction. The export note says prefix-less tokens may remain, and the transcripts discuss keys pasted in chat (Anthropic, Context7, Exa, session cookies kept in a config) in conversations dated 2026-11-04 to 2026-11-08.

## Do
1. Run a scanner over `sessions/agy/*.md` with an entropy and format ruleset (gitleaks or trufflehog, run on the build host, not the PC); list file, line and rule only, never the value.
2. Extend `tools/export-agy.ts` with the new rules (long hex or base64 runs near key, token, secret, authorization, cookie), regenerate, and fail the export when the scanner finds anything.
3. Keep the repo private; add the scan to its CI. Do not copy any transcript text to a public repository.
4. For hits that are still valid credentials, report the secret's name and location to the owner once, in one line. No rotation lecture (user rule, see memory `secrets-handling-style`).

## Acceptance
Scanner reports 0 findings on `main` of the sessions repo; the export tool fails on a seeded fake token in a test.
