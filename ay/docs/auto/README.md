# Auto

Auto is the workflow automation, invoicing and office suite of Aphrody, merged into [`aphrody-labs/ay`](https://github.com/aphrody-labs/ay) (the former `aphrody-labs/auto` repository is archived). Its Rust crates are `crates/auto-*`, its apps `apps/auto-*`, its TypeScript packages `packages/auto` and `packages/auto-*`; this folder holds its documentation (API, architecture, compliance, deployment, SDK, design) and licence.

Unified billing, Factur-X compliance, HR, workflow automation, and workspace software delivered as a single LTO Rust binary (`auto`), embeddable libraries (`aphrody_auto`, `aphrody_auto::auth`, `auto_ffi`, `@aphrody/auto`), a Cloudflare Workers edge bundle, a Web UI, and a GPUI desktop app.

## Architecture

- **Rust core and single binary (`crates/`)**
  - `aphrody-auto/auth`: Better Auth engine in Rust (Argon2id + Scrypt compatibility, ChaCha20-Poly1305 encrypted vaults, Ed25519 JWKS/JWT, TOTP RFC 6238, ES256 P-256 Passkeys, RBAC, OAuth2 PKCE, SQLite store, Axum router).
  - `aphrody-auto/model`, `aphrody-auto/i18n`, `aphrody-auto/docs`, `aphrody-auto/facturx`, `aphrody-auto/sheets`, `aphrody-auto/files`, `aphrody-auto/knowledge`, `aphrody-auto/client`, `aphrody-auto/theme`: native domain, PDF/A-3b Factur-X, DOCX/XLSX/PPTX, RAG chunking, HLC sync, and Material 3 design tokens.
  - `auto-ffi`: C ABI (`cdylib` + `staticlib` + `include/auto.h`) and `wasm32-unknown-unknown` WASM module for Cloudflare Workers, browsers, and Bun.
  - `auto-sdk` (`aphrody-auto`): embeddable Rust SDK library and unified `auto` executable (`serve`, `lb`, `mcp`, `auth`, `pdf`, `facturx`, `docx`, `chunk`, `desktop`).
- **TypeScript packages (`packages/`)**
  - `@aphrody/auto` (SDK entry point), `@aphrody/auto-i18n`, `@aphrody/auto-platform`, `@aphrody/auto-domain`, `@aphrody/auto-native`, `@aphrody/auto-store`, `@aphrody/auto-services`, `@aphrody/auto-client`.
- **Applications (`apps/`)**
  - `apps/auto-server`: single server targeting Cloudflare Workers (`DB` D1, `CompanyDO` SQLite Durable Objects, `AutomationWorkflow`, R2 prefix storage, `ASSETS`) and Bun self-hosted SQLite/WebSocket mode (`AUTO_MODE=serverless|self-hosted`); holds `auto.toml` and `wrangler.jsonc`.
  - `apps/auto-ui`: NSH 0.6.0 web interface built to `apps/auto-ui/dist/web/`.
  - `apps/auto-desktop`: standalone GPUI desktop workspace (`gpui-kit 0.10.0` + `gpui-fast`), its own Cargo workspace.
  - `apps/auto-cli`: `auto-cli` tool, MCP stdio bridge and the Claude plugin of Auto.
  - `apps/auto-extension`: Chrome Manifest V3 invoice clipper.

## Quick start

```sh
bun install
bun run check                      # repository gates, including the Auto tests

bun run auto:build-native          # Wasm FFI module (nightly toolchain, run on the build host)
bun run auto:build-aot             # Bun AOT binary
bun run auto:server --port 3000    # self-hosted server
bun tools/auto-cargo.ts run --features runtime --bin auto -- lb --listen 127.0.0.1:8080 --upstreams http://127.0.0.1:3000
```

Configuration is declarative via `apps/auto-server/auto.toml` and environment variables (`AUTO_MODE`, `AUTO_HOST`, `AUTO_PORT`, `AUTO_DB_PATH`, `AUTO_UPSTREAMS`, `AUTO_URL_APP`). Deployment scripts are `tools/auto-deploy-cf.ts` (Cloudflare) and `tools/auto-deploy-fleet.ts` (self-hosted hosts); CI is `.github/workflows/auto.yml`.
