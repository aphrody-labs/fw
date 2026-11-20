---
name: ai-os-repo
description: aphrody-dev/ai-os is archived (2026-11-17); the agent kernel and AI surface live in fw (packages/ai-os as @aphrody/fw-ai-os, crates fw-*)
metadata:
  type: project
---

`aphrody-dev/ai-os` is archived and listed in `ay.toml` [retired]. Its code lives in `aphrody-labs/fw`: the Bun agent harness in
`packages/ai-os` (`@aphrody/fw-ai-os`), the Rust agent, RAG, model, llama and tool-call crates as `crates/fw-*`. There is no
`$WORKSPACE_ROOT/ai-os` checkout any more.

**Why:** fw is the toolchain and agent runtime, the base of the stack; a separate AI repository duplicated it.

**How to apply:** change AI/agent code in fw; read `ay.toml` [repos] for the live repositories before naming one.
