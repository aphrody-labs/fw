set windows-shell := ["pwsh", "-NoLogo", "-NoProfile", "-Command"]

# Show available recipes
default:
    @just --list

# Zero-warning Clippy across all 13 workspace crates
clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# Run Rust, Bun, and Python checks
test:
    cargo test --workspace
    bun test
    uvx ruff check .

# Build Fat-LTO native binary + Wasm32 module
build:
    cargo build --release -p fw-bin
    cargo build --release --target wasm32-unknown-unknown -p fw-wasm

# Verify Linux static musl cross-compilation
musl:
    cargo check --target x86_64-unknown-linux-musl -p fw-bin

# Run Hermes/OpenClaw Rust agent smoke checks
claw:
    ./target/release/fairwind-native claw status
    ./target/release/fairwind-native claw mcp
    ./target/release/fairwind-native claw run "Audit Rust CLI MCP skills"

# Launch fullstack control-plane + MCP/GraphQL/WebRTC/WebGPU server
serve addr="127.0.0.1:4380":
    ./target/release/fairwind-native serve {{addr}}

# Build Alpine Linux static musl container image
docker tag="fairwind:latest":
    docker build -t {{tag}} .

# Sync Fairwind plugin and validate ~/.gemini
plugin:
    ./dist/fairwind plugin sync --install
    bun ~/.gemini/scripts/check.ts

# Cloudflare Workers types, dry-run, dev, and deploy (Wrangler)
cf-types:
    bunx wrangler types -c cloudflare/wrangler.jsonc cloudflare/worker-configuration.d.ts

cf-dry-run:
    bunx wrangler deploy --dry-run -c cloudflare/wrangler.jsonc

cf-dev port="8787":
    bunx wrangler dev -c cloudflare/wrangler.jsonc --port {{port}} --local

cf-deploy:
    bunx wrangler deploy -c cloudflare/wrangler.jsonc

# Reproducible Nix full-stack Rust userland across all 3 VPS (fr1, fr2, prd)
nix-bootstrap-vps host="fr1":
    ssh {{host}} "bash -s -- {{host}}" < nix/bootstrap-vps.sh

# Apply tuned Windows .wslconfig + .wslgconfig for Fairwind-WSL (WSL 3.0.5 + RTX 4070)
wsl-apply-configs:
    pwsh -NoProfile -File os/fairwind-wsl/install-fairwind-wsl.ps1 -ApplyHostConfigs

# Regenerate byte-exact MAP.tsv for token-minimal agent navigation
map:
    bun src/repo_map.ts

# Auto-generate & update plugin/skills/upstreams/ from upstreams.toml + live Git HEADs
sync-upstreams:
    bun src/upstream_skills.ts

# Full CI verification pipeline
ci: map clippy test build musl claw cf-dry-run
