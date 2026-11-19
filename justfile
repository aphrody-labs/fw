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

# Full CI verification pipeline
ci: clippy test build musl claw
