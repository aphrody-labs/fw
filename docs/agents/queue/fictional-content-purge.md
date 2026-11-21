# Lot: nothing fictional, placeholder or hallucinated, anywhere

Order of the owner, 2026-11-21: fix everything that is fictional, a placeholder or hallucinated, everywhere, and let nothing pass.

## Known cases, found this session

- **Fake cluster topology:**
  - `fw cluster/topology.toml` is "documentation only". Its gcloud L4/T4 GPU hosts, TPU burst node and `10.66.0.x` RPC addresses do not exist.
  - `crates/fw-cluster` `default_hybrid_cluster` hardcodes the same fake layout, and `fw status` reports it as `gpuHosts: 4`.
- **Invented names:**
  - prd's systemd slices are called "Fairwind" (`fairwind-build-worker.slice`). Check where the name comes from, and rename to the cluster names.
  - `fw-bin` `m3_agent_tui.rs` writes a fake superblock string.
- **Unchecked claims and versions** to verify: ggml-rpc compatibility claims, "Gemini 4 Neon", and `gemini-decision-1` / `v1beta/decisions` (the agy-retirement agent found Google pages for them; re-verify live).

## Method

1. **Scan every tracked file** of fw, aphrody, ay, nie, shenron and the bun fork. Look for:
   - placeholder markers: TODO, FIXME, XXX, "placeholder", lorem ipsum, example.com outside docs examples, `unimplemented!`, `todo!`, stub functions that return constants, "mock" outside tests;
   - hardcoded fake data: IPs, hosts, GPU models, benchmark numbers without a command, sizes, dates;
   - claims in docs, skills and READMEs about versions, features or measurements.
2. **Check each hit against a live source:** the code that runs, `gh api`, registries, the vendor's docs, a measured command.
3. **Fix each hit one of three ways:**
   - make it real: implement it, read it live, or derive it from config;
   - delete it;
   - mark it as a clearly labelled example in test fixtures only.
4. **Guard against regressions.** Add a CI check with an allowlist for real test fixtures that fails on the markers and on known-fake identifiers.

## Output

`docs/research/fictional-purge-2026-11.md` holds one row per finding: file:line, what it claimed, what is true, fix commit.
