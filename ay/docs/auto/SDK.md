# Library, Crate & Package SDKs

## Rust Crates (`crates/`)

- `aphrody-auto/auth` (`crates/auto-sdk`): Argon2id & Better Auth Scrypt hashing, ChaCha20-Poly1305 vault sealing, Ed25519 JWT/JWKS, TOTP RFC 6238, Passkey P-256 verification, RBAC permissions, SQLite store, Axum router. Disable default features for `wasm32-unknown-unknown`.
- `aphrody-auto` (`crates/auto-sdk`, `[lib] aphrody_auto`): Single-import SDK uniting `aphrody_auto::auth`, `aphrody_auto::client`, `aphrody_auto::docs`, `aphrody_auto::facturx`, `aphrody_auto::files`, `aphrody_auto::i18n`, `aphrody_auto::knowledge`, `aphrody_auto::model`, `aphrody_auto::sheets`, `aphrody_auto::theme`, plus `balancer` (L7 proxy/failover), `config` (`auto.toml`), `mcp`, `updater`, and `server_rt`.
- `auto-ffi` (`crates/auto-ffi`): C shared/static ABI (`cdylib` + `staticlib` + `crates/auto-ffi/include/auto.h`) and WebAssembly module (`wasm32-unknown-unknown`).

## TypeScript & Bun Packages

```ts
import { createAutoClient, asAgentTools } from "@aphrody/auto/client";
import { createAutoApp, createSelfHostedServer } from "@aphrody/auto/server";
import { initNative, facturx, authHashPassword, authSignJwt } from "@aphrody/auto/native";

const client = createAutoClient({
  upstreams: ["https://auto.aphrody.com", "https://edge-2.example.com"],
  strategy: "consistent_hash",
  token: process.env.AUTO_TOKEN,
  companyId: process.env.AUTO_COMPANY_ID,
});
const tools = asAgentTools(client);
```
