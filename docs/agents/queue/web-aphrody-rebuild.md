# web.aphrody.com: what is wrong (measured 2026-11-21 in the owner's Chrome 158, Windows, NVIDIA)

Owner verdict: slop from A to Z, to be redone entirely. Start in Figma and Google Stitch, from googlebook (`plugins/aphrody/skills/design/references/google-design.md`), craft-libs, figcraft and COSMIC; see memory `web-redo-and-real-content`.

## Measured

| Check | Result |
|---|---|
| Renderer | Frozen: `Page.captureScreenshot` timed out after 30 s. The wasm render loop blocks the main thread, so no frame can be captured. |
| Content | The whole page is one `<canvas>`. The only text is a debug line: "aphrody_desktop \| backend=webgpu-hw \| cohort=simd-baseline" (58 characters). |
| Headings and structure | No `h1`; only a `main` landmark; no header, nav or footer. |
| SEO | No meta description and 0 Open Graph tags; the title is "aphrody_desktop — Aphrody WebGPU / Wasm". |
| Fonts | None loaded (`document.fonts` is empty): no Google Sans Flex, no design tokens. |
| LCP | 0: nothing paintable is reported. |
| Payload | `web_os_bg.wasm` is 9,692 KB decoded, for an app that shows no content. |
| Isolation | COOP/COEP are now on (crossOriginIsolated is true). That is the only part that is right. |

## Fix = rebuild

1. Design first, in Figma and Stitch, from the measured googlebook patterns and the COSMIC desktop. The content is real product content, nothing synthetic.
2. Use an HTML shell with real text, landmarks, h1, meta description, Open Graph and JSON-LD derived from `product.toml`, with fonts and icons from material.aphrody.com and media from media.aphrody.com.
3. The WebGPU/wasm desktop becomes a progressive enhancement:
   - it loads after first paint, in a worker (OffscreenCanvas) so it never freezes the page;
   - the wasm is split and lazy-loaded, with a byte budget per route;
   - frames are paced with requestAnimationFrame.
4. Gates: Lighthouse (web-quality skill), screenshot review, no debug strings in production, and the CI check against hardcoded colors and strings.

## Design process for the rebuild (frontend-design and ui-ux-pro-max skills)

1. **Subject first.** The page is Aphrody, an OS and desktop whose stack is fw, aphrody and ay. Open with the most characteristic thing in that world: the real desktop running live (after first paint), not a big-number hero.
2. **Two passes.**
   - Write a compact token plan: 4–6 named colors derived from `product.toml` [brand] through fw-m3, Google Sans Flex and Google Sans Code, a layout described in prose and ASCII wireframes, and the page's principles.
   - Review that plan against the brief, and revise anything that reads as a default.
3. **Avoid the generated-page tells:**
   - cream with serif and a terracotta accent;
   - near-black with one acid accent;
   - identical rounded cards with soft grey shadows and gradient washes;
   - all-caps eyebrow labels, middle-dot meta strings, "WORD — fragment" labels, monospace data labels, "→" on buttons;
   - fade-and-slide entrances on every section;
   - accenting one word in a headline.
4. **One bold element,** everything else quiet. Real copy, active voice, sentence case.
5. **ui-ux-pro-max priorities** (in this order):
   - accessibility: contrast 4.5:1, keyboard, labels;
   - touch targets of 44 px or more;
   - performance: AVIF, lazy loading, CLS < 0.1;
   - responsive layout, mobile first;
   - type and colour only from tokens.

   Run the `references/pro-rules.md` pre-delivery checklist before any deploy.
