# Rust stack: tools and crates for fw

Generated from `rust-stack.toml` by `bun configs/claude/scripts/rust-stack.ts`; `--check` gates drift in CI. `bun configs/claude/scripts/rust-stack.ts refresh` refills the live columns. fw status: used = declared in a Cargo.toml or wired into the repo; adopt = measured or queued with a wire-in point; reject = evaluated, reason given.

<!-- rust-stack:start -->

Catalog of 272 entries (61 tools, components and actions, 211 crates): used 107, adopt 20, reject 145.

Sources read live:

- blessed.rs: https://blessed.rs/crates (2026-11-21, nicoburns/blessed-rs@6e317b7, 155 crates)
- awesome-rust: https://github.com/rust-unofficial/awesome-rust (2026-11-21, rust-unofficial/awesome-rust@98131d8)
- lib.rs categories: https://lib.rs/ (2026-11-21, 17 category pages)
- crates.io API: https://crates.io/api/v1/crates (2026-11-21, version, downloads, recent_downloads, release date)
- GitHub API: https://api.github.com/ (2026-11-21, releases and tags of non-crates.io tools)
- rustup components: https://rust-lang.github.io/rustup/concepts/components.html (2026-11-21, rustfmt, clippy, rust-analyzer, miri, llvm-tools)

### test

| name | kind | version | downloads | seen in | fw | purpose and reason |
|---|---|---|---|---|---|---|
| [cargo-nextest](https://crates.io/crates/cargo-nextest) | tool | 0.9.152 | 14.0M | blessed.rs, awesome-rust | used | parallel test runner, per-test process, JUnit and retries: rust-audit test step runs the workspace under nextest; the baseline run of 2026-11-21 was killed by SIGTERM (rc 143) under fr1 load, so no wall-time comparison with cargo test is recorded |
| [cargo-mutants](https://crates.io/crates/cargo-mutants) | tool | 27.1.0 | 996k | awesome-rust | adopt | mutation testing of the test suite: run on demand |
| [cargo-llvm-cov](https://crates.io/crates/cargo-llvm-cov) | tool | 0.9.2 | 9.4M |  | adopt | LLVM source based coverage: already installed on the PC |
| [cargo-miri](https://crates.io/crates/cargo-miri) | tool | 0.0.0 | 5k |  | adopt | UB checker for unsafe code (rustup component miri): already installed on the PC |
| [cargo-hack](https://crates.io/crates/cargo-hack) | tool | 0.6.46 | 2.0M |  | adopt | feature powerset and per-crate check: checks features individually |
| [cargo-insta](https://crates.io/crates/cargo-insta) | tool | 1.49.0 | 1.7M |  | reject | review UI for insta snapshots: fw has no insta snapshots |
| [cargo-tarpaulin](https://crates.io/crates/cargo-tarpaulin) | tool | 0.37.6 | 3.2M | awesome-rust | reject | coverage via ptrace: cargo-llvm-cov is cross-platform and exact |
| [cargo-fuzz](https://crates.io/crates/cargo-fuzz) | tool | 0.13.2 | 6.7M |  | reject | libFuzzer harness: needs nightly; fw parsers are covered by proptest |
| [miri](https://github.com/rust-lang/miri) | component |  |  |  | adopt | MIR interpreter, nightly only: run on demand on unsafe code, nightly only |
| [insta](https://crates.io/crates/insta) | crate | 1.49.0 | 133.5M | blessed.rs, awesome-rust | reject | snapshot tests: assertions are explicit; cargo-insta rejected too |
| [proptest](https://crates.io/crates/proptest) | crate | 1.12.0 | 255.1M | awesome-rust | used | property tests: declared by 1 manifest |
| [quickcheck](https://crates.io/crates/quickcheck) | crate | 1.1.0 | 81.3M | awesome-rust | reject | property tests: proptest is used |
| [rstest](https://crates.io/crates/rstest) | crate | 0.28.0 | 154.4M | awesome-rust | reject | parametrized tests: plain loops are used |
| [pretty_assertions](https://crates.io/crates/pretty_assertions) | crate | 1.4.1 | 242.8M |  | used | readable assert_eq diffs: declared by 8 manifests |
| [tempfile](https://crates.io/crates/tempfile) | crate | 3.27.0 | 1.0B | blessed.rs, awesome-rust | used | temporary files: declared by 14 manifests |
| [mockall](https://crates.io/crates/mockall) | crate | 0.15.0 | 196.5M | awesome-rust | reject | mock generation: traits are faked by hand |
| [wiremock](https://crates.io/crates/wiremock) | crate | 0.6.5 | 109.0M |  | reject | HTTP mock server: tests bind a real axum server |
| [httpmock](https://crates.io/crates/httpmock) | crate | 0.9.0 | 34.9M | awesome-rust | reject | HTTP mock server: same as wiremock |
| [assert_cmd](https://crates.io/crates/assert_cmd) | crate | 2.2.2 | 100.3M |  | adopt | run and assert CLI binaries: queued for fw-bin integration tests; the CLI is checked end to end from Bun scripts today |
| [trybuild](https://crates.io/crates/trybuild) | crate | 1.0.125 | 83.6M |  | reject | compile-fail tests: no proc macros of our own |

### build-speed

| name | kind | version | downloads | seen in | fw | purpose and reason |
|---|---|---|---|---|---|---|
| [cargo-hakari](https://crates.io/crates/cargo-hakari) | tool | 0.9.39 | 1.2M |  | reject | workspace-hack crate that unifies features across the workspace: unit-graph check: switching from -p fw-bin to --workspace recompiles 14 of fw-bin's 55 external units, while the workspace-hack crate would carry 159 dependency lines and touch every one of 39 manifests; release and CI builds do not alternate subsets |
| [sccache](https://github.com/mozilla/sccache) | tool | v0.19.0 |  | awesome-rust | reject | shared compiler cache for rustc and cc: clean release fw-bin build on fr1: 188.6 s cold against 187.7 s warm (cache hits 25 to 91, cpu 70.6 s to 75.4 s); the time is fw-bin fat-LTO codegen, which sccache cannot cache; the host-level sccache the fleet sets stays for dev and check builds |
| [cargo-chef](https://crates.io/crates/cargo-chef) | tool | 0.1.78 | 5.3M | awesome-rust | reject | dependency layer caching for Docker builds: the Dockerfile builds one binary; Swatinem/rust-cache covers CI |
| [rustc_codegen_cranelift](https://github.com/rust-lang/rustc_codegen_cranelift) | component | v0.1.0 |  |  | reject | Cranelift backend for dev builds, nightly only: toolchain is pinned to stable 1.100.0 |
| [nativelink](https://github.com/TraceMachina/nativelink) | tool | v1.8.7 |  | awesome-rust | reject | remote cache and remote execution server speaking the Bazel REAPI, running on fr1: cargo has no REAPI client, only Bazel rules_rust or Buck2 reindeer would use it, which would duplicate Cargo.lock; its single worker on fr1 advertises cpu_count from nproc (the same 12 CPUs as a local build), so remote execution adds no cores, and the host-level sccache already shares compiled crates across clones |

### supply-chain

| name | kind | version | downloads | seen in | fw | purpose and reason |
|---|---|---|---|---|---|---|
| [cargo-deny](https://crates.io/crates/cargo-deny) | tool | 0.22.0 | 7.8M | blessed.rs, awesome-rust | used | advisories, licenses, bans and sources gate driven by deny.toml: deny.toml gate: advisories ok, bans ok, licenses ok, sources ok; first run found 26 errors, 13 advisories ignored with a reason each |
| [cargo-auditable](https://crates.io/crates/cargo-auditable) | tool | 0.7.7 | 1.5M | awesome-rust | adopt | embeds the dependency list in the binary for cargo audit bin: installed on fr1; the byte cost of the embedded dependency list is not measured yet |
| [cargo-vet](https://crates.io/crates/cargo-vet) | tool | 0.10.3 | 751k |  | reject | audit ledger for third-party crates: 1100+ crates in the graph, no audit ledger to import; cargo-deny covers advisories and licenses |
| [cargo-audit](https://crates.io/crates/cargo-audit) | tool | 0.23.0 | 15.4M | blessed.rs, awesome-rust | used | RustSec advisory scan of Cargo.lock: baseline 11 vulnerabilities and 18 warnings on Cargo.lock; the 11 are the deny.toml ignores with reasons |
| [cargo-geiger](https://crates.io/crates/cargo-geiger) | tool | 0.14.0 | 280k | awesome-rust | reject | counts unsafe usage per dependency: output is informational, no gate threshold; miri covers fw own unsafe |
| [cargo-license](https://crates.io/crates/cargo-license) | tool | 0.7.0 | 2.1M | blessed.rs, awesome-rust | reject | lists dependency licenses: cargo deny check licenses covers it |

### deps-hygiene

| name | kind | version | downloads | seen in | fw | purpose and reason |
|---|---|---|---|---|---|---|
| [cargo-udeps](https://crates.io/crates/cargo-udeps) | tool | 0.1.61 | 1.5M | awesome-rust | used | unused dependencies from a real compile, needs nightly: rust-audit unused step on nightly; the baseline run exited 101 after 1332 s, so machete and shear are the gates |
| [cargo-machete](https://crates.io/crates/cargo-machete) | tool | 0.10.1 | 3.4M | awesome-rust | used | fast regex scan for unused dependencies: CI hygiene job over the workspace members; baseline flagged 22 lines repo-wide: 7 removed from workspace members, pbjson ignored in fw-a2a (OUT_DIR code), the other 14 sit in fw-edge and os/* outside the workspace |
| [cargo-shear](https://crates.io/crates/cargo-shear) | tool | 1.14.1 | 553k |  | used | AST scan for unused and misplaced dependencies, can --fix: CI hygiene job; baseline 4 errors (fw-proto 3, fw-drive 1), now 0 errors |
| [cargo-edit](https://crates.io/crates/cargo-edit) | tool | 0.13.14 | 4.1M | blessed.rs, awesome-rust | adopt | cargo upgrade and cargo set-version, bumps dependency requirements: run on demand |
| [cargo-outdated](https://crates.io/crates/cargo-outdated) | tool | 0.19.0 | 1.0M | blessed.rs, awesome-rust | reject | lists outdated dependencies: cargo upgrade --dry-run and cargo update --dry-run cover it |
| [cargo-sort](https://crates.io/crates/cargo-sort) | tool | 2.1.5 | 2.7M |  | reject | sorts Cargo.toml sections: taplo and cargo-shear cover; style only |
| [taplo-cli](https://crates.io/crates/taplo-cli) | tool | 0.10.0 | 2.4M |  | reject | TOML formatter and linter: style only |
| [typos-cli](https://crates.io/crates/typos-cli) | tool | 1.52.2 | 1.1M | awesome-rust | reject | source spell checker: docs are French and English, high false positives |

### size

| name | kind | version | downloads | seen in | fw | purpose and reason |
|---|---|---|---|---|---|---|
| [cargo-bloat](https://crates.io/crates/cargo-bloat) | tool | 0.12.1 | 447k |  | used | binary size per crate and per function: baseline fw-bin release .text: std 21.7%, fw 13.2%, rustls 10.0%, ring 7.7%, fw_apm 6.0% |
| [cargo-llvm-lines](https://crates.io/crates/cargo-llvm-lines) | tool | 0.4.48 | 156k |  | used | LLVM IR lines per generic function, finds monomorphization bloat: rust-audit bloat step; the baseline parse of its table failed and is recorded as unknown |
| [twiggy](https://crates.io/crates/twiggy) | tool | 0.8.0 | 102k |  | reject | wasm code size profiler: wasm-opt size pass covers fw-wasm |

### release

| name | kind | version | downloads | seen in | fw | purpose and reason |
|---|---|---|---|---|---|---|
| [cargo-semver-checks](https://crates.io/crates/cargo-semver-checks) | tool | 0.51.0 | 751k | blessed.rs, awesome-rust | reject | semver lint of a crate against its previous release: fw crates are not published on crates.io (fw-wasm lookup returns not found), so there is no baseline to diff |
| [cargo-msrv](https://crates.io/crates/cargo-msrv) | tool | 0.19.6 | 538k |  | adopt | finds the minimal supported Rust version: run on demand |
| [cargo-about](https://crates.io/crates/cargo-about) | tool | 0.9.3 | 1.5M |  | reject | license attribution generator: THIRD_PARTY_NOTICES.md is maintained already |
| [cargo-dist](https://crates.io/crates/cargo-dist) | tool | 0.34.0 | 192k |  | reject | release packaging and installers: release.yml already ships fw artifacts |
| [cargo-release](https://crates.io/crates/cargo-release) | tool | 1.1.7 | 555k | blessed.rs, awesome-rust | reject | version bump, tag and publish: workspace version is bumped by xtask ship |
| [release-plz](https://crates.io/crates/release-plz) | tool | 0.4.3 | 367k | awesome-rust | reject | release PR automation from conventional commits: xtask ship owns the release flow |
| [cargo-diet](https://crates.io/crates/cargo-diet) | tool | 1.4.2 | 26k |  | reject | trims crate package size: cargo package gate in CI verifies the tarballs |
| [cargo-public-api](https://crates.io/crates/cargo-public-api) | tool | 0.52.0 | 425k |  | reject | public API diff via rustdoc json: cargo-semver-checks covers it |

### install

| name | kind | version | downloads | seen in | fw | purpose and reason |
|---|---|---|---|---|---|---|
| [cargo-binstall](https://crates.io/crates/cargo-binstall) | tool | 1.27.0 | 3.8M | awesome-rust | adopt | installs prebuilt cargo tools instead of compiling them: installs prebuilt cargo tools on fr1 (about 35 tools this session) without compiling them |
| [rustup](https://github.com/rust-lang/rustup) | tool | 1.29.3 |  | blessed.rs, awesome-rust | used | toolchain manager: rust-toolchain.toml pins 1.100.0 |

### debug

| name | kind | version | downloads | seen in | fw | purpose and reason |
|---|---|---|---|---|---|---|
| [cargo-expand](https://crates.io/crates/cargo-expand) | tool | 1.0.127 | 3.9M | blessed.rs, awesome-rust | adopt | prints macro-expanded source: run on demand |

### profile

| name | kind | version | downloads | seen in | fw | purpose and reason |
|---|---|---|---|---|---|---|
| [flamegraph](https://crates.io/crates/flamegraph) | tool | 0.6.15 | 1.0M |  | adopt | cargo flamegraph, perf-based flame graphs: run on demand |
| [samply](https://crates.io/crates/samply) | tool | 0.13.1 | 147k |  | adopt | sampling profiler with Firefox Profiler UI: run on demand |
| [cargo-pgo](https://crates.io/crates/cargo-pgo) | tool | 0.3.0 | 81k |  | reject | PGO and BOLT builds: fw is a short-lived CLI with no single representative workload; needs llvm-tools and a profile corpus per release |
| [cargo-wizard](https://crates.io/crates/cargo-wizard) | tool | 0.2.3 | 11k |  | adopt | suggests Cargo profile templates for compile time or runtime: installed on fr1 as a cross-check for profile choices; not run |
| [cargo-show-asm](https://crates.io/crates/cargo-show-asm) | tool | 0.2.65 | 202k | blessed.rs | adopt | shows assembly or LLVM IR of one function: run on demand |
| [llvm-tools](https://doc.rust-lang.org/rustc/instrument-coverage.html) | component |  |  |  | reject | llvm-profdata and friends for PGO and coverage: cargo-llvm-cov installs it on demand |

### link

| name | kind | version | downloads | seen in | fw | purpose and reason |
|---|---|---|---|---|---|---|
| [mold](https://github.com/rui314/mold) | tool | v3.0.0 |  | awesome-rust | adopt | fast ELF linker: dev relink of fw-bin on fr1: 1.19 s CPU against 1.57 s with the default rust-lld (5 relinks each); a host-level setting, not repo config, since CI runners do not ship it |
| [wild-linker](https://github.com/davidlattimore/wild) | tool | 0.11.0 |  |  | adopt | Rust ELF linker, incremental linking: dev relink of fw-bin on fr1: 1.16 s CPU against 1.57 s with rust-lld (5 relinks each), wall 1.66 s against 3.03 s; host-level setting like mold |

### cross

| name | kind | version | downloads | seen in | fw | purpose and reason |
|---|---|---|---|---|---|---|
| [cargo-zigbuild](https://crates.io/crates/cargo-zigbuild) | tool | 0.24.1 | 8.7M | blessed.rs | reject | cross compile with zig cc as linker and libc selector: CI builds musl and wasm with plain rustup targets and musl-tools |
| [cross](https://crates.io/crates/cross) | tool | 0.2.5 | 6.6M | blessed.rs | reject | docker-based cross compilation: needs docker on the builder and CI already uses native runners per OS |
| [cargo-xwin](https://crates.io/crates/cargo-xwin) | tool | 0.23.1 | 5.4M |  | reject | cross-compile to MSVC from Linux: CI builds on windows-latest natively |

### bench

| name | kind | version | downloads | seen in | fw | purpose and reason |
|---|---|---|---|---|---|---|
| [hyperfine](https://crates.io/crates/hyperfine) | tool | 2.0.0 | 789k | blessed.rs, awesome-rust | used | command-line benchmarking: rust-audit startup and profile steps time the release binary |
| [criterion](https://crates.io/crates/criterion) | crate | 0.8.2 | 348.1M | blessed.rs | used | statistical benchmarks: declared by 3 manifests |
| [divan](https://crates.io/crates/divan) | crate | 0.1.21 | 11.5M | blessed.rs, awesome-rust | reject | attribute-based benchmarks: criterion benches exist and run on stable |
| [gungraun](https://crates.io/crates/gungraun) | crate | 0.20.0 | 1.9M | blessed.rs | reject | valgrind instruction-count benchmarks: criterion already runs the benches; a second harness adds no signal |
| [iai-callgrind](https://crates.io/crates/iai-callgrind) | crate | 0.16.1 | 2.2M |  | reject | valgrind instruction-count benchmarks: renamed to gungraun |
| [dhat](https://crates.io/crates/dhat) | crate | 0.3.3 | 16.3M | blessed.rs | reject | heap profiler: samply and bloat cover; no allocation hot spot |
| [hotpath](https://crates.io/crates/hotpath) | crate | 0.32.1 | 1.6M | blessed.rs, awesome-rust | reject | runtime hot path profiler: samply covers |

### dev-loop

| name | kind | version | downloads | seen in | fw | purpose and reason |
|---|---|---|---|---|---|---|
| [cargo-watch](https://crates.io/crates/cargo-watch) | tool | 8.5.3 | 2.1M | awesome-rust | reject | rerun on file change: no watcher is wired; the dev loop is rust-analyzer checks and nextest |
| [bacon](https://crates.io/crates/bacon) | tool | 3.26.0 | 349k | awesome-rust | reject | background checker TUI: editor and rust-analyzer already run checks |

### lint

| name | kind | version | downloads | seen in | fw | purpose and reason |
|---|---|---|---|---|---|---|
| [rustfmt](https://rust-lang.github.io/rustup/concepts/components.html) | component |  |  | blessed.rs, awesome-rust | used | formatter, rustup component: pinned in rust-toolchain.toml |
| [clippy](https://github.com/rust-lang/rust-clippy) | component | v0.0.212 |  | blessed.rs, awesome-rust | used | lints, workspace perf group denied: CI runs clippy -D warnings |

### ide

| name | kind | version | downloads | seen in | fw | purpose and reason |
|---|---|---|---|---|---|---|
| [rust-analyzer](https://github.com/rust-lang/rust-analyzer) | component | 2026-11-16 |  | awesome-rust | adopt | language server: installed on the PC and fr1 (rustup show), not listed in rust-toolchain.toml components |

### ci

| name | kind | version | downloads | seen in | fw | purpose and reason |
|---|---|---|---|---|---|---|
| [Swatinem/rust-cache](https://github.com/Swatinem/rust-cache) | action | v2.9.3 |  |  | used | target and registry cache for GitHub Actions: every ci.yml job |
| [taiki-e/install-action](https://github.com/taiki-e/install-action) | action | v2.90.6 |  |  | adopt | installs prebuilt tools in CI: installs deny, machete and shear in the hygiene job |

### async

| name | kind | version | downloads | seen in | fw | purpose and reason |
|---|---|---|---|---|---|---|
| [tokio](https://crates.io/crates/tokio) | crate | 1.54.1 | 1.2B | blessed.rs, awesome-rust | used | async runtime: de facto standard, axum, hyper, tonic and reqwest require it |
| [smol](https://crates.io/crates/smol) | crate | 2.0.2 | 29.5M | blessed.rs | reject | small async runtime: the HTTP stack is tokio only |
| [glommio](https://crates.io/crates/glommio) | crate | 0.9.0 | 180k | blessed.rs | reject | io_uring thread-per-core runtime: Linux only, fw targets Windows, macOS and Linux |
| [monoio](https://crates.io/crates/monoio) | crate | 0.2.4 | 1.8M |  | reject | io_uring thread-per-core runtime: Linux only |
| [futures](https://crates.io/crates/futures) | crate | 0.3.34 | 961.9M | blessed.rs | used | combinators and traits: declared by 7 manifests |
| [futures-util](https://crates.io/crates/futures-util) | crate | 0.3.34 | 1.2B |  | used | stream and future utilities: declared by 7 manifests |
| [tokio-stream](https://crates.io/crates/tokio-stream) | crate | 0.1.19 | 600.4M |  | used | stream adapters for tokio: declared by 1 manifest |
| [async-trait](https://crates.io/crates/async-trait) | crate | 0.1.92 | 837.4M | blessed.rs | used | async fn in dyn traits: dyn dispatch still needs boxing |
| [pin-project-lite](https://crates.io/crates/pin-project-lite) | crate | 0.2.18 | 1.3B |  | reject | pin projection without a proc macro: reached transitively (cargo tree -i shows the path); no fw code calls it directly |
| [tokio-util](https://crates.io/crates/tokio-util) | crate | 0.7.20 | 979.9M |  | reject | codecs and cancellation token: reached transitively (cargo tree -i shows the path); no fw code calls it directly |

### http

| name | kind | version | downloads | seen in | fw | purpose and reason |
|---|---|---|---|---|---|---|
| [reqwest](https://crates.io/crates/reqwest) | crate | 0.13.5 | 940.5M | blessed.rs, awesome-rust | used | HTTP client with HTTP/3: optional client feature of fw-http, with h2 and h3 |
| [ureq](https://crates.io/crates/ureq) | crate | 3.5.2 | 276.6M | blessed.rs | used | blocking HTTP client, rustls: small blocking client for fw-bin |
| [hyper](https://crates.io/crates/hyper) | crate | 1.12.0 | 1.2B | blessed.rs, awesome-rust | used | HTTP implementation: declared by 4 manifests |
| [hyper-util](https://crates.io/crates/hyper-util) | crate | 0.1.21 | 660.2M |  | used | hyper client and server helpers: declared by 2 manifests |
| [axum](https://crates.io/crates/axum) | crate | 0.8.9 | 610.5M | blessed.rs, awesome-rust | used | HTTP server framework: declared by 9 manifests |
| [tower](https://crates.io/crates/tower) | crate | 0.5.3 | 866.4M |  | used | service abstraction: declared by 4 manifests |
| [http](https://crates.io/crates/http) | crate | 1.5.0 | 1.3B | blessed.rs, awesome-rust | used | HTTP types: declared by 4 manifests |
| [http-body-util](https://crates.io/crates/http-body-util) | crate | 0.1.5 | 643.9M |  | used | body utilities: declared by 5 manifests |
| [actix-web](https://crates.io/crates/actix-web) | crate | 4.20.0 | 92.0M | blessed.rs, awesome-rust | reject | HTTP server framework: axum already shares tower and hyper with the rest of the stack |
| [h3](https://crates.io/crates/h3) | crate | 0.0.8 | 6.0M |  | used | HTTP/3 on quinn, git fork patch: master carries fixes 0.0.8 lacks |
| [quinn](https://crates.io/crates/quinn) | crate | 0.11.12 | 436.9M | awesome-rust | used | QUIC: declared by 1 manifest |
| [tonic](https://crates.io/crates/tonic) | crate | 0.14.9 | 485.8M | blessed.rs, awesome-rust | used | gRPC: declared by 2 manifests |

### tls

| name | kind | version | downloads | seen in | fw | purpose and reason |
|---|---|---|---|---|---|---|
| [rustls](https://crates.io/crates/rustls) | crate | 0.23.46 | 1.2B | awesome-rust | used | TLS in Rust: declared by 10 manifests |
| [tokio-rustls](https://crates.io/crates/tokio-rustls) | crate | 0.26.6 | 970.0M |  | used | rustls for tokio: declared by 3 manifests |
| [rustls-native-certs](https://crates.io/crates/rustls-native-certs) | crate | 0.8.5 | 707.2M |  | used | OS trust store: declared by 1 manifest |
| [webpki-roots](https://crates.io/crates/webpki-roots) | crate | 1.0.12 | 955.3M |  | used | Mozilla root set: declared by 1 manifest |
| [rcgen](https://crates.io/crates/rcgen) | crate | 0.14.10 | 154.2M |  | used | certificate generation: declared by 3 manifests |
| [aws-lc-rs](https://crates.io/crates/aws-lc-rs) | crate | 1.19.0 | 327.1M |  | reject | crypto provider for rustls: rustls selects its crypto provider; no direct use |
| [native-tls](https://crates.io/crates/native-tls) | crate | 0.2.18 | 393.7M |  | reject | OS TLS binding: rustls everywhere keeps one TLS stack |

### serde

| name | kind | version | downloads | seen in | fw | purpose and reason |
|---|---|---|---|---|---|---|
| [serde](https://crates.io/crates/serde) | crate | 1.0.229 | 1.8B | blessed.rs, awesome-rust | used | serialization framework: declared by 37 manifests |
| [serde_json](https://crates.io/crates/serde_json) | crate | 1.0.154 | 1.7B | blessed.rs, awesome-rust | used | JSON: declared by 30 manifests |
| [toml](https://crates.io/crates/toml) | crate | 1.2.1+spec-1.1.0 | 1.2B | blessed.rs, awesome-rust | used | TOML: declared by 8 manifests |
| [prost](https://crates.io/crates/prost) | crate | 0.14.6 | 730.0M | blessed.rs, awesome-rust | used | protobuf: declared by 3 manifests |
| [schemars](https://crates.io/crates/schemars) | crate | 1.2.3 | 677.1M |  | used | JSON schema from types: declared by 1 manifest |
| [serde-saphyr](https://crates.io/crates/serde-saphyr) | crate | 2.2.0 | 21.7M | awesome-rust | used | YAML on serde: serde_yaml is archived upstream (verified with gh api) |
| [postcard](https://crates.io/crates/postcard) | crate | 1.1.3 | 99.0M | blessed.rs, awesome-rust | reject | compact binary serde format: no on-disk or wire format in fw needs it |
| [rkyv](https://crates.io/crates/rkyv) | crate | 0.8.18 | 188.8M | blessed.rs, awesome-rust | reject | zero-copy deserialization: no hot deserialization path in the profile |
| [bincode](https://crates.io/crates/bincode) | crate | 3.0.0 | 385.7M | awesome-rust | reject | binary serde format: no binary serde format is in use |
| [simd-json](https://crates.io/crates/simd-json) | crate | 0.19.0 | 26.0M | awesome-rust | reject | SIMD JSON parser: JSON payloads are small, serde_json is not on a profile |
| [sonic-rs](https://crates.io/crates/sonic-rs) | crate | 0.5.12 | 9.0M | awesome-rust | reject | SIMD JSON parser: same as simd-json |
| [rmp-serde](https://crates.io/crates/rmp-serde) | crate | 1.3.1 | 156.8M |  | reject | MessagePack serde: no consumer |
| [ciborium](https://crates.io/crates/ciborium) | crate | 0.2.2 | 308.1M |  | reject | CBOR serde: no consumer |
| [quick-xml](https://crates.io/crates/quick-xml) | crate | 0.42.0 | 567.3M | awesome-rust | reject | XML parser: no XML input in fw |

### error

| name | kind | version | downloads | seen in | fw | purpose and reason |
|---|---|---|---|---|---|---|
| [anyhow](https://crates.io/crates/anyhow) | crate | 1.0.104 | 1.2B | blessed.rs | used | application errors: declared by 4 manifests |
| [thiserror](https://crates.io/crates/thiserror) | crate | 2.0.21 | 1.9B | blessed.rs | used | library error derive: declared by 15 manifests |
| [color-eyre](https://crates.io/crates/color-eyre) | crate | 0.6.5 | 80.7M | blessed.rs | reject | colored application errors: anyhow is used in all binaries, one error type |
| [miette](https://crates.io/crates/miette) | crate | 7.6.0 | 96.3M |  | reject | diagnostic errors with source spans: no span-based diagnostics in fw |
| [snafu](https://crates.io/crates/snafu) | crate | 0.9.2 | 132.2M |  | reject | context-selector error derive: thiserror is used |

### cli

| name | kind | version | downloads | seen in | fw | purpose and reason |
|---|---|---|---|---|---|---|
| [clap](https://crates.io/crates/clap) | crate | 4.6.9 | 1.4B | blessed.rs, awesome-rust | used | argument parser: declared by 7 manifests |
| [argh](https://crates.io/crates/argh) | crate | 0.1.19 | 18.6M | awesome-rust | used | small derive argument parser: used by fw-ring3-agent |
| [lexopt](https://crates.io/crates/lexopt) | crate | 0.3.2 | 15.5M | blessed.rs | reject | minimal parser: fw-bin has its own parser in cli_parser.rs |
| [pico-args](https://crates.io/crates/pico-args) | crate | 0.5.0 | 98.1M | blessed.rs | reject | minimal parser: same as lexopt |
| [bpaf](https://crates.io/crates/bpaf) | crate | 0.9.29 | 10.2M |  | reject | combinator parser: clap is used |
| [clap_complete](https://crates.io/crates/clap_complete) | crate | 4.6.12 | 144.7M |  | reject | shell completions for clap: fw-bin uses its own parser (cli_parser.rs), so there is no clap completion to generate |
| [indicatif](https://crates.io/crates/indicatif) | crate | 0.18.6 | 261.1M | blessed.rs, awesome-rust | reject | progress bars: fw prints plain lines for agents |
| [ratatui](https://crates.io/crates/ratatui) | crate | 0.30.2 | 73.7M | blessed.rs, awesome-rust | reject | TUI framework: fw ui uses blitz and gpui in the upper layers |
| [crossterm](https://crates.io/crates/crossterm) | crate | 0.29.0 | 239.0M | blessed.rs, awesome-rust | reject | terminal control: no TUI in this repo |
| [inquire](https://crates.io/crates/inquire) | crate | 0.9.5 | 25.3M | blessed.rs, awesome-rust | reject | interactive prompts: agents run non-interactive |

### log

| name | kind | version | downloads | seen in | fw | purpose and reason |
|---|---|---|---|---|---|---|
| [tracing](https://crates.io/crates/tracing) | crate | 0.1.44 | 1.1B | blessed.rs, awesome-rust | used | structured instrumentation: declared by 11 manifests |
| [tracing-subscriber](https://crates.io/crates/tracing-subscriber) | crate | 0.3.23 | 768.5M |  | used | subscriber and filters: declared by 1 manifest |
| [tracing-appender](https://crates.io/crates/tracing-appender) | crate | 0.2.5 | 153.8M |  | reject | non-blocking file appender: logs go to stderr |
| [tracing-opentelemetry](https://crates.io/crates/tracing-opentelemetry) | crate | 0.34.0 | 258.6M |  | reject | OpenTelemetry bridge: no collector in fw |
| [log](https://crates.io/crates/log) | crate | 0.4.35 | 1.6B | blessed.rs, awesome-rust | reject | log facade: tracing replaces it everywhere |
| [env_logger](https://crates.io/crates/env_logger) | crate | 0.11.12 | 637.5M |  | reject | log backend: tracing-subscriber replaces it |
| [slog](https://crates.io/crates/slog) | crate | 2.8.2 | 83.7M | blessed.rs, awesome-rust | reject | structured logger: tracing replaces it |

### alloc

| name | kind | version | downloads | seen in | fw | purpose and reason |
|---|---|---|---|---|---|---|
| [mimalloc](https://crates.io/crates/mimalloc) | crate | 0.1.52 | 79.5M |  | used | global allocator: measured against the system allocator |
| [tikv-jemallocator](https://crates.io/crates/tikv-jemallocator) | crate | 0.7.0 | 117.5M |  | reject | jemalloc global allocator: mimalloc is already linked; one allocator only |
| [snmalloc-rs](https://crates.io/crates/snmalloc-rs) | crate | 0.7.6 | 10.1M |  | reject | snmalloc global allocator: same as jemalloc |
| [bumpalo](https://crates.io/crates/bumpalo) | crate | 3.20.4 | 750.4M |  | reject | bump arena: oxc_allocator is the arena in use |

### hash

| name | kind | version | downloads | seen in | fw | purpose and reason |
|---|---|---|---|---|---|---|
| [foldhash](https://crates.io/crates/foldhash) | crate | 0.2.0 | 866.6M |  | reject | fast default hasher, hashbrown default: reached transitively (cargo tree -i shows the path); no fw code calls it directly |
| [ahash](https://crates.io/crates/ahash) | crate | 0.8.12 | 930.9M |  | reject | AES hasher: foldhash replaces it as hashbrown default |
| [rustc-hash](https://crates.io/crates/rustc-hash) | crate | 2.1.3 | 1.1B |  | reject | FxHash: two versions in the graph; foldhash is faster on strings |
| [blake3](https://crates.io/crates/blake3) | crate | 1.8.7 | 234.3M |  | used | SIMD hash, mmap and rayon: declared by 6 manifests |
| [sha2](https://crates.io/crates/sha2) | crate | 0.11.1 | 1.3B |  | used | SHA-2: declared by 17 manifests |
| [xxhash-rust](https://crates.io/crates/xxhash-rust) | crate | 0.8.21 | 148.2M |  | reject | XXH3 non-cryptographic hash: blake3 is fast enough for content addressing |
| [twox-hash](https://crates.io/crates/twox-hash) | crate | 2.1.5 | 308.4M |  | reject | XXH hash: same as xxhash-rust |
| [siphasher](https://crates.io/crates/siphasher) | crate | 1.0.4 | 719.7M |  | reject | SipHash: std default hasher |
| [hmac](https://crates.io/crates/hmac) | crate | 0.13.0 | 782.1M |  | used | HMAC: declared by 1 manifest |

### collections

| name | kind | version | downloads | seen in | fw | purpose and reason |
|---|---|---|---|---|---|---|
| [smallvec](https://crates.io/crates/smallvec) | crate | 1.16.6 | 1.5B | blessed.rs | reject | inline small vectors: reached transitively (cargo tree -i shows the path); no fw code calls it directly |
| [arrayvec](https://crates.io/crates/arrayvec) | crate | 0.7.8 | 715.6M | blessed.rs | reject | fixed-capacity vector: no fixed-size hot collection |
| [tinyvec](https://crates.io/crates/tinyvec) | crate | 1.13.3 | 929.1M | blessed.rs | reject | safe small vector: smallvec is the one in the graph |
| [hashbrown](https://crates.io/crates/hashbrown) | crate | 0.17.1 | 3.2B |  | reject | std HashMap implementation: std HashMap is hashbrown |
| [indexmap](https://crates.io/crates/indexmap) | crate | 2.14.2 | 1.9B | blessed.rs | reject | insertion-ordered map: transitive only |
| [dashmap](https://crates.io/crates/dashmap) | crate | 6.2.1 | 465.8M | blessed.rs | reject | concurrent hash map: no shared map contention measured |
| [papaya](https://crates.io/crates/papaya) | crate | 0.2.5 | 17.1M | blessed.rs | reject | lock-free map: no shared map contention measured |
| [compact_str](https://crates.io/crates/compact_str) | crate | 0.10.0 | 224.4M |  | reject | small string optimization: no string hot path measured |
| [thin-vec](https://crates.io/crates/thin-vec) | crate | 0.2.21 | 49.6M |  | reject | pointer-sized vector: oxc brings it where needed |
| [slotmap](https://crates.io/crates/slotmap) | crate | 1.1.1 | 138.8M |  | reject | generational arena: petgraph covers graphs |
| [moka](https://crates.io/crates/moka) | crate | 0.12.16 | 167.1M | awesome-rust | reject | concurrent cache: no cache in fw hot path |
| [lru](https://crates.io/crates/lru) | crate | 0.19.1 | 457.5M | awesome-rust | reject | LRU cache: no cache in fw hot path |
| [petgraph](https://crates.io/crates/petgraph) | crate | 0.8.3 | 630.6M | awesome-rust | used | graph structures: declared by 1 manifest |
| [roaring](https://crates.io/crates/roaring) | crate | 0.12.0 | 66.2M |  | reject | compressed bitmaps: no bitmap indexes |

### parse

| name | kind | version | downloads | seen in | fw | purpose and reason |
|---|---|---|---|---|---|---|
| [winnow](https://crates.io/crates/winnow) | crate | 1.0.5 | 1.2B | awesome-rust | reject | parser combinators: hand-written and tree-sitter parsers cover fw |
| [nom](https://crates.io/crates/nom) | crate | 8.0.0 | 919.7M | awesome-rust | reject | parser combinators: two nom versions only transitively |
| [chumsky](https://crates.io/crates/chumsky) | crate | 0.14.2 | 32.5M |  | reject | parser combinators with recovery: no DSL in fw |
| [logos](https://crates.io/crates/logos) | crate | 0.16.1 | 106.7M |  | reject | lexer generator: no custom lexer |
| [pest](https://crates.io/crates/pest) | crate | 2.9.6 | 401.7M | awesome-rust | reject | PEG parser: no grammar file |
| [tree-sitter](https://crates.io/crates/tree-sitter) | crate | 0.27.2 | 58.5M | awesome-rust | used | incremental parsing: declared by 3 manifests |
| [oxc_parser](https://crates.io/crates/oxc_parser) | crate | 0.159.0 | 12.0M |  | used | JS and TS parser: declared by 3 manifests |
| [memchr](https://crates.io/crates/memchr) | crate | 2.8.3 | 1.8B |  | reject | SIMD byte search: reached transitively (cargo tree -i shows the path); no fw code calls it directly |
| [aho-corasick](https://crates.io/crates/aho-corasick) | crate | 1.1.5 | 1.4B |  | reject | multi-pattern search: regex pulls it in |
| [regex](https://crates.io/crates/regex) | crate | 1.13.2 | 1.4B | blessed.rs, awesome-rust | used | regular expressions: declared by 6 manifests |
| [fancy-regex](https://crates.io/crates/fancy-regex) | crate | 0.20.2 | 318.0M | blessed.rs, awesome-rust | reject | backtracking regex: no lookaround need |
| [bstr](https://crates.io/crates/bstr) | crate | 1.14.0 | 522.2M |  | reject | byte string utilities: std covers |

### compress

| name | kind | version | downloads | seen in | fw | purpose and reason |
|---|---|---|---|---|---|---|
| [flate2](https://crates.io/crates/flate2) | crate | 1.1.11 | 877.9M | blessed.rs | used | gzip and deflate: zlib-rs backend measured below |
| [zlib-rs](https://crates.io/crates/zlib-rs) | crate | 0.6.8 | 264.1M |  | adopt | fast pure-Rust zlib, flate2 backend: queued: enable the flate2 zlib-rs feature where tar and zip extraction run; not measured yet |
| [miniz_oxide](https://crates.io/crates/miniz_oxide) | crate | 0.9.1 | 1.2B |  | reject | default flate2 backend: replaced by zlib-rs if measured faster |
| [zstd](https://crates.io/crates/zstd) | crate | 0.14.4 | 494.7M |  | reject | zstd bindings: C build script; no zstd format in fw |
| [lz4_flex](https://crates.io/crates/lz4_flex) | crate | 0.14.2 | 179.4M |  | reject | pure Rust LZ4: no LZ4 format in fw |
| [brotli](https://crates.io/crates/brotli) | crate | 9.1.3 | 341.3M | awesome-rust | reject | brotli codec: HTTP gzip only in ureq |
| [snap](https://crates.io/crates/snap) | crate | 1.1.2 | 142.9M |  | reject | snappy: no snappy format |
| [zip](https://crates.io/crates/zip) | crate | 9.8.1 | 356.2M | awesome-rust | used | zip archives: declared by 1 manifest |
| [tar](https://crates.io/crates/tar) | crate | 0.4.46 | 298.2M |  | used | tar archives: declared by 1 manifest |

### crypto

| name | kind | version | downloads | seen in | fw | purpose and reason |
|---|---|---|---|---|---|---|
| [aes-gcm](https://crates.io/crates/aes-gcm) | crate | 0.11.1 | 221.7M |  | used | AEAD: declared by 2 manifests |
| [argon2](https://crates.io/crates/argon2) | crate | 0.6.0 | 79.7M |  | used | password hashing: declared by 1 manifest |
| [zeroize](https://crates.io/crates/zeroize) | crate | 1.9.2 | 912.7M |  | used | zero memory on drop: declared by 2 manifests |
| [rsa](https://crates.io/crates/rsa) | crate | 0.10.0 | 280.4M |  | used | RSA: declared in a fw manifest; advisories are gated by deny.toml |
| [chacha20poly1305](https://crates.io/crates/chacha20poly1305) | crate | 0.11.0 | 120.5M |  | reject | AEAD: aes-gcm is used |
| [ed25519-dalek](https://crates.io/crates/ed25519-dalek) | crate | 3.0.0 | 284.3M |  | reject | signatures: no signature scheme needed |
| [ring](https://crates.io/crates/ring) | crate | 0.17.14 | 930.1M | awesome-rust | reject | crypto library: rustls selects its crypto provider; no direct use |
| [subtle](https://crates.io/crates/subtle) | crate | 2.6.1 | 900.7M |  | reject | constant-time compare: transitive via RustCrypto |
| [rand](https://crates.io/crates/rand) | crate | 0.10.4 | 2.2B | blessed.rs, awesome-rust | used | random numbers: declared by 3 manifests |
| [getrandom](https://crates.io/crates/getrandom) | crate | 0.4.3 | 2.7B |  | used | OS randomness: declared by 2 manifests |
| [fastrand](https://crates.io/crates/fastrand) | crate | 2.5.0 | 1.2B | blessed.rs | reject | small RNG: rand is already linked |

### simd

| name | kind | version | downloads | seen in | fw | purpose and reason |
|---|---|---|---|---|---|---|
| [wide](https://crates.io/crates/wide) | crate | 1.7.1 | 100.7M |  | reject | portable SIMD on stable: candle and gemm carry the numeric SIMD |
| [pulp](https://crates.io/crates/pulp) | crate | 0.22.3 | 40.2M |  | reject | SIMD multiversioning: gemm uses it internally |
| [simdutf8](https://crates.io/crates/simdutf8) | crate | 0.1.5 | 318.7M |  | reject | SIMD UTF-8 validation: no UTF-8 validation hot path |
| [gemm](https://crates.io/crates/gemm) | crate | 0.19.0 | 25.8M |  | used | matrix multiplication kernels: declared by 1 manifest |
| [ndarray](https://crates.io/crates/ndarray) | crate | 0.17.2 | 151.0M | awesome-rust | used | n-dimensional arrays: declared by 1 manifest |
| [nalgebra](https://crates.io/crates/nalgebra) | crate | 0.35.0 | 104.1M | awesome-rust | reject | linear algebra: ndarray is used |
| [glam](https://crates.io/crates/glam) | crate | 0.34.3 | 196.4M | blessed.rs | reject | 3D math: no 3D math in fw crates |
| [faer](https://crates.io/crates/faer) | crate | 0.24.4 | 6.0M | awesome-rust | reject | linear algebra: no dense solver need |
| [bytemuck](https://crates.io/crates/bytemuck) | crate | 1.25.2 | 492.0M | blessed.rs | reject | safe casts: zerocopy is used |
| [zerocopy](https://crates.io/crates/zerocopy) | crate | 0.8.63 | 1.2B | blessed.rs, awesome-rust | used | safe zero-copy casts: declared by 2 manifests |
| [rayon](https://crates.io/crates/rayon) | crate | 1.12.0 | 685.5M | blessed.rs, awesome-rust | used | data parallelism: declared by 2 manifests |
| [crossbeam-channel](https://crates.io/crates/crossbeam-channel) | crate | 0.5.17 | 724.3M | blessed.rs | reject | channels: tokio and std channels used |
| [flume](https://crates.io/crates/flume) | crate | 0.12.0 | 301.6M | blessed.rs | reject | channels: tokio and std channels used |

### gpu

| name | kind | version | downloads | seen in | fw | purpose and reason |
|---|---|---|---|---|---|---|
| [wgpu](https://crates.io/crates/wgpu) | crate | 30.0.2 | 53.0M | awesome-rust | reject | portable GPU API on WebGPU: no GPU code in fw crates; fw-wayland-wgpu is a protocol layer and the renderer lives in aphrody |
| [naga](https://crates.io/crates/naga) | crate | 30.0.2 | 58.2M |  | reject | shader translator: owned by the wgpu skill and aphrody |
| [vello](https://crates.io/crates/vello) | crate | 0.12.0 | 1.2M | blessed.rs | reject | GPU 2D renderer: renderer lives in aphrody, dependency points down only |

### ml

| name | kind | version | downloads | seen in | fw | purpose and reason |
|---|---|---|---|---|---|---|
| [candle-core](https://crates.io/crates/candle-core) | crate | 0.11.1 | 12.3M | awesome-rust | used | tensor library: declared by 1 manifest |
| [ort](https://crates.io/crates/ort) | crate | 2.0.0-rc.15 | 27.1M |  | used | ONNX Runtime: declared by 1 manifest |
| [tokenizers](https://crates.io/crates/tokenizers) | crate | 0.23.2 | 49.2M | awesome-rust | used | HF tokenizers: declared by 1 manifest |
| [burn](https://crates.io/crates/burn) | crate | 0.22.1 | 2.4M | awesome-rust | reject | deep learning framework: candle already used |
| [tch](https://crates.io/crates/tch) | crate | 0.27.0 | 9.8M |  | reject | libtorch bindings: needs libtorch; candle is already used |
| [safetensors](https://crates.io/crates/safetensors) | crate | 0.8.0 | 38.2M |  | used | tensor file format: declared by 1 manifest |
| [hf-hub](https://crates.io/crates/hf-hub) | crate | 1.0.0 | 25.5M |  | used | Hugging Face Hub client: declared by 3 manifests |

### time

| name | kind | version | downloads | seen in | fw | purpose and reason |
|---|---|---|---|---|---|---|
| [chrono](https://crates.io/crates/chrono) | crate | 0.4.45 | 1.0B | blessed.rs, awesome-rust | used | dates and times: declared by 8 manifests |
| [jiff](https://crates.io/crates/jiff) | crate | 1.0.1 | 295.8M | blessed.rs, awesome-rust | reject | modern dates and times: chrono is used in 7 crates; jiff only transitively |
| [time](https://crates.io/crates/time) | crate | 0.3.55 | 1.1B | blessed.rs, awesome-rust | reject | dates and times: chrono is used |

### os

| name | kind | version | downloads | seen in | fw | purpose and reason |
|---|---|---|---|---|---|---|
| [memmap2](https://crates.io/crates/memmap2) | crate | 0.9.12 | 441.3M | blessed.rs | used | memory maps: declared by 3 manifests |
| [walkdir](https://crates.io/crates/walkdir) | crate | 2.5.1 | 795.8M | blessed.rs | used | directory walk: declared by 3 manifests |
| [ignore](https://crates.io/crates/ignore) | crate | 0.4.34 | 219.2M | blessed.rs | used | gitignore-aware walk: declared by 2 manifests |
| [globset](https://crates.io/crates/globset) | crate | 0.4.21 | 301.2M | blessed.rs | used | glob sets: declared by 1 manifest |
| [notify](https://crates.io/crates/notify) | crate | 9.0.0 | 204.4M | blessed.rs | used | file watching: declared by 1 manifest |
| [dirs](https://crates.io/crates/dirs) | crate | 7.0.0 | 420.6M | blessed.rs | used | user directories: declared by 5 manifests |
| [directories](https://crates.io/crates/directories) | crate | 6.0.0 | 84.2M | blessed.rs | reject | user directories: dirs is used |
| [rustix](https://crates.io/crates/rustix) | crate | 1.1.6 | 1.4B | blessed.rs, awesome-rust | used | syscalls without libc: declared by 1 manifest |
| [nix](https://crates.io/crates/nix) | crate | 0.32.0 | 1.0B | blessed.rs, awesome-rust | reject | POSIX bindings: rustix is used |
| [libc](https://crates.io/crates/libc) | crate | 0.2.191 | 2.0B | blessed.rs | used | C bindings: declared by 5 manifests |
| [windows](https://crates.io/crates/windows) | crate | 0.62.2 | 420.5M | blessed.rs | used | Windows API: declared by 3 manifests |
| [windows-sys](https://crates.io/crates/windows-sys) | crate | 0.61.2 | 2.0B |  | used | raw Windows bindings: declared by 5 manifests |
| [sysinfo](https://crates.io/crates/sysinfo) | crate | 0.40.4 | 253.2M | awesome-rust | used | system information: declared by 1 manifest |
| [which](https://crates.io/crates/which) | crate | 8.0.6 | 488.8M |  | used | PATH lookup: declared by 4 manifests |
| [fs4](https://crates.io/crates/fs4) | crate | 1.1.0 | 83.5M |  | used | file locking: declared by 1 manifest |

### db

| name | kind | version | downloads | seen in | fw | purpose and reason |
|---|---|---|---|---|---|---|
| [rusqlite](https://crates.io/crates/rusqlite) | crate | 0.43.0 | 162.0M | blessed.rs, awesome-rust | used | SQLite: declared by 8 manifests |
| [tokio-postgres](https://crates.io/crates/tokio-postgres) | crate | 0.7.18 | 91.1M | blessed.rs | used | Postgres client: declared by 2 manifests |
| [deadpool-postgres](https://crates.io/crates/deadpool-postgres) | crate | 0.14.2 | 30.1M |  | used | Postgres pool: declared by 1 manifest |
| [sqlx](https://crates.io/crates/sqlx) | crate | 0.9.1 | 195.7M | blessed.rs, awesome-rust | reject | async SQL toolkit: rusqlite and tokio-postgres are used directly |
| [redb](https://crates.io/crates/redb) | crate | 5.0.0 | 16.8M | awesome-rust | reject | pure Rust embedded KV: SQLite used |
| [heed](https://crates.io/crates/heed) | crate | 0.23.1 | 8.1M | blessed.rs, awesome-rust | reject | LMDB wrapper: SQLite used |
| [fjall](https://crates.io/crates/fjall) | crate | 3.1.13 | 2.3M |  | reject | LSM embedded KV: SQLite used |
| [sqlite-vec](https://crates.io/crates/sqlite-vec) | crate | 0.1.9 | 4.3M |  | used | vector search in SQLite: declared by 1 manifest |

### misc

| name | kind | version | downloads | seen in | fw | purpose and reason |
|---|---|---|---|---|---|---|
| [uuid](https://crates.io/crates/uuid) | crate | 1.29.3 | 1.0B | blessed.rs | used | UUIDs: declared by 6 manifests |
| [url](https://crates.io/crates/url) | crate | 2.5.9 | 1.1B |  | used | URL parsing: declared by 4 manifests |
| [bytes](https://crates.io/crates/bytes) | crate | 1.12.2 | 1.3B |  | used | byte buffers: declared by 7 manifests |
| [base64](https://crates.io/crates/base64) | crate | 0.23.1 | 2.0B |  | used | base64: two versions in the graph |
| [hex](https://crates.io/crates/hex) | crate | 0.4.3 | 826.5M | awesome-rust | used | hex: declared by 3 manifests |
| [itertools](https://crates.io/crates/itertools) | crate | 0.15.0 | 1.9B | blessed.rs, awesome-rust | reject | iterator adapters: std iterators suffice; transitive |
| [bitflags](https://crates.io/crates/bitflags) | crate | 2.13.2 | 2.3B | blessed.rs | reject | flag types: enumflags2 is used |
| [parking_lot](https://crates.io/crates/parking_lot) | crate | 0.13.0 | 1.3B | blessed.rs | used | locks: declared by the fw-browser-* crates; std locks stay in the rest of the workspace |
| [arc-swap](https://crates.io/crates/arc-swap) | crate | 1.9.5 | 423.2M | blessed.rs | used | atomic Arc swap: declared by 1 manifest |
| [strum](https://crates.io/crates/strum) | crate | 0.28.0 | 793.5M |  | used | enum derives: declared by 1 manifest |
| [derive_more](https://crates.io/crates/derive_more) | crate | 2.1.1 | 612.4M |  | reject | derive helpers: std derives and thiserror |
| [bon](https://crates.io/crates/bon) | crate | 3.10.3 | 81.3M | awesome-rust | reject | builder derive: no builders |

### wasm

| name | kind | version | downloads | seen in | fw | purpose and reason |
|---|---|---|---|---|---|---|
| [wasm-bindgen](https://crates.io/crates/wasm-bindgen) | crate | 0.2.130 | 678.7M | awesome-rust | used | JS bindings: declared by 1 manifest |
| [web-sys](https://crates.io/crates/web-sys) | crate | 0.3.107 | 486.4M |  | used | web APIs: declared by 1 manifest |
| [js-sys](https://crates.io/crates/js-sys) | crate | 0.3.107 | 631.8M |  | used | JS builtins: declared by 2 manifests |

### ui

| name | kind | version | downloads | seen in | fw | purpose and reason |
|---|---|---|---|---|---|---|
| [gpui](https://crates.io/crates/gpui) | crate | 0.2.2 | 534k | blessed.rs | used | native GPU UI: declared by configs/gemini/dashboard |
| [iced](https://crates.io/crates/iced) | crate | 0.14.0 | 3.4M | blessed.rs, awesome-rust | reject | Elm-style GUI: gpui in aphrody |
| [egui](https://crates.io/crates/egui) | crate | 0.37.0 | 32.6M | blessed.rs, awesome-rust | reject | immediate mode GUI: gpui in aphrody |
| [taffy](https://crates.io/crates/taffy) | crate | 0.15.1 | 21.2M | blessed.rs | reject | flexbox layout: owned by blitz and gpui |
| [resvg](https://crates.io/crates/resvg) | crate | 0.50.0 | 40.8M | awesome-rust | used | SVG rendering: declared by 1 manifest |
| [cosmic-text](https://crates.io/crates/cosmic-text) | crate | 0.19.0 | 12.9M | blessed.rs | reject | text layout: owned by gpui and blitz |

<!-- rust-stack:end -->
