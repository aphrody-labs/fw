---
name: crate-unification-2026-11
description: State and conventions after the 2026-11-19 merge of ay crate families into feature-gated crates and the ay-http stack
metadata:
  type: project
---

Crate families are merged into one crate per domain, sub-domains are Cargo features (default on): ay-a2a (pb, client, server, coordinator, grpc), ay-agent (home, proto, tools), ay-model (store, client, antigravity, antigravity_model), ay-gemini (boq, web, runtime), ay-llama (server, in-process), ay-ml (config, runtime), ay-toolcall (repair), ay-cf (assets, edge), ay-media (luma, hls), ay-sh (pty), aphrody-auto in crates/auto-sdk (auth, client, docs, facturx, files, i18n, knowledge, model, sheets, theme, runtime), ay-http (client, server, error).

Kept splits (real boundary): ay-llama-sys (FFI), ay-rag-core (wasm32 core for the Bun package), ay-agent-context (pyo3 cdylib agent_core), auto-ffi (cdylib/staticlib/wasm32), ay-core capi/jni, ay-vault/ay-chromium/ay-ghidra-bun (cdylib).

**Why:** the 82 crates came from merged repos and many were thin splits; the lead asked for end-to-end unification and one HTTP stack with h3.

**How to apply:**
- wasm32 consumers use `default-features = false` plus the named feature (ay-a2a client, ay-gemini web, ay-agent home/proto, ay-model store/antigravity, aphrody-auto i18n/model/theme); check `cargo check --target wasm32-unknown-unknown -p <crate> --no-default-features --features <f>` on the build host.
- Crates that contain `unsafe` keep their own `[lints]` table (workspace forbids unsafe_code): ay-a2a, ay-ml, ay-toolcall, ay-model, ay-gemini, ay-llama, ay-agent, ay-media.
- reqwest http3 needs `--cfg reqwest_unstable` (set in .cargo/config.toml `[build] rustflags`, overridden by RUSTFLAGS or per-target rustflags such as the PC's user config). h3 and h3-quinn are patched to hyperium/h3 master through the aphrody-labs/h3-1 fork (aphrody-labs/h3 is a fork of mullvad/h3, not used).
- reqwest sends h3 only for requests whose version is HTTP/3 on a client built with http3_prior_knowledge; ay-http keeps a TCP client plus lazily built h3 clients and routes by Alt-Svc.
- h3 server: let a request end by itself before stopping reads (STOP_SENDING code 0 makes reqwest fail the exchange, seanmonstar/reqwest#3147); reqwest 0.13.5 never sends 0-RTT (#3148). ay-http replays idempotent requests over TCP after an h3 request-phase failure and marks the origin broken for the Alt-Svc freshness.
- ureq stays in ay-accounts, ay-embed (native download) and ay-core (grammars): synchronous code with no async runtime. hyper-rustls stays in ay-model antigravity local_ls (tonic channel).

## Second wave (2026-11-20): concrete features, 32 crates

- Further merges: ay-design (brand, dioxus, svg, raster, glyph, symbols, softraster, sprites), ay-rag (rag, sql, sqlite, postgres, vec, localstore, codegraph, fsindex, memory, ocr; bins ay-store and ai-os-graph), ay-model (+infer/onnx, embed-*, gateway), ay-agent (+guard, patch, jev*, rollout, subsystem), ay-toolcall (+mcp), ay-gemini (+google), ay-ml (+jobs), ay-build (deps, siso; bins ay-deps, ay-build-model).
- Merged crates have `default = []`, a `full` feature and a dev-dependency on themselves with the test features (ay-model, ay-rag, ay-llama exclude ONNX, candle, fastembed and in-process because they need native runtimes); `cargo hack check --each-feature --no-dev-deps` is a CI job.
- Re-running scratch/merge.ts on an existing target regenerates its manifest: restore the `[lints]` table (own lints for crates with unsafe) and move non-wasm tokio/uuid/ay-http server features back out of plain `[dependencies]` (wasm32 checks catch it).
- Features that were never compiled (in-process of ay-llama, decompile of ay-re, ocr sqlite of ay-rag) had rotted: `--all-features` clippy/check is part of the gate now.
- CI: RUSTFLAGS in a workflow replaces `.cargo/config.toml`, so every workflow that sets RUSTFLAGS must carry `--cfg reqwest_unstable`.
- fr1 disk: every target dir is 10-20 GB; delete the ones you created (target-ay-hack, -after, -chk) when done, the volume filled up once (ENOSPC shows as lld Bus error).
