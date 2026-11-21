# material-cdn: complete CDN with à-la-carte builds (queued, not deployed)

## Goal
"Améliore notre CDN pour être complet et choix à la carte": material.aphrody.com serves every Material Symbol (3 styles, full axes, static SVGs), Google Sans Flex / Code, Roboto Flex, M3 tokens (CSS, JSON, DTCG), Aphrody icons and versioned assets; the Worker builds exactly what a request names (icon lists, text or unicode subsets, pinned or limited axes; sprite, JSON, SVG, woff2, CSS). Sources pinned in a lock. Typed Bun/TS and Rust clients. Deploy and verify by bytes. Report coverage counts, size and latency tables, commits, deploy proof.

## Done (pushed to origin/main)
- 00b4c987 typed client: `tools/material-cdn/grammar.ts` (parser and canonical form shared by Worker and clients), `client.ts`, `fixtures/urls.json`, `fw_material::cdn::Request` (Rust). Verified: `bun test` 20 pass (grammar and fixtures), `cargo test -p fw-material --lib` 6 pass, `cargo clippy -p fw-material --all-targets -D warnings` clean, rustfmt clean.
- b5e5b48c Worker source (`cloudflare/material-cdn`: worker.ts, src/{hb,woff2,fonts,icons,meta,http}.ts, tests, README, wrangler.jsonc, `woff2/` Rust wasm encoder) and `tools/material-cdn/wasm.ts`. Verified: `tsc -p .` clean, `bun test test/static.test.ts` 6 pass. Probes (not committed): hb-subset via harfbuzzjs 1.6.4 wasm subsets Outlined in 22 ms; with flag NoLayoutClosure (0x200) a 2-icon subset keeps exactly 11 glyphs and 2 ligatures; FILL glyph ids must be passed via glyph_set; Rust woff2 wasm 924 KB after wasm-opt.
- Nothing deployed, nothing published to R2. The live Worker and bucket are the pre-existing ones. No deploy IDs.

## Not committed (in the working tree of C:\src\fw, mine only; commit by path after verifying)
`tools/material-cdn/{sync.ts, lock.json, build.ts, changed.ts, fonts.py, publish.ts, r2.ts, asset.ts, extras.ts, bench.ts, verify.py}`, `.github/workflows/material-cdn.yml` (push trigger removed on purpose: the worker job is workflow_dispatch only), `plugins/aphrody/skills/google-design/SKILL.md` (section 10, claims URLs that exist only after deploy; the "5 to 10 times" bytes claim is unmeasured, replace by measured numbers).
Status: sync.ts ran (lock written: icons 10ecc1479061, google/fonts 06bfe0d62a98, all 4 blobs verified); extras.ts ran (388 CSS variables, 378 DTCG tokens, 410 Aphrody icons); build.ts ran through the font step on Windows and fonts.py succeeded; build.ts tail (meta.json, index, tarball) and publish/asset/r2 refactor are NOT yet executed. build.ts needs Linux (GNU tar, gzip).

## Remaining
1. On fr1 in `/srv/xbuild/material-cdn` (work dir `work/` already holds the pinned icons checkout 1.6 GB and the fonts; `fw/` and `aphrody/` are stale copies of the tools; re-copy tools from C:\src\fw first): run build.ts, extras.ts. Check `out/v1/meta.json`, `data/symbols/*.json`, `src/*.ttf`, `tokens/`, `aphrody/`.
2. Pull `meta.json`, `data/`, one source TTF per style locally into `.cache/material-cdn-out/v1` and run `bun test` in `cloudflare/material-cdn` (11 Worker tests are skipped until then; they check glyph and ligature counts, sprite ids, caching headers, POST).
3. Publish: `CLOUDFLARE_API_TOKEN=... bun tools/material-cdn/publish.ts --out <out>` (index.json last; additive, old paths untouched), then `bun tools/material-cdn/wasm.ts`, `cd cloudflare/material-cdn && bunx wrangler@4.166.0 types && bunx wrangler@4.166.0 deploy`. Record the deploy ID; roll back with `wrangler rollback` if verify fails.
4. Verify live: `uv run --with fonttools==4.68.0 --with brotli==1.2.1 tools/material-cdn/verify.py`, render a sprite and an icon font in Chrome (check FILL=1 pinned renders the filled glyph), check `content-encoding: br` on svg/css, memory of the 15 MB Rounded source (Worker limit 128 MB; loadInto streams into wasm memory).
5. `bun tools/material-cdn/bench.ts` for the size and cold/warm/R2 latency tables; fix bench.ts sizeRows output if needed.
6. Commit the remaining files by path, push (origin main moved: use a clean clone and cherry-pick if `git pull` is blocked by foreign untracked files), run `bun tools/material-cdn/pin.ts` only if the crate tarball changed.
7. Decide: `woff2` transform (glyf/loca) is off (Chromium rejects some ttf2woff2 transformed output, about 5% smaller on a 12-glyph subset); keep off unless validated in Chrome.

## Directories created (sizes at handoff)
- fr1 `/srv/xbuild/material-cdn/work` 1.6 GB: pinned sources; safe to delete (re-created by sync.ts). `/srv/xbuild/material-cdn/{fw,aphrody,tmp}`: copies of tools and icons, safe to delete.
- fr1 `~/material-cdn` 152 KB of old logs; safe to delete. Old `~/material-cdn/{src,out,dl}` (about 2.5 GB) were removed from `/`.
- PC `C:\src\fw\.cache\material-cdn` (pinned sources, about 1.3 GB) and `.cache\material-cdn-out` (partial local build, about 0.5 GB), gitignored; safe to delete. `C:\tmp\mcdn-probe`, `C:\tmp\extras-out`, `C:\tmp\fwpush`: scratch, safe to delete. `C:\src\fw\cloudflare\material-cdn\{node_modules,wasm,woff2\target}`: build artefacts, gitignored.

## Note on conflicts
origin/main gained fc1cce2b and d703622f (build.ts and fonts.py: sorted manifests, fork-independent sources, uv shebang) after my working-tree edits of `tools/material-cdn/{build.ts,fonts.py,sync.ts}` were made. Merge by hand when committing the uncommitted files: keep their determinism fixes, keep my font/meta/symbol-data changes.
