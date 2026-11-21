# Bun-native gate: organization standard proposal

Status: proposal for `aphrody-labs/.github` (STANDARD.md, reusable workflows). Nothing here is applied to that repository; the
only workflow written is `ay/.github/workflows/bun-native.yml`, which already works as a reusable workflow.

## Rule

First-party JavaScript and TypeScript runs on Bun with Bun's own APIs. A change that adds a Node.js API, an npm package,
a script, a CI step or a Dockerfile line that has a proven Bun owner fails the check, unless the finding is recorded in the
repository's baseline file.

## What is checked

`tools/bun-native.ts` (ay) holds no hand-written replacement list. Its rules come from:

- the n2b registry published on crates.io (`aphrody-n2b-registry`: apis, cli, globals, modules, packages), read live;
- the "Instead of X, use Y" table of the installed `bun-native` skill;
- a handful of structural rules (shebang, lockfile, tsconfig, CI, Dockerfile) listed in `structural()`.

Each rule is proven before it can gate: a documentation page and heading in the Bun fork (`docs/`), a declaration in its
`bun-types`, and a `bun -e` (or `bun <command> --help`) check executed on the installed Bun, whose version is recorded in
`docs/research/bun-native-rules.toml` (`[meta]`). A rule that fails any of them is `status = "unproven"` and never matches.
Rules derived from the registry that name no concrete Bun feature (`<natif>`, free text) stay unproven.

Severity of a finding:

| Severity | Meaning |
|---|---|
| `safe-rewrite` | Same behaviour per the registry (`aggressive = false`), or a runner swap such as `npm ci` to `bun install --frozen-lockfile` |
| `behaviour-change` | A Bun owner exists and is proven, but edge cases differ; a person reviews the rewrite |
| `no-equivalent` | No proven Bun owner; reported, never applied |

Tier: rules whose replacement is a Web-standard substitute that Bun already runs unchanged (`Buffer.alloc` to `Uint8Array`,
`setImmediate` to `queueMicrotask`) are `tier = "style"`. They are counted separately and do not gate.

Scanning: imports, `require` and dynamic `import()` are read with `Bun.Transpiler.scan`; call sites are matched on source whose
strings, comments, template text and regex literals are masked by a lexer, and only in files whose import graph includes the
module the rule is about; `package.json`, `tsconfig*.json` (JSONC), workflow YAML (`Bun.YAML`), Dockerfiles and shell lines are
parsed as such. Bun exposes no general AST API, so a call-site match is lexical, not semantic.

## The gate

```
bun tools/bun-native.ts check . --baseline .bun-native-baseline
```

- exit 1 and one `file:line: rule: use <replacement> (<severity>)` line per finding that is not in the baseline;
- `--fail-on safe-rewrite` restricts the gate to the safe class;
- `--write-baseline <file>` records the current findings (`<rule> <file>` per line) and exits 0;
- the rules come from the committed rules file: no Bun fork checkout, registry download or network is needed in CI.

The baseline may only shrink. A pull request that adds a line to it needs the same review as a lint suppression.

## Reusable workflow (proposed text for `aphrody-labs/.github`)

`.github/workflows/ci-bun-native.yml`:

```yaml
name: ci-bun-native

# Fails on first-party patterns that have a proven Bun owner. Caller job id must be "bun-native" so the required check is "bun-native / gate".
on:
  workflow_call:
    inputs:
      tools-repository:
        description: Repository that holds tools/bun-native.ts and its rules file
        type: string
        required: true
      tools-ref:
        type: string
        default: main
      baseline:
        type: string
        default: .bun-native-baseline

permissions:
  contents: read

jobs:
  gate:
    runs-on: ubuntu-latest
    timeout-minutes: 10
    steps:
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
        with:
          persist-credentials: false
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
        with:
          repository: ${{ inputs.tools-repository }}
          ref: ${{ inputs.tools-ref }}
          path: .bun-native-tools
          persist-credentials: false
      - uses: oven-sh/setup-bun@0c5077e51419868618aeaa5fe8019c62421857d6 # v2.2.0
      - env:
          BASELINE: ${{ inputs.baseline }}
        run: bun .bun-native-tools/tools/bun-native.ts check . --baseline "$BASELINE"
```

Caller, in each repository that has JS or TS:

```yaml
jobs:
  bun-native:
    uses: <standard repository>/.github/workflows/ci-bun-native.yml@main
    with:
      tools-repository: <repository that holds tools/bun-native.ts>
```

`STANDARD.md` table row for the reusable workflows: `ci-bun-native.yml` | Bun-native gate: proven Bun owners for Node APIs, npm packages, scripts, CI and Dockerfile lines | baseline file shrinks, never grows.

`tools-repository` is an input, not a literal, because the repository that owns the tool is named in `ay.toml`, not in the standard.

## Adoption per repository

1. `bun tools/bun-native.ts audit --repos <key>` (results go to the private store, `.private/bun-native-audit.toml`) for the current findings.
2. Apply the `safe-rewrite` class, run the repository's own tests, commit.
3. `bun tools/bun-native.ts check . --write-baseline .bun-native-baseline` for what remains, commit the baseline.
4. Add the caller job, then require its check once it is green.

## Limits

- Call-site matching is lexical (see above); a false positive is silenced by a baseline line, never by a code change.
- Rules reflect the registry's classification; `safe-rewrite` is its claim, backed here by existence of the replacement, not by
  an equivalence proof of every call.
- The Bun on a CI runner can differ from the one recorded in the rules file (`[meta].bun_version`, `bun_revision`); the rules file is
  regenerated by `bun tools/bun-native.ts rules`, which re-proves every rule on the Bun it runs on.
