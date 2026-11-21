# Results, limits, threats to validity

Source: Table 1 (p8), sections 4.1, 5.1.6, 5.2.5, 5.3, 6, 8, 9, Appendices B.7, C.5-C.6. Own words.

## Proven (about the compiled libsignal-cli, init/send/recv)

- Binary-level functional refinement: every run matches the Lean model, including rejects (exit 1, no file written) and panics (SIGABRT); no run faults, diverges or exhausts memory (section 4.1, Lemma 4). CLI checks proved too: stdin over 2^20 bytes exits 2; send rejects plaintext above 10^6 bytes (exponents confirmed on the web edition, the PDF text layer drops superscripts).
- Symbolic model: ten theorems, each holding on every collision-free execution: forward secrecy and post-compromise security of message keys, initiator and responder secrecy, authentication, peer authentication, KCI resistance, handshake forward secrecy, harvest-now-decrypt-later resistance, session independence. Includes a future quantum attacker breaking DH after the handshake (handshake secrecy).
- Computational model: confidentiality and authenticity of messages with forward secrecy and post-compromise security for PQXDH composed with the Triple Ratchet, as a concrete advantage bound with cost-bounded reductions to Gap-DH, XEdDSA unforgeability, Kyber1024 collision resistance, HKDF/HMAC PRF-PRG and AES-CBC with HMAC security; classical adversaries only (section 5.2.3).
- Spec strength demonstrated: two historical bugs reintroduced, symbolic theorem refuted for each binary (section 6).
- The authors claim no earlier work tied protocol-level security theorems to the unmodified, compiled code of a widely used crypto protocol library (section 1).

## Not proven or not covered

Cryptography
- Symbolic guarantees assume ideal primitives; byte collisions are excused, their probability is bounded outside Lean; Kyber correctness is not assumed, so a decapsulation mismatch gets no guarantee.
- Computational bound is classical only; the handshake HKDF random-oracle step in clause 2 is a heuristic; ML-KEM-768 decryption failures are charged as an explicit term.
- Two non-standard joint-security assumptions (one Curve25519 key used for both DH and XEdDSA); Lean-proved reductions to standard Gap-DH exist but sit outside the main theorem and lose tightness (Appendix C.5, C.10).
- Statistical terms grow with messages and epochs: with an 8-byte MAC the bound becomes vacuous for a single session of about 25,000 messages when nearly every message starts a new epoch, and for many long sessions with many epochs (section 5.2.5, p23).
- SPQR's post-quantum healing is not proved: ratchet theorems let the attacker choose SPQR keys, so they show the Triple Ratchet is no weaker than the Double Ratchet only (section 5.1.6).

Scenario and scope
- One-to-one sessions only; no groups, sealed sender or server-side protocols; no multi-device state; fixed message lengths; one session per promotion.
- Fixed harness: two honest parties on one device each, identity keys pinned (trust-on-first-use path not exercised), one signed prekey, one last-resort Kyber prekey, at most one one-time prekey; one-time Kyber prekeys not covered; coins files of exact sizes; bounded inputs (Appendix B.7, C.6).
- Replay-based forward-secrecy loss described in prior work is not ruled out by the model-level statements; the binary does deduplicate, but the model does not require it.
- Key generation (`keygen`) is not verified; its output is a provisioned input.
- The computational game forbids the adversary from injecting its own PQXDH initial message to the responder of a later-promoted session, and corruption is refused at some points (until epoch 10 for the initiator).
- Results concern this build of libsignal-cli on a fresh process with the stated memory, not Signal's apps; carrying over to Android/iOS requires trusting that they preserve protocol behavior as exercised by the CLI (section 4).
- Semantics scope: single-threaded, static, no signals/vDSO/dynamic linking/outside files.

## Trusted base weaknesses (section 5.3, 4)

Hooked routine contracts (crypto primitives, five libc routines, two output functions, a few Rust library routines) are not proved against real code; they were only compared by running scenarios in Lean vs the real binary. Symbol-name extraction tooling and reproducible build scripts are trusted. The Lean kernel is trusted. Soundness of the AArch64 semantics cannot be proved (hardware and kernel are not formal objects); it is argued by differential testing against Unicorn, Sail and qemu-aarch64.

## Threats to validity worth remembering

- Spec errors: caught by review and testing, not guaranteed; human review will not find all bugs in a large artifact (section 2.1).
- Model fidelity: literature models had real errors; the repaired Lean statements may still differ from intent; the deviation catalog documents known gaps.
- Some environment assumptions are argued informally; some game restrictions (e.g. that the promotion oracle's bundle check loses no generality) are argued, not mechanized (Appendix C.6).
- Compiler flags and linking differ across deployments; the proof is about one binary (SHA-256 pinned), and a rebuild needs a new refinement proof.
- Single case study, small expert team, long time to author specs; no controlled cost comparison; no compute cost reported.
- Authors involved in the Signal feedback loop; Signal engineers gave feedback on specifications (acknowledgments).
- The live web edition changed some numbers relative to the PDF (see paper.md).

## Future work named by the authors (section 9, p30)

Strengthen theorems (post-quantum and classical healing, relax single-session and fixed-length restrictions); shrink the trusted base (verify the output functions and library routines so trust falls to system calls; connect crypto contracts to verified implementations such as HACL* or libcrux); broaden scope (group messaging, sealed sender, server); source-level verification to cut proof cost (at the price of trusting compiler and language semantics); tight coupling of AI code generation with spec and verification so every change is verified before merge, proofs repaired by prover agents when code changes; verify new libsignal releases before they ship; open review of the specs by the community.
