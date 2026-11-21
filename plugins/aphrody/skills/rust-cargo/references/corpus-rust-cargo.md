# Rust, Cargo, cross-compilation and LLVM from the corpus

Distilled from the local corpus on `fr1` (`fw-corpus`, read 2026-11-21). Facts are paraphrased; each cites the URL the corpus stored. Re-query before relying on a detail that may have moved (`fw-corpus search "<words>" --source <id>`).

## Sources

| id | holds | published at |
| --- | --- | --- |
| `cargo` | The Cargo Book (`rust-lang/cargo` `doc/book/src`) | `https://doc.rust-lang.org/cargo/` |
| `rust` | book, reference, rustc book (platform support, target tier policy), rustdoc book, unstable book, style guide, rustc dev guide, nomicon, edition guide | `doc.rust-lang.org`, `rustc-dev-guide.rust-lang.org` |
| `rust-std` | std, core, alloc items from the nightly rustdoc JSON | `https://doc.rust-lang.org/nightly/` |
| `rust-process` | Rust Forge, accepted RFCs, project goals | `forge.rust-lang.org`, `rust-lang.github.io/rfcs`, `rust-lang.github.io/rust-project-goals` |
| `rust-cross` | `cross`, `cargo-zigbuild`, `cargo-xwin`, `xwin` repositories (README, docs, changelogs) | GitHub blob URLs pinned at the crawled commit |
| `llvm` | LLVM, Clang, lld and MLIR docs from `llvm/llvm-project` | `llvm.org/docs`, `clang.llvm.org/docs`, `lld.llvm.org` |

## Resolver versions and feature unification

- `resolver = "1"` is the default; `"2"` is the default of edition 2021 and needs Rust 1.51; `"3"` is the default of edition 2024 and needs Rust 1.84, and only changes the default of `resolver.incompatible-rust-versions` from `allow` to `fallback`. The setting is global: only the top-level package, or `[workspace]` of a virtual workspace, counts. Source: `https://doc.rust-lang.org/cargo/reference/resolver.html#resolver-versions`.
- Resolver 2 stops unifying features for target-specific dependencies of a target that is not being built, for build-dependencies and proc-macros against the same normal dependency, and for dev-dependencies unless they are being built (`cargo test`, examples). Source: `https://doc.rust-lang.org/cargo/reference/resolver.html#feature-resolver-version-2`.

## Targets and tiers

- The rustc book lists targets by tier; tier 1 with host tools includes `aarch64-unknown-linux-gnu` (kernel 4.1+, glibc 2.17+), `x86_64-pc-windows-msvc` (Windows 10+) and `aarch64-apple-darwin`, and says all of them carry the full standard library. Source: `https://doc.rust-lang.org/rustc/platform-support.html#tier-1-with-host-tools`. Adding a target is documented in `https://rustc-dev-guide.rust-lang.org/building/new-target.html`.

## Cross-compiling without a target machine

- `cargo-zigbuild`: install Zig (or `pip3 install ziglang`), `rustup target add <triple>`, run `cargo zigbuild --target <triple>`. For `*-gnu` triples Zig builds against a default glibc that depends on the Zig release; a minimum glibc is chosen by suffixing the triple, for example `--target aarch64-unknown-linux-gnu.2.17`. macOS deployment target comes from `MACOSX_DEPLOYMENT_TARGET`. Source: `https://github.com/rust-cross/cargo-zigbuild/blob/875d3aa050975e468e4771686f1cdbf35adaeccb/README.md#specify-glibc-version`.
- `cargo-xwin`: builds `*-pc-windows-msvc` from any host with clang, `llvm-tools` (or a full LLVM) and the `xwin` CRT and SDK; usage is `cargo xwin build --target x86_64-pc-windows-msvc` after `rustup target add`; the MSVC CRT and Windows SDK can be pre-cached for offline builds. Using it means accepting the Microsoft licence it links. Source: `https://github.com/rust-cross/cargo-xwin/blob/e152e8fac67e977df9cf8e832c6045e5262fee1a/README.md#pre-caching-for-offline-builds`.
- `xwin` downloads, unpacks and splats the CRT and Windows SDK into a cache directory (`.xwin-cache/splat` with `crt/` and `sdk/` trees). Source: `https://github.com/Jake-Shadle/xwin/blob/723d758d484f505c214e1b8c298fe34b19ba9e92/README.md#splat-options`.
- `cross` runs the build in a container image per target; system packages for a target are added with `target.<triple>.pre-build` in the cross config, using `$CROSS_DEB_ARCH`. Source: `https://github.com/cross-rs/cross/blob/a9301e91f72bba3850460512685f6b16062015ff/docs/config_file.md#target-target-pre-build`.
- Clang is natively a cross-compiler (one binary, `--target`), but every target still needs its own headers, libraries and binutils, found with `--sysroot` and related options. Source: `https://clang.llvm.org/docs/CrossCompilation.html`. lld is selected with `-fuse-ld=lld` (`https://lld.llvm.org/index.html#using-lld`); ThinLTO is described in `https://clang.llvm.org/docs/ThinLTO.html`.

## Process

- Whether a compiler change needs an FCP or RFC: `https://forge.rust-lang.org/compiler/proposals-and-stabilization.html#compiler-flags` and `https://forge.rust-lang.org/mdbook/proposals-and-stabilization.html#when-are-fcps-required`.
