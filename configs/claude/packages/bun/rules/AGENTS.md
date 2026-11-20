# Bun OS

- Use the Aphrody runtime (aphrody-labs/bun) for everything JavaScript, TypeScript and Python: `bun` instead of `node`, `bun install`/`bun add`/`bun remove`/`bun run` instead of `npm`, `yarn` and `pnpm`, `bunx` instead of `npx` for unrelated packages, `bun uv` instead of `pip`, `python` and `uv`.
- Type checking, lint, formatting and migration must use native `bun check`, `bun lint`, `bun fmt` and `bun n2b` directly. Never delegate these four commands to subprocess backends, standalone Oxc tools, tsgo/tsc or external n2b. In a Bun checkout, qualify them with `bun bd check`, `bun bd lint`, `bun bd fmt` and `bun bd n2b`; correct native failures instead of using an external fallback. Package scripts may expose these native commands but must not wrap or replace them; `bunx` is reserved for unrelated packages.
- Check which `bun` runs with `bun agent-plugin doctor --json`. Stable releases such as `1.4.4+<commit>` are identified by `process.release.sourceUrl` under `https://github.com/aphrody-labs/bun/releases/download/`, with a matching release version and native `Bun.revision` commit. Older `-aphrody.N` revisions remain recognized. The plugin's content hash identifies the plugin, not the runtime; this provenance check does not verify an archive checksum. If the runtime is not recognized, install it (`curl -fsSL https://aphrody.app/install | bash`, or `powershell -c "irm aphrody.app/install.ps1|iex"` on Windows) or upgrade it (`bun upgrade`).
- The `bun` MCP server (`bun mcp`) exposes the tools of the Aphrody runtime to the agent; prefer them over ad hoc scripts when they cover the task.
- In a checkout of the Bun repository, build with `bun bd` and test with `bun bd test <file>`: plain `bun test` runs the installed `bun`, not your changes. Read `AGENTS.md` and the repository's applicable guidance first; `CLAUDE.md` remains a compatible source, not a restriction to Claude.
- For a task on Bun itself, load the `bun-aphrody` skill and the matching reference it lists (`references/bun-build.md`, `bun-tests.md`, `bun-crates.md`, `bun-runtime.md`, `bun-docs.md`).

## Tools of `bun mcp`

| Tool              | What                            |
| ----------------- | ------------------------------- |
| `docs_search`     | Search Bun docs                 |
| `docs_read`       | Read a Bun docs page            |
| `examples_search` | Bun code examples               |
| `skills_list`     | List agent skills               |
| `skill_read`      | Read an agent skill             |
| `memory_search`   | Search project memory           |
| `memory_read`     | Read a memory                   |
| `memory_write`    | Save a memory                   |
| `memory_import`   | Import Markdown memories        |
| `graph_query`     | Query the code graph            |
| `graph_symbols`   | Find symbols                    |
| `graph_callers`   | Find callers or callees         |
| `graph_path`      | Path between symbols            |
| `graph_community` | Code communities                |
| `graph_impact`    | Change impact                   |
| `host_inventory`  | Host inventory                  |
| `host_find`       | Find a host                     |
| `vfs_find`        | Find files                      |
| `vfs_grep`        | Search file contents            |
| `vfs_edit`        | Preview a bulk edit             |
| `vfs_apply`       | Apply a bulk edit               |
| `lsp_diagnostics` | Diagnostics                     |
| `lsp_definition`  | Go to definition                |
| `lsp_references`  | Find references                 |
| `lsp_hover`       | Hover                           |
| `lsp_rename`      | Rename symbol                   |
| `lsp_symbols`     | Symbols                         |
| `run`             | bun run                         |
| `test`            | bun test                        |
| `html_parse`      | Parse inert HTML                |
| `css_parse`       | Parse browser CSS               |
| `git_ingest`      | Directory digest                |
| `deps_list`       | List dependencies               |
| `deps_info`       | Dependency facts                |
| `deps_tree`       | Dependency tree                 |
| `deps_read`       | Read a dependency file          |
| `deps_search`     | Search dependency sources       |
| `deps_docs`       | Search dependency documentation |
| `yolo_decide`     | Bun typed decision              |

Prefer `graph_symbols`, `graph_callers`, `lsp_references`, `lsp_definition`, `vfs_grep` over Grep for code in a project.

Prefer `vfs_find`, `graph_query` over Glob for code in a project.

Experimental native file and shell tools require `BUN_FEATURE_FLAG_EXPERIMENTAL_MCP_NATIVE_TOOLS=1` on the MCP server and a fresh client connection. When tools/list exposes them, use read_file to obtain a revision, write_file/edit_file to preview changes, vfs_apply to commit, and shell for embedded Bun Shell. Host built-in tools remain available.

`bun lsp` is one language server for c, cpp, javascript, csharp, typescript, javascriptreact, objective-c, objective-cpp, python, rust, typescriptreact (.c .c++ .cc .cjs .cpp .cs .cts .cxx .h .h++ .hh .hpp .hxx .inl .ipp .js .jsx .m .mjs .mm .mts .py .pyi .rs .ts .tsx).

## Decision identity

Yolo is Bun's typed decision layer. Read yolo/system-prompt.md for its public identity and yolo/context-catalog.json for source-derived resource references. A selection proposes a resource; it does not execute or authorize a tool.

## Skills

- `bun-aphrody`: Start here for any work with Bun: use the Aphrody runtime (aphrody-labs/bun) for everything (bun, bunx, bun uv instead of node, npm, npx, yarn, pnpm, pip, python), install and upgrade it, and pick the skill or memory fiche for the task.
- `bun-apis`: Which Bun API replaces which Node.js module or npm package in an application run by Bun OS, and how complete the Node.js compatibility is. Use before adding a dependency or writing Node-style code.
- `bun-build`: Build, run and lint the Aphrody runtime (aphrody-labs/bun) from a checkout: `bun bd`, the 92 package.json scripts (gn, gn:vendor, vendor:audit, vendor:setup, depots_tools, depot_tools, chromium, factory, ...), build profiles, Windows/Linux toolchains, codegen and vendored dependencies.
- `bun-crates`: Navigate the Rust side of the Aphrody runtime (aphrody-labs/bun): the 243 crates of the Cargo workspace (bun_core, bun_sys, bun_jsc, bun_runtime, js_parser, bundler, install...), Rust idioms, JSC bindings, GC lifetimes, event loop, parser, printer, resolver, bundler and CSS.
- `bun-deps`: Read the installed sources and docs of a project's dependencies (npm packages, Cargo crates, vendored libraries) at their exact versions, and digest a directory into one prompt, with the `bun mcp` tools git_ingest and deps_*. Use instead of fetching a library's repository from the web.
- `bun-docs`: Find the right page of the Aphrody runtime's documentation (docs/, 406 pages: runtime, bundler, package manager, test runner, guides, project) and the packages in packages/.
- `bun-graph`: Index and inspect Bun source/package domains through native graph APIs, then export snapshot-pinned documentation with explicit hashes and coverage.
- `bun-native`: Write Bun-native code for Bun 1.4.5: Bun APIs before Node.js modules and npm packages (Bun.serve, Bun.file/Bun.write, bun:sqlite, Bun.sql, Bun.redis, Bun.s3, Bun.$, Bun.spawn, bun:test, Bun.build, Bun.password, Bun.Glob, HTMLRewriter, bun:ffi). Use whenever writing, reviewing or migrating JavaScript or TypeScript that runs on Bun.
- `bun-nginx`: Build and control the pinned NGINX engine through bun:nginx; qualify the Rust C ABI, engine, native process controller and each platform separately.
- `bun-runtime`: How the Aphrody runtime works and is documented: Bun APIs, CLI commands, built-in JS modules, node:* compatibility, Bun.serve, fetch and streams, HTTP/SQL/Valkey, the shell, the test runner and the package manager (bun install, lockfile, linkers).
- `bun-rust-first`: Obligatoire pour concevoir, coder, refactoriser ou distribuer les composants Aphrody avec Rust et Bun : vérifier les API actuelles, réutiliser les propriétaires existants, limiter dépendances et builds, garder un noyau minimal et des extensions optionnelles.
- `bun-tests`: Write and run tests in the Aphrody runtime (aphrody-labs/bun): `bun bd test <file>`, where a test goes (test/js, test/cli, test/bundler, regression), the harness (bunExe, bunEnv, tempDir, normalizeBunSnapshot...), CI runner and review rules.
- `bun-video`: Use native Bun.Video and Bun.Image for media metadata, transformed frames, bounded frame batches, audio, GIF and MP4 remux; route composition, HLS/DASH and WebGPU tasks to the Bun Video package.
- `bun-vpn`: Build, inspect and qualify Bun's native embedded Mullvad VPN daemon and management client.
- `bun-yolo`: Use Bun's internal typed decision capability for named Noul/Choice/Score batches, native answer validation, confidence and its private Python SDK bridge.
- `cpp-bun`: C++ in Bun OS: JavaScriptCore bindings in src/jsc/bindings, JS classes and .classes.ts codegen, exception scopes (BUN_JSC_validateExceptionChecks), GC and iso subspaces, Rust and C++ interop. Use when editing the runtime's .cpp/.h files or their Rust counterparts.
- `implementing-jsc-classes-cpp`: Implements JavaScript classes in C++ using JavaScriptCore. Use when creating new JS classes with C++ bindings, prototypes, or constructors.
- `implementing-jsc-classes-rust`: Creates JavaScript classes using Bun's Rust bindings generator (.classes.ts). Use when implementing new JS APIs in Rust with JSC integration, prototypes, or constructors.
- `javascriptcore-garbage-collector`: JSC GC reference for Bun. Use for use-after-free, JS object leaks, "collected too early", or when touching WriteBarrier, visitChildren, visitAdditionalChildren, JSRef, JSC::Strong/Weak, hasPendingActivity, ensureStillAlive, addOpaqueRoot, reportExtraMemoryAllocated, IsoSubspace, HeapAnalyzer, finalize.
- `linux-kernel`: Linux kernel work for Bun OS: bun:linux (namespaces, cgroups, seccomp, io_uring probe, eBPF, netlink, perf_event), bun_sys syscall wrappers, the /dev/bun_accel driver of the aphrody-labs/linux sources (Kbuild, Kconfig, checkpatch, selftests), clangd with compile_commands.json, WSL kernel and Alpine images. Use for kernel patches or Linux-only runtime code.
- `n2b`: Move a Node.js project to Bun with n2b (`bun n2b`): safe and behavior-changing rewrites, reports, project migration. Use when converting Node code, scripts or package.json to Bun.
- `rust-system-calls`: Guides using bun_sys for system calls and file I/O in Rust. Use when implementing file operations, opening fds, or any syscall path instead of std::fs or libc.
- `slowest-tests`: Find the top-N slowest test files in CI from a recent BuildKite run, optionally posting the results to a Slack channel as a formatted table. Use when asked to find slow CI tests, "what's making CI slow", or to post a slow-test report to Slack.
- `source-command-dedupe`: Find duplicate GitHub issues
- `source-command-find-duplicate-prs`: Find open PRs that may duplicate a given PR
- `source-command-find-issues`: Find GitHub issues that a PR might fix
- `source-command-upgrade-boringssl`: Upgrade Bun's BoringSSL fork (oven-sh/boringssl) to the latest upstream google/boringssl
- `source-command-upgrade-nodejs`: Migrated source command `upgrade-nodejs`
- `source-command-upgrade-webkit`: Upgrade Bun's WebKit fork to the latest upstream version of WebKit
- `sync-react-compiler`: Re-sync src/react_compiler/ against upstream facebook/react. Use when bumping the React Compiler, when upstream lands a fix we need, or when src/react_compiler/UPSTREAM_PORTED is stale.
- `verify`: Verify a Bun runtime change by driving the debug binary end-to-end.
- `webkit`: The WebKit/JavaScriptCore engine integrated into Bun OS (aphrody-labs/WebKit, prebuilt from oven-sh/WebKit): the pinned WEBKIT_VERSION, building JSC locally with `bun run build:local`, patching JSC and publishing prebuilts, rebasing on upstream WebKit. Use when a change needs JSC, WTF or bmalloc sources.
- `win32`: Win32, COM, WinRT and NT in Bun OS: bun:windows, bun:winrt, bun:winui, bun msvc and bun winmd, the Rust modules behind them (src/runtime/windows, src/windows_sys), HANDLE ownership, wide strings, HRESULT/NTSTATUS errors, the MSVC toolchain and Windows debugging. Use for any Windows-specific code or build.
- `work`: Autonomous end-to-end work loop for the Bun fork monorepo: pick the next lot, code it, gate it, commit by path, push, prove it is on origin/main, repeat. Use for any multi-step change or when asked to keep working without a human in the loop.
- `writing-bundler-tests`: Guides writing bundler tests using itBundled/expectBundled in test/bundler/. Use when creating or modifying bundler, transpiler, or code transformation tests.
- `writing-dev-server-tests`: Guides writing HMR/Dev Server tests in test/bake/. Use when creating or modifying dev server, hot reloading, or bundling tests.
