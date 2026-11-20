---
name: bun-aphrody
description: "Start here for any Bun work: use the Aphrody runtime (aphrody-labs/bun) for everything (bun and bunx instead of node, npm, npx, yarn, pnpm, pip). Use BEFORE any choice of JS/TS tool, script, test, format, typecheck or build. Routes to Bun APIs vs Node/npm, runtime docs, building and testing the runtime, Rust crates, C++/JSC bindings, nginx, video, VPN, yolo, legacy-Node migration (n2b) and the Bun graph."
---
# Bun with the Aphrody runtime

`references/core.md` is the former body of this skill (install, upgrade, runtime rules, memory fiches): read it first, then the reference that matches the task.

Read the reference that matches the task in full before acting. Paths written inside reference files are relative to this skill's directory (the one holding this SKILL.md). Where a reference names another merged skill, open `references/<name>.md`; its helper files are under `references/<name>/`.

| Reference | Use when |
| --- | --- |
| [references/core.md](references/core.md) | Former body of this skill (read first) |
| [references/bun.md](references/bun.md) | bun reference (CLI, runtime API, upstream docs), bun:ffi bridge to native C ABIs, zero-dependency / zero-bunx package rules with native replacements (Bun.SQL, bun:sqlite, Bun.S3Client, Bun.Image, Bun.serve, Bun.Glob, bun fmt, bun check, bun build --compile). Use BEFORE any choice of JS/TS tool, script, test, format,... |
| [references/bun-native.md](references/bun-native.md) | Write Bun-native code for Bun 1.4.5: Bun APIs before Node.js modules and npm packages (Bun.serve, Bun.file/Bun.write, bun:sqlite, Bun.sql, Bun.redis, Bun.s3, Bun.$, Bun.spawn, bun:test, Bun.build, Bun.password, Bun.Glob, HTMLRewriter, bun:ffi). Use whenever writing, reviewing or migrating JavaScript or TypeScript that... |
| [references/bun-apis.md](references/bun-apis.md) | Which Bun API replaces which Node.js module or npm package in an application run by Bun OS, and how complete the Node.js compatibility is. Use before adding a dependency or writing Node-style code. |
| [references/bun-awesome.md](references/bun-awesome.md) | Curated Bun 1.4+ stack, ecosystem libraries, frameworks, and architectural patterns. Use when choosing libraries or templates for web servers, APIs, databases, fullstack apps, workers, containers, or CI with Bun. |
| [references/bun-rust-first.md](references/bun-rust-first.md) | Obligatoire pour concevoir, coder, refactoriser ou distribuer les composants Aphrody avec Rust et Bun : vérifier les API actuelles, réutiliser les propriétaires existants, limiter dépendances et builds, garder un noyau minimal et des extensions optionnelles. |
| [references/bun-legacy-replace.md](references/bun-legacy-replace.md) | Analyze and migrate legacy Node.js, npm/Yarn/pnpm, CommonJS, Dockerfile, CI, and tsconfig codebases to Bun 1.4+ native APIs and commands. Use when scanning for legacy JS/TS patterns, replacing npm packages with Bun built-ins, or planning a Bun/Rust migration. |
| [references/n2b.md](references/n2b.md) | Move a Node.js project to Bun with n2b (`bun n2b`): safe and behavior-changing rewrites, reports, project migration. Use when converting Node code, scripts or package.json to Bun. |
| [references/bun-deps.md](references/bun-deps.md) | Inspect installed npm/Cargo/vendor dependency trees, metadata, and docs offline at their exact installed versions using bun pm ls/why/info, relay core_ingest/core_query, and local node_modules/Cargo sources instead of guessing from web searches. |
| [references/bun-runtime.md](references/bun-runtime.md) | How the Aphrody runtime works and is documented: Bun APIs, CLI commands, built-in JS modules, node:* compatibility, Bun.serve, fetch and streams, HTTP/SQL/Valkey, the shell, the test runner and the package manager (bun install, lockfile, linkers). |
| [references/bun-docs.md](references/bun-docs.md) | Find the right page of the Aphrody runtime's documentation (docs/, 406 pages: runtime, bundler, package manager, test runner, guides, project) and the packages in packages/. |
| [references/bun-build.md](references/bun-build.md) | Build, run and lint the Aphrody runtime (aphrody-labs/bun) from a checkout: `bun bd`, the 92 package.json scripts (gn, gn:vendor, vendor:audit, vendor:setup, depots_tools, depot_tools, chromium, factory, ...), build profiles, Windows/Linux toolchains, codegen and vendored dependencies. |
| [references/bun-tests.md](references/bun-tests.md) | Write and run tests in the Aphrody runtime (aphrody-labs/bun): `bun bd test <file>`, where a test goes (test/js, test/cli, test/bundler, regression), the harness (bunExe, bunEnv, tempDir, normalizeBunSnapshot...), CI runner and review rules. |
| [references/bun-crates.md](references/bun-crates.md) | Navigate the Rust side of the Aphrody runtime (aphrody-labs/bun): the 137 workspace crates of the Cargo workspace (bun_core, bun_sys, bun_jsc, bun_runtime, js_parser, bundler, install...), Rust idioms, JSC bindings, GC lifetimes, event loop, parser, printer, resolver, bundler and CSS. |
| [references/cpp-bun.md](references/cpp-bun.md) | C++ in Bun OS: JavaScriptCore bindings in src/jsc/bindings, JS classes and .classes.ts codegen, exception scopes (BUN_JSC_validateExceptionChecks), GC and iso subspaces, Rust and C++ interop. Use when editing the runtime's .cpp/.h files or their Rust counterparts. |
| [references/bun-graph.md](references/bun-graph.md) | Index and inspect Bun source/package domains through native graph APIs, then export snapshot-pinned documentation with explicit hashes and coverage. |
| [references/bun-gpui.md](references/bun-gpui.md) | Bun integration in aphrody via the `vendor/bun` submodule (aphrody-labs/bun). Use whenever working on TypeScript/TSX Native Shell (NSH) surfaces, typechecking (`bun check` / `nsh check`), formatting (`bun fmt` / `nsh fmt`), linting (`bun lint` / `nsh lint`), bundling (`Bun.build` / `bun build`), or reading upstream... |
| [references/bun-nginx.md](references/bun-nginx.md) | Build and control the pinned NGINX engine through bun:nginx; qualify the Rust C ABI, engine, native process controller and each platform separately. |
| [references/bun-video.md](references/bun-video.md) | Use native Bun.Video and Bun.Image for media metadata, transformed frames, bounded frame batches, audio, GIF and MP4 remux; route composition, HLS/DASH and WebGPU tasks to the Bun Video package. |
| [references/bun-vpn.md](references/bun-vpn.md) | Build, inspect and qualify Bun's native embedded Mullvad VPN daemon and management client. |
| [references/bun-yolo.md](references/bun-yolo.md) | Use Bun's internal typed decision capability for named Noul/Choice/Score batches, native answer validation, confidence and its private Python SDK bridge. |
| [references/bun-guide.md](references/bun-guide.md) | Master guide and router for Bun 1.4+ development in Antigravity: navigates ~/.gemini/brain/bun/ knowledge digests, bun-brain MCP tools, bun-lsp diagnostics/hover, and native Bun CLI tooling (check, lint, fmt, test, build, pm). |

<!-- merge-skill-families:v1 -->
