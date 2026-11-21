set dotenv-load

just := just_executable()

# List the SDK recipes (workspace-wide recipes live in workspace.just, imported by the root justfile)
default:
    @{{ just }} --list

# Run every conformance, path, privacy, ABI, submodule, TypeScript, and Bun test gate
check:
    bun run check

# Reconcile `.gitmodules` and `160000` gitlinks from `submodules.toml`
submodules:
    bun tools/submodules.ts

# Shallow-checkout all or selected fork submodules on `<build-host>`
submodules-init *paths:
    bun tools/submodules.ts --init {{ paths }}

# Audit cross-repository crate, lockfile, and SQL duplication into `.private/dedup.md`
dedup:
    bun tools/dedup.ts

# Audit duplicate vendor/third_party trees across workspace checkouts with eza + rg + fuzzy matching
vendor-dedup *args:
    bun tools/vendor-dedup.ts {{ args }}

# Audit code clones, code-graph overlaps, and copied upstream code across workspace checkouts
code-dedup *args:
    bun tools/code-dedup.ts {{ args }}

# Generate and verify the unified stage-aware deploy plan across `ay.toml` products and Gatekeepers
deploy stage="prod":
    bun tools/deploy.ts --stage {{ stage }}

# Import and deduplicate local Claude Code sessions & memories into `.private/claude-sessions/`
sessions:
    bun tools/claude-sessions.ts

# Sync workspace to `<build-host>` (`ay.local.toml` [hosts].build)
sync-host:
    bun tools/sync-host.ts

# Regenerate `.private/plan.md` from verified claims
plan:
    bun tools/plan.ts

# Clean ephemeral Rust/Wasm build outputs
clean:
    bun -e "import { rmSync } from 'node:fs'; rmSync('target', { recursive: true, force: true });"

# Audit the Cloudflare manifest, the R2 catalog and the Worker configs, then dry-run bundle the reference Worker
cf-audit:
    bun tools/cf-audit.ts
    bun x wrangler deploy --dry-run --outdir target/wrangler-dry --config apps/cf-gateway/wrangler.jsonc
    bun x wrangler deploy --dry-run --outdir target/wrangler-dry-release --config apps/tools-release/wrangler.jsonc

# Workers Paid + R2 Paid FinOps live cost and savings report
cf-finops:
    bun tools/cf-finops.ts

# Reconcile packages/cf-control-plane/cf.toml against the live Cloudflare API (dry-run by default)
cf-reconcile *args:
    bun tools/cf-reconcile.ts {{ args }}

# Verify cloudflared tunnel ingress parity against live Cloudflare DNS
cf-tunnels:
    bun tools/cf-tunnels.ts

# Audit the secret bindings of every Cloudflare Worker (never prints a value)
cf-secrets *args:
    bun tools/cf-secrets.ts {{ args }}

# Generate the Pingora, Nginx, nftables, Mullvad and cloudflared configs of the edge-proxy and origin hosts
cf-cluster-proxy:
    bun tools/cf-cluster-proxy.ts

# Validate the reusable R2 media and 3D asset catalog and its SPDX licenses
cf-r2-catalog *args:
    bun tools/cf-r2-catalog.ts {{ args }}

# Scan every live HTTP endpoint of the Cloudflare zones and auto-fix resolvable 404s
cf-http-scan *args:
    bun tools/cf-http-scan.ts {{ args }}

# Benchmark xh vs curl vs wget2 vs bun fetch against the live Cloudflare edge
cf-bench-http url="https://1.1.1.1/cdn-cgi/trace" runs="5":
    bun tools/cf-bench-http.ts {{ url }} {{ runs }}

# Regenerate the agent-skills discovery index served by apps/cf-gateway
cf-skills-index:
    bun tools/cf-skills-index.ts

# Python workspace: Ruff lint and format checks, then pytest, through uv
py-check:
    uv run ruff check python
    uv run ruff format --check python
    uv run pytest

# C++23 Abseil and GoogleTest suite with Bazel (cpp/)
cpp-check:
    bazelisk test //cpp/...
