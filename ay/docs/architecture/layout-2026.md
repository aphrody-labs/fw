# Layout 2026: ay

The target layout of this repository. The rules, the conventions with their sources, the proof of relocation safety and the upstream-or-in-house decisions are in
`docs/architecture/layout-2026.md` of aphrody-labs/fw; this page is the decision for every tracked path of this repository, generated from `layout.toml`
(`bun ../fw/tools/layout.ts doc`) and `git ls-files`. A path that is not in the map fails `bun ../fw/tools/layout.ts check` in CI.

ay is the SDK, cloud and edge layer: Cargo members in `crates/*`, Bun packages and apps, `python/` for uv members, `kotlin/` for the Gradle root. It depends on fw (and aphrody) only; direction fw <- aphrody <- ay.

## Decision for every tracked path

<!-- layout:begin generated (bun tools/layout.ts doc, from layout.toml and git ls-files of 1897 paths) -->

### Every tracked path, by area

| area | files | decision | reason |
| --- | ---: | --- | --- |
| `.bazelrc` | 1 | keep | Bazel requires these at the workspace root |
| `.bazelversion` | 1 | keep | Bazel requires these at the workspace root |
| `.bun-native-baseline` | 1 | keep | Bun workspace root: install, lockfile, runtime config and TypeScript base config are read from the root |
| `.cargo` | 1 | keep | Cargo reads .cargo/config.toml from the workspace root |
| `.clang-format` | 1 | keep | editors and search/lint tools discover their config upward from the root |
| `.clang-tidy` | 1 | keep | editors and search/lint tools discover their config upward from the root |
| `.claude` | 12 | keep | Claude Code project agents, workflows and memory |
| `.editorconfig` | 1 | keep | editors and search/lint tools discover their config upward from the root |
| `.gitattributes` | 1 | keep | Git reads these from the repository root |
| `.github` | 15 | keep | GitHub reads workflows, actions and templates from .github |
| `.gitignore` | 1 | keep | Git reads these from the repository root |
| `.gitmessage` | 1 | keep | Git reads these from the repository root |
| `.gitmodules` | 1 | keep | Git reads these from the repository root |
| `.markdownlint-cli2.jsonc` | 1 | keep | editors and search/lint tools discover their config upward from the root |
| `.oxfmtrc.json` | 1 | keep | Bun workspace root: install, lockfile, runtime config and TypeScript base config are read from the root |
| `.oxlintrc.json` | 1 | keep | Bun workspace root: install, lockfile, runtime config and TypeScript base config are read from the root |
| `.python-version` | 1 | keep | uv reads the Python project, workspace and pin from the root |
| `abi` | 5 | keep | ABI description (ay.abi.toml) and its generators, one source for the C, Rust and wasm bindings |
| `AGENTS.md` | 1 | keep | agent rules and the style contract; agents load them from the root |
| `apps` | 106 | keep | Bun workspaces convention: deployable apps live in apps/* |
| `ay.local.example.toml` | 1 | keep | declarative product, naming, release and layout configuration: the single root source other files are derived from |
| `ay.toml` | 1 | keep | declarative product, naming, release and layout configuration: the single root source other files are derived from |
| `brand` | 57 | keep | brand assets, identity and tokens, the input of tokens-compose |
| `bun.lock` | 1 | keep | Bun workspace root: install, lockfile, runtime config and TypeScript base config are read from the root |
| `bunfig.toml` | 1 | keep | Bun workspace root: install, lockfile, runtime config and TypeScript base config are read from the root |
| `Cargo.lock` | 1 | keep | Cargo and the Rust toolchain read these from the workspace root |
| `Cargo.toml` | 1 | keep | Cargo and the Rust toolchain read these from the workspace root |
| `CLAUDE.md` | 1 | keep | agent rules and the style contract; agents load them from the root |
| `clippy.toml` | 1 | keep | Cargo and the Rust toolchain read these from the workspace root |
| `config` | 21 | keep | dotfile sources for the developer shell tools, linked by config/links.toml |
| `corelib` | 50 | keep | freestanding assembly and wasm core library with its own proof and tests |
| `cpp` | 4 | keep | CMake root of the C++ edge ABI (the other C++ loader joins it) |
| `crates` | 332 | keep | Cargo convention: flat crates/* members, one directory per crate |
| `docs` | 119 | keep | documentation tree |
| `justfile` | 1 | keep | just discovers the justfile at the root |
| `kotlin` | 11 | keep | Gradle root: settings.gradle.kts, version catalog and wrapper at kotlin/ |
| `layout.toml` | 1 | keep | declarative product, naming, release and layout configuration: the single root source other files are derived from |
| `LICENSE` | 1 | keep | repository front matter and legal files, read by GitHub and package registries at the root |
| `locales` | 2 | keep | message catalogs by locale (fr) |
| `map.toml` | 1 | keep | declarative product, naming, release and layout configuration: the single root source other files are derived from |
| `MAP.tsv` | 1 | keep | declarative product, naming, release and layout configuration: the single root source other files are derived from |
| `MODULE.bazel` | 1 | keep | Bazel requires these at the workspace root |
| `naming.toml` | 1 | keep | declarative product, naming, release and layout configuration: the single root source other files are derived from |
| `package.json` | 1 | keep | Bun workspace root: install, lockfile, runtime config and TypeScript base config are read from the root |
| `packages` | 696 | keep | Bun workspaces convention: libraries in packages/* |
| `packages/env-kit` | 205 | keep | polyglot .env loaders: one self-contained project per language (cpp, dotnet, go, lua, python, shell) inside one Bun package; uv and CMake accept members anywhere through globs |
| `paths.allow` | 1 | keep | declarative product, naming, release and layout configuration: the single root source other files are derived from |
| `pyproject.toml` | 1 | keep | uv reads the Python project, workspace and pin from the root |
| `python` | 4 | keep | uv workspace members (python/ay_*_py) |
| `README.md` | 1 | keep | repository front matter and legal files, read by GitHub and package registries at the root |
| `release.toml` | 1 | keep | declarative product, naming, release and layout configuration: the single root source other files are derived from |
| `REVIEW.md` | 1 | keep | repository front matter and legal files, read by GitHub and package registries at the root |
| `rust-toolchain.toml` | 1 | keep | Cargo and the Rust toolchain read these from the workspace root |
| `submodules.toml` | 1 | keep | declarative product, naming, release and layout configuration: the single root source other files are derived from |
| `third_party` | 99 | keep | submodule gitlinks listed in submodules.toml (forks and upstreams ay builds against) |
| `tools` | 117 | keep | repository scripts (Bun) with their own tests; one script per task |
| `tsconfig.json` | 1 | keep | Bun workspace root: install, lockfile, runtime config and TypeScript base config are read from the root |
| `uv.lock` | 1 | keep | uv reads the Python project, workspace and pin from the root |
| `workspace.just` | 1 | keep | just discovers the justfile at the root |

### Move table (old path -> new path)

| old path | new path | reason | status |
| --- | --- | --- | --- |
| (none) | | | |

<!-- layout:end generated -->
