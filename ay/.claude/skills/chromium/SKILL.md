---
name: chromium
description: Screenshot a URL or evaluate JavaScript in a real Chromium page from Rust, the `ay-chromium` CLI or Bun (`@aphrody/chromium`), through CEF off-screen rendering. Use when a task needs a rendered page as a PNG, a value computed inside a live page (document.title, DOM state, a promise), a browser proof of a web UI, or when building on the CEF embedding of the browser product (ADR-3).
---

# Chromium (CEF) from Rust and Bun

One engine, three entry points; reference in `docs/chromium.md`.

| Need | Use |
|---|---|
| PNG of a page | `ay-chromium screenshot <url> <out.png> [--width N --height N --wait MS --timeout S]` |
| Value from a page as JSON | `ay-chromium eval <url> '<js expression>' [--wait MS --timeout S]` |
| Same from Bun | `import { screenshot, evaluate } from "@aphrody/chromium"` |
| Same from Rust | `ay_chromium::Chromium::start(Config::default())?` then `.screenshot(..)` / `.evaluate(..)` |
| Get the CEF binaries without cargo | `ay-chromium install [dir]` (downloads the matching build, prints the directory) |

## Run it (build host only, never the PC)
Keep the CEF download, the target directory and the checkout on the build volume: `/srv/xbuild/shared/<dir>` on fr1
(fr2 sees the same path over NFS). The roots of both hosts are too small.
```sh
ssh fr1
cd /srv/xbuild/shared/<dir>/ay
export CARGO_INCREMENTAL=0 CEF_PATH=/srv/xbuild/shared/<dir>/cef CARGO_TARGET_DIR=/srv/xbuild/shared/<dir>/target
cargo build --release -p ay_chromium
target/release/ay-chromium eval https://example.com document.title      # "Example Domain"
target/release/ay-chromium screenshot https://example.com /tmp/x.png    # {"path":...,"width":1280,"height":720}
cd packages/chromium && bun run build && bun test                       # addon + helper into native/, then the test
# fr2 runs the same binaries from the shared path: libcef.so sits next to ay-chromium
```

## Pages and backends
`ay_chromium::{Backend, Page}` is the engine-neutral shape (`open(url)`, `screenshot`, `evaluate`) for the future
`ay_browser` crate; `Chromium` is the CEF backend. A page keeps its state between calls and closes on drop.

## Rules
- Expressions are evaluated with `returnByValue` and `awaitPromise`: return plain data, or a promise of it.
- The call waits for the main frame's load event; add `--wait` for pages that render after it.
- A failed load is an error (`page failed to load: ...`), a thrown exception is `javascript error: ...`.
- CEF starts once per process and cannot restart: in Bun keep one process, call `shutdown()` at the end.
- Linux runs headless (`--ozone-platform=headless`, software rendering): no display server is needed.
- Proof of a UI change is the PNG, inspected, not the exit code: copy it back (`ssh <host> 'cat /tmp/x.png' > file.png`) and read it.
