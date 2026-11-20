# Direct Rust Competitors Benchmark: `fw` vs Peers (2026-11-19)

Generated reproducibly by `bun tools/competitor_audit.ts` (live GitHub API + local `Cargo.toml` parsed via `Bun.TOML.parse`, timestamp `2026-11-19T14:45:52.221Z`). Machine-readable matrix: [`rust-competitors-matrix.tsv`](./rust-competitors-matrix.tsv).

## 1. Synthèse quantitative

| Projet | Stars | Dernier push | Licence | Workspace Members | Crates | Root Deps (externes) | Sécurité mémoire |
|---|---:|---|---|---:|---:|---:|---|
| **fw** (`aphrody-labs/fw`) | private | `2026-11-19` | MIT | 14 | 13 | **5** (16 total) | 12/13 #![forbid(unsafe_code)] |
| **ZeroClaw** (`zeroclaw-labs/zeroclaw`) | 33,495 | `2026-11-19` | Apache-2.0 | 44 | 24 | **67** (75 total) | partial (FFI/sys crates) |
| **OpenFang** (`RightNow-AI/openfang`) | 18,271 | `2026-07-02` | Apache-2.0 | 14 | 13 | **63** (63 total) | partial (runtime crates) |
| **IronClaw** (`nearai/ironclaw`) | 12,718 | `2026-11-09` | Apache-2.0 | 68 | 10 | **35** (89 total) | partial (Wasmtime/crypto) |
| **Zed** (`zed-industries/zed`) | 95,319 | `2026-11-19` | GPL-3.0 / Apache-2.0 | 263 | 255 | **303** (545 total) | unsafe in GPU/platform FFI |
| **Asterinas** (`asterinas/asterinas`) | 5,116 | `2026-11-19` | MPL-2.0 | 57 | 57 | **52** (105 total) | OSTD isolates unsafe; services safe |

**Lecture directe :** `fw` couvre OS, Wasm, WebGPU, cluster et agent MCP/Claw dans **14 crates** avec **5 dépendances externes** (`serde`, `serde_json`, `sha2`, `tokio`, `flate2`) et **12/13 #![forbid(unsafe_code)]**. Les cinq pairs analysés déclarent entre **35** et **303** dépendances externes directes dans leur `Cargo.toml` racine.

## 2. Arborescence

| Projet | Dossiers racine | Organisation top-level |
|---|---:|---|
| **fw** | 16 | `cloudflare`, `cluster`, `configs`, `core`, `crates`, `docs`, `native`, `nix`, `os`, `plugin`, ... |
| **ZeroClaw** | 21 | `apps`, `benches`, `crates`, `demo`, `deploy-k8s`, `dev`, `dist`, `docs`, `evals`, `firmware`, ... |
| **OpenFang** | 9 | `agents`, `crates`, `deploy`, `docs`, `packages`, `public`, `scripts`, `sdk`, `xtask` |
| **IronClaw** | 16 | `crates`, `deploy`, `docker`, `docs`, `harness`, `infra`, `migrations`, `openwiki`, `profiles`, `prompts`, ... |
| **Zed** | 9 | `assets`, `ci`, `crates`, `docs`, `extensions`, `legal`, `nix`, `script`, `tooling` |
| **Asterinas** | 7 | `book`, `distro`, `kernel`, `osdk`, `ostd`, `test`, `tools` |

- **`fw`** : monorepo plat à couches séparées : `crates/fw-*` (13 crates Rust purs), `os/` (Asterinas OSDK + WSL), `nix/` (fleet NixOS + `flake.nix`), `cloudflare/` + `vercel/` (edge), `web/` (control-plane embarqué), `plugin/` (10 skills `agentskills.io`), `tools/` (audit concurrentiel) et `xtask/` (CI parallèle Rust pure). Un index `MAP.tsv` (< 600 tokens) évite aux agents de scanner l'arbre.
- **ZeroClaw** (44 members) : découpage dense par domaine agent (canaux, mémoire, providers, outils, sécurité, TUI/web) avec `crates/` et dossiers auxiliaires (`docs`, `scripts`, `web`, `deploy`).
- **OpenFang** (14 members) : structure compacte centrée sur `crates/` (runtime agent, mémoire, skills, CLI, gateway).
- **IronClaw** (68 members) : workspace étendu autour de `crates/`, `channels-src/` et `tools-src/` pour isoler les connecteurs Wasm et les politiques TEE.
- **Zed** (263 members) : monorepo centré sur `crates/` (GPUI, langage/LSP, collaboration, ACP/agent, extensions WIT) et `tooling/`.
- **Asterinas** (57 members) : séparation stricte entre `ostd/` (couche TCB minimale), `kernel/` (services Linux-compatibles en Rust sûr) et `osdk/` (outillage de build/test QEMU).

## 3. Architecture

| Projet | Modèle d'exécution | Isolation & sûreté | Cibles compilées |
|---|---|---|---|
| **`fw`** | Binaire Fat-LTO `fw-bin` + contrôle-plan HTTP/WS/MCP/GraphQL + module `fw-wasm` | 12/13 #![forbid(unsafe_code)] (seul `fw-wasm` expose l'ABI C/Wasm) | Windows MSVC, Linux `x86_64-unknown-linux-musl`, `wasm32-unknown-unknown`, Asterinas OSDK |
| **ZeroClaw** | Daemon Tokio multi-canaux (Telegram/Discord/Slack/HTTP/CLI) | Sandboxing OS/politiques outil + crates système | Linux, macOS, Windows |
| **OpenFang** | Runtime agent Rust + boucle autonome & registre de skills | Permissions par outil et sandboxing de commandes | Linux, macOS, Windows |
| **IronClaw** | Hôte Rust + plugins Wasm (Wasmtime) + attestation TEE (NEAR AI) | Isolation mémoire par bac à sable Wasm + enclaves | Linux (TEE/cloud), macOS, Wasm32 |
| **Zed** | Application desktop GPU native (GPUI) + serveur collab + hôte ACP | `unsafe` confiné aux bindings Metal/Vulkan/DirectX/Win32/Cocoa | macOS, Linux, Windows |
| **Asterinas** | Noyau Framekernel (OSTD = petite TCB, services noyau en Rust sûr) | Séparation formelle TCB (`ostd`) / reste du noyau (`#![deny(unsafe_code)]`) | Bare-metal / QEMU `x86_64`, `riscv64` |

## 4. Dépendances

- **`fw`** : **5 dépendances externes** (`serde`, `serde_json`, `sha2`, `tokio`, `flate2`), zéro dépendance C/OpenSSL/libgit2 dans le workspace principal. Compilation incrémentale courte, binaire statique musl Alpine sans glibc, surface d'audit supply-chain réduite.
- **OpenFang** : 63 dépendances externes racine (63 au total). Runtime agent généraliste compact sur stack Tokio/Reqwest/SQLite.
- **Asterinas** : 52 dépendances externes racine (105 au total). Crates `no_std` noyau, allocateurs, virtio, ACPI et pilotes bas niveau.
- **IronClaw** : 35 dépendances externes racine (89 au total). Volume porté par Wasmtime, crypto TEE, bases de données et SDK providers.
- **ZeroClaw** : 67 dépendances externes racine (75 au total). Volume porté par les connecteurs messagerie, clients LLM, stockage vectoriel et TUI.
- **Zed** : 303 dépendances externes racine (545 au total). Volume lié à un IDE complet (tree-sitter, wgpu/blade, font-kit, webrtc, lsp-types, git2/gix, protobuf).

## 5. Feature Matrix

| Capacité | `fw` | ZeroClaw | OpenFang | IronClaw | Zed | Asterinas |
|---|:---:|:---:|:---:|:---:|:---:|:---:|
| **OS / Userland** | Oui (`fw-uutils`, Alpine musl, NixOS) | Non | Non | Non | Non | Oui (Linux ABI) |
| **Kernel** | Oui (`os/fw-os` Asterinas OSDK + `fw-ostd-wasm`) | Non | Non | Non | Non | Oui (Framekernel OSTD) |
| **Wasm** | Oui (`fw-wasm` `wasm32-unknown-unknown`) | Partiel | Partiel | Oui (Wasmtime plugins) | Oui (extensions WIT) | Non |
| **WebGPU** | Oui (`fw-wayland-wgpu`, WGSL, probe web) | Non | Non | Non | Oui (GPUI Metal/Vulkan/DX12) | Non |
| **Cluster / Routing** | Oui (`fw-cluster` Pingora/Dynamo/vLLM-Router) | Non | Non | Partiel (TEE nodes) | Partiel (collab server) | Non |
| **MCP (JSON-RPC 2.0)** | Oui (`fw-hermes-claw` + `POST /mcp`) | Oui | Oui | Oui | Oui (client MCP) | Non |
| **ACP (Agent Client Protocol)** | Partiel (planifié / pont `agy`) | Partiel | Non | Partiel | Oui (natif) | Non |
| **A2A (Agent-to-Agent)** | Partiel (spec `fw-cluster` / relay) | Partiel | Oui | Oui | Non | Non |
| **Cloudflare Edge** | Oui (`cloudflare/` Workers + SQLite DO) | Non | Non | Non | Non | Non |
| **Nix** | Oui (`flake.nix` flake-parts + crane + NixOS) | Partiel | Non | Non | Oui (`nix/` flake-parts + crane) | Partiel (dev env) |
| **WSL** | Oui (`os/fw-wsl` tuning WSL + GPU) | Non | Non | Non | Oui (WSL remote) | Non |
| **CLI-to-Skill** | Oui (`fw-hermes-claw` `agentskills.io` gen) | Partiel (skills MD) | Oui (skill registry) | Partiel (Wasm tools) | Oui (skills / rules) | Non |

## 6. Comparaison UI/UX (Web / GPUI / Blitz / TUI)

| Surface UI | `fw` | ZeroClaw / OpenFang / IronClaw | Zed | Asterinas |
|---|---|---|---|---|
| **Web** | Dashboard embarqué zéro-build (`web/index.html` servi par `fw-proto` sur `/`, câblé sur `/health`, `/cluster`, `/claw/*`, `/mcp`, WebGPU probe) | Web UI optionnelle (SPA JS/TS séparée ou gateway HTTP) | Non (desktop natif) | Non |
| **GPUI** | Modèle scène/couches `fw-ostd-wasm` + `fw-wayland-wgpu` | Non | Référence GPU desktop (60-120 FPS, rendu natif, Taffy layout) | Non |
| **Blitz** | Moteur déclaratif HTML/CSS-vers-GPU (`fw-ui-blitz`) sans dépendance Chromium | Non | Non | Non |
| **TUI** | `tui-test` + `wta` (forks) (contrôle PTY, snapshots, locators) | TUI Ratatui / CLI interactive centrée chat & logs | Terminal intégré GPUI | Console série / QEMU |

**Constat UI/UX :** `fw` combine un panneau Web embarqué instantané (zéro bundler au runtime) avec une base graphique Rust (`fw-ui-blitz` + `fw-wayland-wgpu`) et une suite de test terminal (`tui-test`). Zed domine le rendu GPU desktop (GPUI), tandis que ZeroClaw, OpenFang et IronClaw concentrent leurs efforts sur la CLI/TUI et les canaux messagerie.

## 7. Pistes d'action concrètes pour `fw`

1. **Verrouiller le budget dépendances (<= 10 crates externes racine)** : l'écart de 5 vs 35-303 dépendances externes est le principal avantage structurel de `fw` (temps de build, binaires statiques musl, audit sécurité). Toute nouvelle fonctionnalité doit rester en Rust pur ou isolée hors chemin critique.
2. **Compléter la parité filaire ACP + A2A dans `fw-proto` et `fw-hermes-claw`** : Zed impose ACP côté IDE et IronClaw/OpenFang poussent A2A côté agents distribués. Exposer des handlers JSON-RPC ACP et A2A dans `fw-proto` (à côté de `POST /mcp`) aligne `fw` sur les trois standards sans dépendance externe supplémentaire.
3. **Renforcer le bac à sable Wasm de `fw-wasm` pour les skills tiers** : reprendre le modèle d'isolation par capacités d'IronClaw et des extensions WIT de Zed pour exécuter les skills générés par `CLI-to-Skill` avec permissions explicites (FS, réseau, env).
4. **Ajouter une vérification visuelle headless sur `fw-ui-blitz` et `fw-wayland-wgpu`** : produire des captures framebuffer/PNG déterministes en test pour rapprocher la rigueur visuelle de GPUI sans embarquer Chromium.
5. **Maintenir la veille reproductible via `just competitors`** : exécuter `bun tools/competitor_audit.ts` périodiquement pour suivre l'évolution des étoiles, licences, tailles de workspace et dérives de dépendances des cinq pairs dans `docs/research/rust-competitors-matrix.tsv`.
