set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

exe := if os() == "windows" { ".exe" } else { "" }
fw := "./target/release/fw" + exe

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

# Auto-update plugin/skills/upstreams/ from upstreams.toml + live Git HEADs (parallel)
sync-upstreams:
    cargo xtask sync-upstreams

# Refresh the research snapshot of watched upstreams (docs/research/upstream-watch.tsv)
sync-research:
    bun tools/upstream_watch.ts

# Parallel full monorepo upgrade (sync-upstreams + parallel CI + purity + MAP.tsv)
upgrade-all:
    cargo xtask upgrade

# End-to-end parallel Feature-to-Deploy pipeline (CI fast -> Fat-LTO fw + Wasm32 + Bun AOT --trim + CF dry-run)
ship:
    cargo xtask ship

# Run a single .rs file with inline Cargo frontmatter using Cranelift
rs-run file:
    cargo -Zscript -Zcodegen-backend {{file}}

# Build Fat-LTO fw binary + Wasm32 module
build:
    cargo build --release -p fw-bin --bin fw
    cargo build --release --target wasm32-unknown-unknown -p fw-wasm

# Verify Linux static musl cross-compilation
musl:
    cargo check --target x86_64-unknown-linux-musl -p fw-bin

# Run Hermes/OpenClaw Rust agent smoke checks
claw:
    {{fw}} claw status
    {{fw}} claw mcp
    {{fw}} claw run "Audit Rust CLI MCP skills"

# Launch fullstack control-plane + MCP/GraphQL/WebRTC/WebGPU server
serve addr="127.0.0.1:4380":
    {{fw}} serve {{addr}}

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

# Apply tuned Windows .wslconfig + .wslgconfig for Fw-WSL (WSL 3.0.5 + RTX 4070)
wsl-apply-configs:
    powershell.exe -NoProfile -File os/fw-wsl/install-fw-wsl.ps1 -ApplyHostConfigs

# Refresh live direct Rust competitor benchmark (TSV + Markdown)
competitors:
    bun tools/competitor_audit.ts

# Refresh live Rust upstream & toolchain brain (Matrix + Agent Cards + Vectors + Skill)
rust-brain:
    bun tools/rust_brain.ts

# Universal benchmark sandbox: silicon (RAM/SWAR/MatMul/System-1/Tokens) & taxonomy
bench-silicon:
    {{fw}} bench silicon --mb 16

bench-matrix:
    {{fw}} bench matrix

# CLI migration from OpenClaw or Hermes Agent into fw
migrate-openclaw *args:
    bun src/migrate_openclaw_to_fw.ts {{args}}

migrate-hermes *args:
    uv run python core/py/fw_py_ai/migrate_hermes_to_fw.py {{args}}

migrate-vllm *args:
    uv run python core/py/fw_py_ai/migrate_vllm_llmd_to_fw.py vllm {{args}}

migrate-llmd *args:
    uv run python core/py/fw_py_ai/migrate_vllm_llmd_to_fw.py llmd {{args}}

# Prove fw ls/table/find/grep against eza, nu, fd, rg via hyperfine
bench-cli:
    hyperfine --warmup 5 --runs 25 -N "{{fw}} ls ." "eza ."
    hyperfine --warmup 3 --runs 15 -N "{{fw}} table ." "nu -n --no-std-lib -c 'ls | select name type size'"
    hyperfine --warmup 5 --runs 25 -N "{{fw}} find .rs crates" "fd -e rs . crates"
    hyperfine --warmup 5 --runs 25 -N "{{fw}} grep forbid crates" "rg -F -n forbid crates"
