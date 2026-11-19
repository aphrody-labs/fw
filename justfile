set windows-shell := ["pwsh", "-NoLogo", "-NoProfile", "-Command"]

# Show available recipes
default:
    @just --list

# Zero-dep pure-Rust parallel CI (all lanes: Rust, Native ABI, Wasm32, Ruff, Bun, Cloudflare, Purity, MAP.tsv)
ci:
    cargo xtask ci all

# Fast parallel gate
ci-fast:
    cargo xtask ci fast

# Deterministic byte-exact MAP.tsv generator
map:
    cargo xtask map

# Zero-hardcoded-path & workspace purity gate
purity:
    cargo xtask purity

# Verify crates.io packaging for publishable fw-* crates
package:
    cargo xtask package

# Check local toolchain health
doctor:
    cargo xtask doctor

# Auto-update plugin/skills/upstreams/ from upstreams.toml + live Git HEADs
sync-upstreams:
    cargo xtask sync-upstreams

# Build Fat-LTO fw binary + Wasm32 module
build:
    cargo build --release -p fw-bin --bin fw
    cargo build --release --target wasm32-unknown-unknown -p fw-wasm

# Verify Linux static musl cross-compilation
musl:
    cargo check --target x86_64-unknown-linux-musl -p fw-bin

# Run Hermes/OpenClaw Rust agent smoke checks
claw:
    ./target/release/fw claw status
    ./target/release/fw claw mcp
    ./target/release/fw claw run "Audit Rust CLI MCP skills"

# Launch fullstack control-plane + MCP/GraphQL/WebRTC/WebGPU server
serve addr="127.0.0.1:4380":
    ./target/release/fw serve {{addr}}

# Build Alpine Linux static musl container image
docker tag="fw:latest":
    docker build -t {{tag}} .

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
