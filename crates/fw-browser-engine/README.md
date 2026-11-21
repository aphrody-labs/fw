# browser

A Cargo workspace of first-party browser crates around pinned engines: Blink from a Chromium checkout, a lite Obscura engine, Kitesurf and the Servo crates.

## Why

Blink renders anything the web needs and is the only engine with the full DevTools Protocol. A lite engine (Obscura on Servo's parsing and style crates) serves agents that only need DOM, network and script, with less memory. The workspace picks between them (`components/browser-engine`) and keeps every upstream as a pin instead of a copy. It is not a Chromium fork and ships no prebuilt V8, Skia or Blink.

## Install

```sh
bun install --frozen-lockfile
```

Needs Bun canary (`bun check`, `bun lint` and `bun fmt` are native) and the Rust toolchain of `rust-toolchain.toml`. Rust and GN builds run on a build host, not on a workstation.

## Usage

```sh
bun testing/acceptance.ts run                  # every engine x the live Google/Alphabet corpus: verdicts, screenshots, report
bun testing/acceptance.ts bench <url>          # hyperfine cold start + process-tree RSS; --jetstream for JetStream
bun testing/acceptance.ts track                # upstream heads and releases against our pins
bun testing/acceptance.ts serve --schedule     # live dashboard (HTML, JSON, SSE) and its cron
build/engines/build.sh                         # V8 + Skia archive from the Chromium checkout (CHROMIUM_SRC)
```

## Documentation

- [ARCHITECTURE.md](ARCHITECTURE.md): engines, decision record.
- [AGENTS.md](AGENTS.md): rules for agents and contributors.
- [docs/](docs/README.md): layout against Chromium's, Kitesurf.
- Upstreams: `tools/upstreams.toml` (pins in `tools/upstreams.lock.json`); forks (bun, kitesurf, browse, mullvad): `tools/layout.toml` [forks]. What was skipped from the Servo org and why: `bun tools/inventory.ts`.

## Development

```sh
bun PRESUBMIT.ts                               # types, lint, format, tests, layout, Bun-native proof
just check lint test                           # Rust workspace, on the build host
```

## License

Apache-2.0, see [LICENSE](LICENSE).
