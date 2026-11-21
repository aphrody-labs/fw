# Workspace recipes for the shared root that holds every checkout of ay.toml [repos] side by side.
# The root justfile is one line, `import 'ay/workspace.just'`; everything else is versioned here.
# Recipes run from the root and call typed Bun tools: no repository is named in this file.

set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

just := just_executable()
ws := "bun ay/tools/workspace.ts"

# List the workspace recipes
default:
    @{{ just }} --list

# Branch, dirty files, ahead/behind and last commit of every live checkout
status:
    {{ ws }} status

# Registry vs disk: missing, unregistered or retired checkouts and wrong remotes
audit:
    {{ ws }} audit

# `cargo metadata --no-deps --offline` of every Cargo workspace (no build)
manifests:
    {{ ws }} manifests

# Fetch, then fast-forward every clean checkout
pull:
    {{ ws }} pull

# Toolchain versions on PATH
tools:
    {{ ws }} tools

# Cache environment and sccache statistics
cache:
    {{ ws }} cache

# Run a recipe of one repository from its directory: `just in shenron build-server`
in repo *args:
    {{ ws }} run {{ repo }} {{ args }}
