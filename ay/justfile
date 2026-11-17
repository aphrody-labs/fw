set dotenv-load

just := just_executable()

# List available Pop!_OS / COSMIC-style SDK & monorepo recipes
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
