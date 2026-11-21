# fw end to end (handoff)

Task: make `fw` work end to end and prove it with a table of every command, its result and its timing. Worker stopped at a clean checkpoint (orchestrator request); the e2e table becomes part of the Phase 1 gate after the workspace unification.

## State

Pushed to origin/main: `744bfe1b` (fw-e2e.ts), `723244af` (self-update), `fd3f3419` (help, panics, /mcp parse error), `78c1b26a` (fw-e2e.ts expectations). Nothing uncommitted.

Last run (debug build of fw-bin at `78c1b26a` minus nothing, fr1, Linux x86-64, `libfw_abi.so` from native/fw-abi): 305/305 ok, 0 failed, total 4192 ms, fw /srv/xbuild/fw-e2e/target/debug/fw, sandbox /srv/xbuild/fw-e2e/tmp/fw-e2e-21DZfU

## Fixed (root cause)

- `fw self-update` / `fw upgrade`: `/releases/latest` is 404 when every release is a pre-release (the only release is `dev-v2026.11.20-d4b85e4`), so it printed "ok, up to date" with a note. Now falls back to the newest published release, matches the ay.release.v1 asset names (`fw-x86_64-windows.exe`, `fw-x86_64-linux-musl`, `fw-aarch64-macos`), reads `SHA256SUMS` (the old code looked for "checksum" in the name), decides up to date by comparing the running binary digest, exits nonzero when the release or asset cannot be resolved. Real upgrade of a temp copy on fr1: downloaded 1954696 bytes, SHA-256 verified, `.prev` kept.
- `fw help <cmd>` for status, hardware, compute, ggml, openapi, ui, ostd, uutils, nix, stack, wayland, claw, migrate, ls, table, find, grep printed the root help; now per-command help (`DIRECT_COMMANDS` in cli_parser.rs, unit test).
- `fw ui`, `fw ggml`, `fw wayland` used `expect`/`unwrap`; now exit 1 with a message.
- `run_sibling` build hint now names the feature builds for `fw-graph` (`-p fw-rag --features codegraph --bin fw-graph`) and `fw-k8s` (`-p fw-cluster --features kube --bin fw-k8s`).
- `POST /mcp` with a non-JSON body answered 200 "method not found"; now 400 with JSON-RPC -32700.
- `cargo test -p fw-bin --bin fw`: 17 passed.

## Open

- `fw graph build` and `fw cluster k8s` were only checked in their "sibling binary absent" path (exit 127 with the build command); the feature builds (`codegraph`, `kube`) were not built, so the real invocations are untested.
- `fw convert` was only checked on a non-video input (clean refusal, exit 2: "MP4 demux: no moov box found"); no MP4 fixture.
- `fw tui` without a TTY exits 0 after printing escape codes; the case only checks exit 0 and no hang.
- The upgrade up-to-date branch (digest equal to the release asset) is untested: the published release (d4b85e4) is older than main, so the upgraded copy is only required to start (`fw help`).
- Windows: the e2e ran on the PC for hooks (14/14) and MCP (18/18, no C ABI so fw_ast_parse, fw_pe_inspect, fw_disasm answer "abi not found" as a clean isError) against the installed older fw.exe. The CLI table was not run on Windows with a current build (no Windows build of main exists; heavy builds stay off the PC).
- `hooks.json SessionStart` (auto-update.ts) takes about 1.1 s through `sh -c` from a pipe-attached Windows parent when the stamp is stale (about 30 ms otherwise): the detached `node:child_process` child keeps the pipe open. `Bun.spawn(...).unref()` returns in 60 ms but the child does not survive the parent on Windows, `detached: true` blocks again, so the hook is unchanged (3 s budget, passes).
- MCP `serverInfo.version` in src/mcp_server.ts is the constant "2.3.0".
- Not touched: Cargo.toml files, deny.toml, CI workflow, rust-audit.ts, rust-stack.ts, crates/fw-material, crates/fw-browser*, toolchain.toml WebAssembly entries.

## Resume

Build on fr1 (clone and target under /srv/xbuild, debug is enough):

```sh
export CARGO_TARGET_DIR=/srv/xbuild/<dir>/target TMPDIR=/srv/xbuild/<dir>/tmp CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0
nice -n 10 cargo build -p fw-bin -p fw-files -p fw-video -p fw-google
nice -n 10 cargo build -p fw-rag --features codegraph --bin fw-graph   # optional siblings
nice -n 10 cargo build -p fw-cluster --features kube --bin fw-k8s
(cd native/fw-abi && CARGO_TARGET_DIR=/srv/xbuild/<dir>/target-abi cargo build)
bun configs/claude/scripts/fw-e2e.ts --fw $CARGO_TARGET_DIR/debug/fw --abi /srv/xbuild/<dir>/target-abi/debug/libfw_abi.so
```

Flags: `--only cli,serve,mcp,hooks,upgrade`, `--fast` (skips mcp and upgrade), `--json`, `--keep` (keep the sandbox), `--root <fw checkout>`. Everything that writes runs in a temporary HOME and FX_HOME, token-like environment variables are removed.

## Table (fr1, Linux, debug build)

| section | command | result | ms |
|---|---|---|---|
| cli | `fw --version` | ok | 4 |
| cli | `fw --help` | ok | 4 |
| cli | `fw help ask` | ok | 4 |
| cli | `fw help chat` | ok | 5 |
| cli | `fw ask --help` | ok | 4 |
| cli | `fw help tool` | ok | 4 |
| cli | `fw help x` | ok | 5 |
| cli | `fw tool --help` | ok | 4 |
| cli | `fw help tui` | ok | 4 |
| cli | `fw help m3-tui` | ok | 3 |
| cli | `fw help god` | ok | 3 |
| cli | `fw tui --help` | ok | 2 |
| cli | `fw help serve` | ok | 2 |
| cli | `fw help s` | ok | 4 |
| cli | `fw serve --help` | ok | 3 |
| cli | `fw help convert` | ok | 4 |
| cli | `fw help cv` | ok | 3 |
| cli | `fw convert --help` | ok | 2 |
| cli | `fw help google` | ok | 2 |
| cli | `fw google --help` | ok | 2 |
| cli | `fw help files` | ok | 2 |
| cli | `fw help fl` | ok | 2 |
| cli | `fw files --help` | ok | 4 |
| cli | `fw help infer` | ok | 3 |
| cli | `fw help run` | ok | 4 |
| cli | `fw infer --help` | ok | 4 |
| cli | `fw help train` | ok | 3 |
| cli | `fw help finetune` | ok | 4 |
| cli | `fw help lora` | ok | 4 |
| cli | `fw train --help` | ok | 3 |
| cli | `fw help hf` | ok | 3 |
| cli | `fw help huggingface` | ok | 3 |
| cli | `fw hf --help` | ok | 4 |
| cli | `fw help abliterate` | ok | 4 |
| cli | `fw help abl` | ok | 3 |
| cli | `fw abliterate --help` | ok | 3 |
| cli | `fw help vram` | ok | 3 |
| cli | `fw vram --help` | ok | 3 |
| cli | `fw help heretic` | ok | 4 |
| cli | `fw heretic --help` | ok | 3 |
| cli | `fw help align` | ok | 3 |
| cli | `fw help dpo` | ok | 3 |
| cli | `fw help orpo` | ok | 5 |
| cli | `fw align --help` | ok | 4 |
| cli | `fw help repomap` | ok | 3 |
| cli | `fw repomap --help` | ok | 3 |
| cli | `fw help watch` | ok | 4 |
| cli | `fw watch --help` | ok | 3 |
| cli | `fw help tune` | ok | 3 |
| cli | `fw help cachy` | ok | 3 |
| cli | `fw tune --help` | ok | 3 |
| cli | `fw help sec` | ok | 4 |
| cli | `fw help deepsec` | ok | 4 |
| cli | `fw sec --help` | ok | 3 |
| cli | `fw help cluster` | ok | 4 |
| cli | `fw cluster --help` | ok | 4 |
| cli | `fw help graph` | ok | 12 |
| cli | `fw help graphify` | ok | 5 |
| cli | `fw graph --help` | ok | 5 |
| cli | `fw help docs` | ok | 5 |
| cli | `fw help c7` | ok | 10 |
| cli | `fw help context7` | ok | 4 |
| cli | `fw docs --help` | ok | 4 |
| cli | `fw help learn` | ok | 3 |
| cli | `fw help autolearn` | ok | 3 |
| cli | `fw learn --help` | ok | 3 |
| cli | `fw help skills` | ok | 3 |
| cli | `fw skills --help` | ok | 3 |
| cli | `fw help onboard` | ok | 4 |
| cli | `fw help init-agent` | ok | 3 |
| cli | `fw onboard --help` | ok | 5 |
| cli | `fw help workflow` | ok | 3 |
| cli | `fw help wf` | ok | 3 |
| cli | `fw workflow --help` | ok | 7 |
| cli | `fw help self-install` | ok | 6 |
| cli | `fw self-install --help` | ok | 3 |
| cli | `fw help self-uninstall` | ok | 4 |
| cli | `fw self-uninstall --help` | ok | 3 |
| cli | `fw help config` | ok | 3 |
| cli | `fw config --help` | ok | 5 |
| cli | `fw help mcp` | ok | 3 |
| cli | `fw mcp --help` | ok | 4 |
| cli | `fw help acp` | ok | 3 |
| cli | `fw acp --help` | ok | 3 |
| cli | `fw help sessions` | ok | 3 |
| cli | `fw help session` | ok | 3 |
| cli | `fw sessions --help` | ok | 3 |
| cli | `fw help permissions` | ok | 3 |
| cli | `fw permissions --help` | ok | 4 |
| cli | `fw help models` | ok | 3 |
| cli | `fw help provider` | ok | 3 |
| cli | `fw models --help` | ok | 4 |
| cli | `fw help workspace` | ok | 3 |
| cli | `fw workspace --help` | ok | 4 |
| cli | `fw help pr` | ok | 4 |
| cli | `fw help issue` | ok | 3 |
| cli | `fw pr --help` | ok | 4 |
| cli | `fw help doctor` | ok | 4 |
| cli | `fw doctor --help` | ok | 3 |
| cli | `fw help completion` | ok | 4 |
| cli | `fw completion --help` | ok | 3 |
| cli | `fw help bench` | ok | 3 |
| cli | `fw bench --help` | ok | 3 |
| cli | `fw help help` | ok | 3 |
| cli | `fw help --help` | ok | 3 |
| cli | `fw help sh` | ok | 3 |
| cli | `fw sh --help` | ok | 3 |
| cli | `fw help jq` | ok | 4 |
| cli | `fw jq --help` | ok | 6 |
| cli | `fw help tokei` | ok | 4 |
| cli | `fw tokei --help` | ok | 6 |
| cli | `fw help du` | ok | 7 |
| cli | `fw du --help` | ok | 13 |
| cli | `fw help bat` | ok | 7 |
| cli | `fw bat --help` | ok | 10 |
| cli | `fw help init` | ok | 7 |
| cli | `fw init --help` | ok | 5 |
| cli | `fw help install` | ok | 11 |
| cli | `fw help i` | ok | 11 |
| cli | `fw install --help` | ok | 13 |
| cli | `fw help add` | ok | 15 |
| cli | `fw help a` | ok | 4 |
| cli | `fw add --help` | ok | 3 |
| cli | `fw help remove` | ok | 5 |
| cli | `fw help rm` | ok | 4 |
| cli | `fw help uninstall` | ok | 4 |
| cli | `fw remove --help` | ok | 3 |
| cli | `fw help lock` | ok | 14 |
| cli | `fw lock --help` | ok | 4 |
| cli | `fw help verify` | ok | 4 |
| cli | `fw verify --help` | ok | 7 |
| cli | `fw help info` | ok | 11 |
| cli | `fw help resolve` | ok | 10 |
| cli | `fw info --help` | ok | 12 |
| cli | `fw help fetch` | ok | 20 |
| cli | `fw help download` | ok | 9 |
| cli | `fw fetch --help` | ok | 6 |
| cli | `fw help tree` | ok | 5 |
| cli | `fw help scan` | ok | 4 |
| cli | `fw tree --help` | ok | 4 |
| cli | `fw help export` | ok | 4 |
| cli | `fw help sbom` | ok | 4 |
| cli | `fw export --help` | ok | 4 |
| cli | `fw help audit` | ok | 4 |
| cli | `fw audit --help` | ok | 6 |
| cli | `fw help compile` | ok | 7 |
| cli | `fw compile --help` | ok | 4 |
| cli | `fw help semver` | ok | 4 |
| cli | `fw semver --help` | ok | 4 |
| cli | `fw help no-such-command` | ok | 4 |
| cli | `fw no-such-command` | ok | 5 |
| cli | `fw help status` | ok | 4 |
| cli | `fw help hardware` | ok | 3 |
| cli | `fw help compute` | ok | 5 |
| cli | `fw help ggml` | ok | 4 |
| cli | `fw help openapi` | ok | 4 |
| cli | `fw help ui` | ok | 4 |
| cli | `fw help ostd` | ok | 4 |
| cli | `fw help uutils` | ok | 4 |
| cli | `fw help nix` | ok | 4 |
| cli | `fw help stack` | ok | 4 |
| cli | `fw help wayland` | ok | 5 |
| cli | `fw help claw` | ok | 4 |
| cli | `fw help migrate` | ok | 4 |
| cli | `fw --offline ask "say hi"` | ok | 4 |
| cli | `fw tool read_file "{\"path\":\"a.txt\"}"` | ok | 4 |
| cli | `fw tui` | ok | 5 |
| cli | `fw serve not-an-address` | ok | 3 |
| cli | `fw convert README.md $SANDBOX/out/convert --to html` | ok | 5 |
| cli | `fw --offline google drive list` | ok | 1297 |
| cli | `fw files ls $SANDBOX/work` | ok | 8 |
| cli | `fw --offline infer hello --url http://127.0.0.1:1/v1` | ok | 4 |
| cli | `fw train --steps 5 --out $SANDBOX/out/lora.json` | ok | 4 |
| cli | `fw --offline hf whoami` | ok | 3 |
| cli | `fw abliterate plan` | ok | 5 |
| cli | `fw vram --vram-gb 12 --model-gb 5.5 --layers 32 --ctx 8192` | ok | 4 |
| cli | `fw heretic presets` | ok | 4 |
| cli | `fw align dpo --steps 5` | ok | 5 |
| cli | `fw repomap .` | ok | 6 |
| cli | `fw watch list` | ok | 4 |
| cli | `fw tune hardware` | ok | 6 |
| cli | `fw sec .` | ok | 8 |
| cli | `fw cluster route hello` | ok | 4 |
| cli | `fw graph build . --json $SANDBOX/out/graph.json` | ok | 36 |
| cli | `fw --offline docs list` | ok | 4 |
| cli | `fw learn status --xfs-root $SANDBOX/out/xfs` | ok | 5 |
| cli | `fw --offline skills list` | ok | 4 |
| cli | `fw onboard .` | ok | 5 |
| cli | `fw workflow list` | ok | 4 |
| cli | `fw self-install --dir $SANDBOX/out/bin` | ok | 81 |
| cli | `fw self-uninstall --dir $SANDBOX/out/bin` | ok | 6 |
| cli | `fw config set model e2e-model` | ok | 3 |
| cli | `fw config get model` | ok | 3 |
| cli | `fw config list` | ok | 3 |
| cli | `fw mcp list` | ok | 3 |
| cli | `fw mcp add e2e echo hi` | ok | 3 |
| cli | `fw mcp remove e2e` | ok | 3 |
| cli | `fw acp` | ok | 3 |
| cli | `fw sessions` | ok | 3 |
| cli | `fw permissions` | ok | 4 |
| cli | `fw models` | ok | 4 |
| cli | `fw workspace` | ok | 4 |
| cli | `fw pr` | ok | 4 |
| cli | `fw doctor` | ok | 4 |
| cli | `fw completion bash` | ok | 3 |
| cli | `fw bench silicon` | ok | 62 |
| cli | `fw help` | ok | 11 |
| cli | `fw sh -c "echo hello` | wc -c" | ok |
| cli | `fw jq .a.b[1] data.json` | ok | 4 |
| cli | `fw tokei .` | ok | 3 |
| cli | `fw du .` | ok | 4 |
| cli | `fw bat a.txt` | ok | 4 |
| cli | `fw init` | ok | 4 |
| cli | `fw --offline install` | ok | 4 |
| cli | `fw --offline add left-pad` | ok | 6 |
| cli | `fw remove left-pad` | ok | 5 |
| cli | `fw lock .` | ok | 5 |
| cli | `fw verify .` | ok | 4 |
| cli | `fw --offline info cargo serde` | ok | 3 |
| cli | `fw --offline fetch cargo serde` | ok | 4 |
| cli | `fw tree .` | ok | 4 |
| cli | `fw export .` | ok | 3 |
| cli | `fw --offline audit` | ok | 3 |
| cli | `fw compile` | ok | 3 |
| cli | `fw semver 1.2.3 ^1.0.0` | ok | 3 |
| cli | `fw cluster k8s status` | ok | 3 |
| cli | `fw status` | ok | 3 |
| cli | `fw hardware` | ok | 3 |
| cli | `fw compute` | ok | 4 |
| cli | `fw ggml` | ok | 5 |
| cli | `fw openapi` | ok | 4 |
| cli | `fw ui --out $SANDBOX/out/ui.png` | ok | 72 |
| cli | `fw ostd` | ok | 4 |
| cli | `fw uutils uname` | ok | 4 |
| cli | `fw nix fw-e2e` | ok | 3 |
| cli | `fw stack` | ok | 5 |
| cli | `fw wayland` | ok | 4 |
| cli | `fw claw ranking` | ok | 4 |
| cli | `fw migrate bun` | ok | 4 |
| cli | `fw ls .` | ok | 3 |
| cli | `fw table . --json` | ok | 4 |
| cli | `fw find a.txt .` | ok | 5 |
| cli | `fw grep hello .` | ok | 5 |
| cli | `uname` | ok | 257 |
| cli | `echo multicall` | ok | 24 |
| cli | `cat a.txt` | ok | 28 |
| serve | `start` | ok | 102 |
| serve | `GET /` | ok | 1 |
| serve | `GET /health` | ok | 1 |
| serve | `GET /hardware` | ok | 1 |
| serve | `GET /openapi.json` | ok | 1 |
| serve | `GET /.well-known/agent.json` | ok | 0 |
| serve | `GET /webgpu` | ok | 0 |
| serve | `GET /cluster` | ok | 1 |
| serve | `GET /v1/models` | ok | 109 |
| serve | `GET /v1/fx/status` | ok | 1 |
| serve | `GET /claw/status` | ok | 1 |
| serve | `POST /mcp initialize` | ok | 1 |
| serve | `POST /mcp tools/list` | ok | 1 |
| serve | `POST /acp` | ok | 1 |
| serve | `POST /a2a` | ok | 0 |
| serve | `POST /graphql` | ok | 1 |
| serve | `POST /cluster/route` | ok | 1 |
| serve | `POST /v1/chat/completions` | ok | 1 |
| serve | `POST /mcp malformed` | ok | 0 |
| serve | `GET /no-such-route` | ok | 0 |
| serve | `curl /health` | ok | 10 |
| serve | `stop` | ok | 0 |
| mcp | `initialize` | ok | 38 |
| mcp | `tools/list` | ok | 0 |
| mcp | `ping` | ok | 0 |
| mcp | `unknown method` | ok | 0 |
| mcp | `fw_status` | ok | 95 |
| mcp | `fw_ast_parse` | ok | 47 |
| mcp | `fw_pe_inspect` | ok | 2 |
| mcp | `fw_target_scan` | ok | 78 |
| mcp | `fw_upgrade` | ok | 67 |
| mcp | `fw_bench` | ok | 2 |
| mcp | `fw_learn` | ok | 9 |
| mcp | `fw_service` | ok | 1 |
| mcp | `fw_bridge` | ok | 1 |
| mcp | `fw_disasm` | ok | 3 |
| mcp | `fw_vfs` | ok | 0 |
| mcp | `fw_ai` | ok | 0 |
| mcp | `fw_tui_terminal` | ok | 1 |
| mcp | `unknown tool` | ok | 0 |
| hooks | `session-start agy` | ok | 21 |
| hooks | `session-start claude` | ok | 20 |
| hooks | `auto-update (fresh stamp)` | ok | 24 |
| hooks | `pre-tool-use npm install (rewrite)` | ok | 28 |
| hooks | `pre-tool-use ls (passthrough)` | ok | 25 |
| hooks | `pre-tool-use non-shell tool` | ok | 21 |
| hooks | `pre-tool-use empty stdin` | ok | 23 |
| hooks | `pre-tool-use malformed stdin` | ok | 21 |
| hooks | `pre-tool-use agy run_command` | ok | 24 |
| hooks | `pre-tool-use gemini` | ok | 24 |
| hooks | `auto-update (stale stamp)` | ok | 24 |
| hooks | `hooks.json SessionStart` | ok | 31 |
| hooks | `hooks.json PreToolUse[Bash]` | ok | 32 |
| hooks | `rewrite.ts + common.ts` | ok | 16 |
| upgrade | `fw upgrade --check` | ok | 155 |
| upgrade | `fw --offline upgrade --check` | ok | 4 |
| upgrade | `fw update --check --version no-such-tag-e2e` | ok | 20 |
| upgrade | `<temp>/fw upgrade` | ok | 190 |
| upgrade | `<temp>/fw help` | ok | 2 |
