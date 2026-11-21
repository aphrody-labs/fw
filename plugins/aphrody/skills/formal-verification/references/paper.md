# Paper facts: AlphaProof Code

Own-words notes on the paper; page numbers are the PDF's printed pages (57 total). The paper text is copyrighted by Google; nothing here is a copy.

## Bibliographic facts

- Title: "AlphaProof Code: Verifying Signal's Security at the Machine-Code Level".
- Authors: Swarat Chaudhuri, Andrew Ferraiuolo, Anton Kovsharov, Tej Chajed, Sam Schlesinger, Karn Seth, Jonathan Katz, Charles Sutton, Pushmeet Kohli (Google DeepMind and Google).
- Dated 2026-11-10. PDF: https://deepmind.google/documents/36/Formal_Verification_of_Signal_4.pdf (57 pages).
- Lean development and web edition: https://google-deepmind.github.io/code-verification (code repo: https://github.com/google-deepmind/code-verification, license field reported as "Other" by the GitHub API on 2026-11-21).
- Target: libsignal v0.105.4 (one-to-one sessions only) with spqr v1.6.0, wrapped in a small CLI harness `libsignal-cli`, compiled by stock rustc 1.98.1 to a static AArch64 Linux (musl) executable.
- Prior context: the work answers the "Signal Shot" challenge announced April 2026 (ref [23]).

## Headline numbers (PDF edition)

| Fact | Value | Where |
|---|---|---|
| Machine-checked proof code produced by agents | about 6.5 million Lean lines | abstract, p1; section 7, p25 |
| Whole Lean repo | nearly 7 million lines incl. comments and blanks | section 7, p25 |
| Trusted base to read | about 18,000 lines of statements/definitions (about 28,000 with docs) plus about 8,000 lines of AArch64 semantics, about 26,000 total | section 1, p3; section 7, p25 |
| Proof-to-trusted ratio | about 250 to 1 (6.5M / 26k, computed here) | derived |
| Proof breakdown | 5.3M about the binary (shared), about 860k computational, about 350k symbolic, 140k reintroduced-bug disproofs, 70k allocator | section 7, p25 |
| Verified code size | about 560 KB, about 140,000 AArch64 instructions | section 7, p25 |
| Spec directories | about 54,000 lines, each file marked trusted / trusted in part / checked by Lean | section 7, p25 |
| Lean image of compiled code | about 95,000 lines, tool-extracted, checked against the ELF bytes, only symbol names trusted | section 7, p25 |
| AArch64 semantics | 467 instructions, 31 system calls, 7,532 Lean lines | section 2.2, p5 |
| Hooked routines | 39 further contract-replaced routines (after the ones listed first) plus 18 panic entry points ending in SIGABRT | section 4, p11 |
| Symbolic theorems | 10 queries (PQXDH Theorem 1 row counts five) | Table 2, p16 |
| Computational theorem | one concrete-bound theorem (Theorem 2), 4 lemmas | section 5.2.3, p21-22 |
| Protocols | PQXDH handshake and Triple Ratchet (Double Ratchet + SPQR) | section 3, p6-7 |
| Properties | forward secrecy, post-compromise security, authentication, KCI resistance, handshake secrecy, session independence, harvest-now-decrypt-later resistance, message confidentiality/authenticity | Table 1, Table 2 |
| Team and time | 5 humans, 4 weeks; campaigns of 40 to 80 agents on a single machine for 24 to 72 hours | section 7, p25; section 2.3, p6 |
| Human-written proof code | none | section 7, p26 |
| Models named | Gemini 4 Carbon (all proofs, spec drafting) and Claude Opus 5.5 (spec drafting); reviewer from a different family than the writer | section 7, p25-26 |
| Compute | no GPU-hours, tokens or cost given; only "single machine, 24-72 h" and clean proof builds of several hours | sections 2.3, 7 |
| Differential test of the Lean semantics | 15.6 million instructions across 10 invocations matched the real binary | section 4, p10 |
| Memory bound proved | each run gets 1 GiB beyond segments and 8 MiB stack; no run runs out; a session record stays below 1.2 MB | section 4.1, p12 |
| Spec-vs-literature catalog | over 200 deviation entries (computational), about 35 (symbolic); 15 gaps found in a published Double Ratchet analysis | section 7, p26 |
| Historical bugs | 2 fixed libsignal bugs reintroduced; Lean proves binaries violate the symbolic theorem | section 6, p24-25 |
| Earlier cost anchors | seL4 about 20 person-years, CompCert about 6 | section 1, p1 |

## Version drift: PDF vs live web page

The web edition fetched 2026-11-21 is a later revision. Its numbers differ: about 6.3M proof lines, about 27,000 trusted lines, about 19,000 spec lines (26,000 with docs), 7,801 semantics lines, 32 system calls, binary about 744 KB / 186,000 instructions, 4.3M binary-proof lines, about 1.3M computational, 360k symbolic, 220k disproofs and synthetic mutants, spec directories about 42,000 lines, and "no generated representation of the compiled code" in the TCB (only 23 symbol-table addresses). Cite the PDF numbers with the PDF page, and re-read the live page before quoting a number as current.

## Section map (PDF pages)

| Section | Pages | Content |
|---|---|---|
| Abstract, 1 Introduction, Figure 1 | 1-2 | claim, cost anchors, workflow diagram |
| 2 AlphaProof Code | 3-6 | 2.1 spec writing (p3), 2.2 machine-code semantics (p4), 2.3 proof synthesis (p5), 2.4 gatekeeping (p6) |
| 3 Background on Signal | 6-8 | handshake, ratchet, Table 1 result summary (p8) |
| 4 Setup for Verification | 9-12 | CLI harness, process semantics, hooks, 4.1 binary-level guarantees (p12) |
| 5 Verifying Signal | 12-24 | 5.1 symbolic (p13-18, Table 2 p16), 5.2 computational (p19-23), 5.3 trusted computing base (p24) |
| 6 Finding Historical Bugs | 24-25 | mutation by real past bugs |
| 7 In Practice | 25-27 | scale, spec writing, proof generation, anti-weakening, human role |
| 8 Related Work | 27-29 | protocol analyses, verified implementations, binary-level, AI for verification |
| 9 Discussion | 30-31 | extensions, spec-is-the-hard-part, future coupling with coding |
| References | 31-38 | comparator is [72], DyLean [99], weakening anecdotes [78, 107] |
| Appendix A Signal in detail | 39-40 | key schedule, Figure 8 |
| Appendix B Symbolic proofs | 40-45 | model transitions (Table 3 p42), events (Table 4 p43), collision clause, scope, Table 5 (p45) |
| Appendix C Computational proofs | 45-57 | game C.1 (p46), reduction costs C.2 (p47), lemmas C.3 (p47-50), full theorem C.4 (p50), assumptions C.5 (p52), restrictions C.6 (p52), primitive premises C.7-C.8 (p53-54), Sess terms C.9 (p55), SHA-512 reductions C.10 (p56) |

## Link check (read 2026-11-21)

- https://google-deepmind.github.io/code-verification : HTTP 200, text/html, about 2.4 MB Verso-generated book of the paper; links its source to the GitHub repo.
- https://github.com/google-deepmind/code-verification : HTTP 200; API: description "Verifying Signal's security at the machine-code level", last push 2026-11-20, default branch main.
- https://github.com/leanprover/comparator : HTTP 200 (the tool the gatekeeper calls, ref [72]).
