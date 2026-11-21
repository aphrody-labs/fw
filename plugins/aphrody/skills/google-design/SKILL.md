---
name: google-design
description: "Google Design / Material 3 Expressive rule set for building web UIs (chat and agent apps): colour roles and tonal surfaces, type scale with Google Sans Flex / Roboto Flex, shape scale and morphing"
---
> , spring motion tokens with web cubic-bezier conversions, breakpoints and navigation rail, component specs, accessibility minimums. Values verified live in Nov 2026.


# Google Design: Material 3 Expressive for the Web

Scope: web UIs (HTML/CSS/TS, WGPU/WGSL surfaces). Compose-specific APIs (`MaterialTheme.motionScheme`, `ColorProducer`, 1.5.0 artifacts, Aphrody theme) live in `kotlin-compose-m3`; do not duplicate them here. Units: `dp` = CSS `px`; `sp`/`pt` in the type tables = `rem` x16 (SP/16 = rem, per the type page).

Rule zero: M3 Expressive is an evolution of M3, not "M4"; baseline M3 stays valid. Expressive means emotion with usability: the research cited by Google found key UI elements spotted up to 4x faster (46 studies, 18,000+ participants).
Sources: https://m3.material.io/blog/building-with-m3-expressive , https://design.google/library/expressive-material-design-google-research

## 1. Design language (what Google Design publishes)

- Google Design (https://design.google/?home) is an editorial site: nav = Community, About, Ten Years of Material (`/m10`); categories Spotlight, Perspectives, Podcasts, Conversations, Guides; links out to material.io, fonts.google.com, blog.google. No jobs/events section exists on the site (careers link lives on `/community`). Its own UI is dark, with oversized display type and one saturated hero tile; the M3 Expressive blog hero is a dark surface with large light-weight display type beside saturated lilac/lime tiles (screenshots viewed).
- Agent-UI stance from "The Next Interface" (https://design.google/library/the-next-interface-gemini, read 2026-11-20): design for people, not for AI; restore agency (voice gave a user with tremor and glaucoma independence). Three shifts Google's Gemini team describes, as direction not spec:
  1. Content is the interface: when the system understands its output, drop the card/grid container and compose editorial layouts around intent (type, hierarchy, image, white space) instead of a wall of chat text; modes (type, speak, see) should not each hide behind a toggle.
  2. Living feedback: replace mechanical spinners with organic motion that shows what the model perceives and that it is thinking (particle clusters gathering on recognized objects).
  3. Proactive collaboration: surface the right tool from context; design sets conditions and boundaries, not every step.
  Editorial restraint is part of the design: write the "golden responses" and system instructions that stop filler and visual excess; decide what the model must not generate. Designers build their own instruments (live-type stress tests against real model output, in-browser editing of the live build).
- Gemini web app shell (https://gemini.google.com/app, inspected 2026-11-20): flat left list (new chat, library, scheduled, customize; activity and settings at the bottom) collapsible from its header, chat search in that header; the empty state centres one question headline and the composer together on a soft radial glow; the composer is a full-round field with `+` (upload and tools) at the start and dictation at the end; the composer docks to the bottom once a conversation starts.
- Seven expressive tactics (https://m3.material.io/blog/building-with-m3-expressive): (1) vary shapes, (2) rich colour with tonal surface hierarchy, (3) emphasised typography to guide attention, (4) contain content in groups with the brightest surface for the most important, (5) fluid spring motion and shape morph, (6) flexible components per context, (7) hero moments: only 1-2 per product, brief and surprising; too many overwhelm.

## 2. Colour roles and tonal surfaces

Sources: https://m3.material.io/styles/color/roles , https://m3.material.io/styles/color/system/how-the-system-works , https://m3.material.io/styles/color/static/baseline

- 26 roles, always consumed as tokens (CSS custom properties `--md-sys-color-*`); never literal hex in components. One source colour -> 5 key colours (primary, secondary, tertiary, neutral, neutral variant) -> tonal palettes (tones 0-100 in steps of 10 plus 95, 98, 99) -> roles in light and dark. HCT space: hue 0-360, chroma up to ~120, tone 0 (black) to 100 (white); tone difference sets contrast (tones 50/98 give 3:1, 30/98 give 7:1). Do not mix HSL values with HCT.
- Pair rules: fill + its `on-*` colour only (`primary`/`on-primary`, `primary-container`/`on-primary-container`, ...). Container roles are fills, never text or icon colour. Wrong pairings break when the user raises contrast (standard, medium 3:1, high 7:1 levels exist; expose a contrast preference).
- Emphasis: primary = highest-emphasis action (FAB, filled button, active); secondary = recessive (tonal button, filter chip, selected nav indicator); tertiary = small contrasting accent (badge, input highlight); error roles are static (do not change with dynamic colour, still flip in dark).
- Surfaces: `surface` for page background; `surface-container-lowest|low|(default)|high|highest` for cards, sheets, dialogs, nav; `on-surface` primary text, `on-surface-variant` secondary text/icons. Typical shell: body on `surface`, nav area on `surface-container`. Keep the same role mapping per layout region across breakpoints; at larger sizes only add extra container tones. Optional `surface-dim` / `surface-bright` keep relative brightness across themes (use bright for a chat window, dim for a rail, if wanted). Fixed colours and `primary-fixed*` do not change between themes: avoid where contrast matters.
- Inverse (`inverse-surface`, `inverse-on-surface`, `inverse-primary`): snackbars and toasts.
- Outline: `outline` for important boundaries (text field border); `outline-variant` for dividers, card borders, decorative; never use `outline` for dividers; never use `outline-variant` for the boundary of a target unless inner content has 4.5:1.
- Baseline light hexes (from the baseline token table, usable as static fallback): primary #6750A4 / on #FFFFFF / container #EADDFF / on-container #4F378B; secondary #625B71 / #FFFFFF / #E8DEF8 / #4A4458; tertiary #7D5260 / #FFFFFF / #FFD8E4 / #633B48; error #B3261E / #FFFFFF / #F9DEDC / #8C1D18; surface #FEF7FF, on-surface #1D1B20, on-surface-variant #49454F, outline #79747E, outline-variant #CAC4D0, surface-container lowest..highest #FFFFFF #F7F2FA #F3EDF7 #ECE6F0 #E6E0E9, inverse-surface #322F35, inverse-on-surface #F5EFF7. Prefer generating schemes with Material Color Utilities / Theme Builder from a brand source colour over hand-editing.
- State layers: overlay in the content colour (on-colour) at hover 8%, focus 10%, pressed 10%, dragged 16%, disabled 38% (disabled uses the 0.38 opacity token). One layer at a time. Visual size 40dp inside a 48dp interactive target.

## 3. Typography

Sources: https://m3.material.io/styles/typography/type-scale-tokens , https://m3.material.io/styles/typography/fonts , https://design.google/library/google-sans-flex-font , https://fonts.google.com/specimen/Google+Sans+Flex

Baseline scale (size / line-height px; weight; tracking px). rem = size/16.

| Style | size/lh | wt | track |
|---|---|---|---|
| Display L / M / S | 57/64, 45/52, 36/44 | 400 | -0.25, 0, 0 |
| Headline L / M / S | 32/40, 28/36, 24/32 | 400 | 0 |
| Title L | 22/28 | 400 | 0 |
| Title M / S | 16/24, 14/20 | 500 | 0.15, 0.1 |
| Body L / M / S | 16/24, 14/20, 12/16 | 400 | 0.5, 0.25, 0.4 |
| Label L / M | 14/20, 12/16 | 500 (prominent 700) | 0.1, 0.5 |
| Label S | 11/16 | 500 | 0.5 |

- Emphasised set (15 more styles, same sizes and line heights): Display, Headline, Title L, Body = weight 500; Title M/S and all Label = weight 700; tracking unchanged. Token swap: `md.sys.typescale.display-large` -> `md.sys.typescale.emphasized.display-large`. Use on selected items, unread, primary actions, badges, extended FAB, headlines; not by default on every component.
- Scale base: 14 key size, Major Second ratio 1.125; keep contrast between sizes; remove unused styles instead of adding near-duplicates. Do not change sizes when customising (breaks component reflow); tune line height and tracking instead (heavier fonts need more tracking).
- Two typeface slots: brand (Display/Headline, expression) and plain (Body/Label, readability). Default Roboto for both. Variable fonts: Roboto Flex (axes slant, width, weight, grade, optical size + advanced axes; Latin/Greek/Cyrillic) is not yet in the official scale but is the documented drop-in; Roboto Serif for reading; Roboto Mono for code and aligned numbers; Noto Sans as fallback.
- Fallback chain: `Roboto Flex, Roboto, Noto Sans`-family (keeps font branding when a script is missing).
- Google Sans Flex: open-sourced 2025, listed on Google Fonts as variable, axes weight (100-900, default 400), width (default 100), optical size, slant (default 0), roundness (`ROND`, default 0), grade. Use for brand/display slots; M3 shapes and Google Sans Flex share roundness cues, so raise `ROND` with rounder shapes. Set `font-optical-sizing: auto`.
- Language height: default to medium height line-heights; Latin/Cyrillic/Greek/Hebrew = small (base), CJK/Arabic/Thai/Hindi etc = medium (~7% taller), Burmese/Telugu ~30% taller, Nastaliq ~100% taller. Components with fixed heights break on tall scripts: use min-height and padding, not fixed height.
- Web units: `font-size` in rem; letter-spacing = tracking px / font px (e.g. 0.2/16 = 0.0125). Keep 40-60 characters per line for body text.

## 4. Shape

Sources: https://m3.material.io/styles/shape/corner-radius-scale , https://m3.material.io/styles/shape/overview-principles

- Ten-step radius scale: none 0, extra-small 4, small 8, medium 12, large 16, large-increased 20, extra-large 28, extra-large-increased 32, extra-extra-large 48, full (pill; the spec moved from "50% of component size" to a true full token).
- Asymmetric corners are allowed (same scale): inner corners for grouped items (menus, split buttons, connected button groups).
- Nested rounding: inner radius = outer radius - padding (48 - 14 = 34). Never reuse one radius for parent and child.
- Defaults seen in specs: dialog container 28dp (full-screen dialog 0dp); assist/filter chip 8dp; buttons full (round) or square variants (below); text fields use their own spec.
- Do not put large or full radii or cut corners on information-dense containers (cards with text); cut-corner families need extra padding.
- 35-shape library (`MaterialShapes`) with built-in morph: use on decorative/visual elements only (avatar masks, image crops, loading indicator, hero art), never on text-heavy containers; use sparingly and with a reason. Morph to express: selection, work in progress, environmental change. Shapes are versatile, not semantic (a wavy form is not a fixed symbol of progress).
- Tension: mix round and square (e.g. square for selected, round for resting) to direct attention; a shape that breaks the surrounding style signals importance; small shapes make essential actions look unimportant.

## 5. Motion (physics system)

Sources: https://m3.material.io/styles/motion/overview/how-it-works , https://m3.material.io/styles/motion/overview/specs

- Two schemes: expressive (overshoots; default for most UI and hero moments) and standard (minimal bounce; utilitarian products). Apply one scheme product-wide; swap per element only for key moments. The scheme is not part of the token name: `md.sys.motion.spring.{fast|default|slow}.{spatial|effects}`.
- Spatial springs: position, size, rotation, corner radius; may overshoot. Effects springs: colour and opacity; never overshoot (damping 1).
- Speeds: fast = small components (switch, button; colour of switch handle); default = partial-screen (bottom sheet, expanded rail; opacity of rail content); slow = full-screen transitions and content refresh.
- Spring values (damping ratio / stiffness), Android token view:

| token | expressive | standard |
|---|---|---|
| fast spatial | 0.6 / 800 | 0.9 / 1400 |
| default spatial | 0.8 / 380 | 0.9 / 700 |
| slow spatial | 0.8 / 200 | 0.9 / 300 |
| fast effects | 1 / 3800 | 1 / 3800 |
| default effects | 1 / 1600 | 1 / 1600 |
| slow effects | 1 / 800 | 1 / 800 |

- Web: use real springs when interruptible or gesture-driven (Web Animations / spring lib with the numbers above); otherwise the official curve conversions (cubic-bezier x1,y1,x2,y2 / duration):
  - expressive spatial: fast `0.42,1.67,0.21,0.90` 350ms; default `0.38,1.21,0.22,1.00` 500ms; slow `0.39,1.29,0.35,0.98` 650ms
  - standard spatial: fast `0.27,1.06,0.18,1.00` 350ms; default same curve 500ms; slow same curve 750ms
  - effects (both schemes): fast `0.31,0.94,0.34,1.00` 150ms; default `0.34,0.80,0.34,1.00` 200ms; slow `0.34,0.88,0.34,1.00` 300ms
- Component motion is driven by just two tokens: expressive fast spatial and fast effects. Legacy easing/duration/path tokens are superseded; keep only as fallback.
- Animate: state changes, selection, shape morph on press/select, panel open/close (rail, sheets). Keep effects (colour, opacity) free of overshoot; honour `prefers-reduced-motion` by dropping spatial overshoot (house rule, not from the spec).

## 6. Layout, breakpoints, navigation

Sources: https://m3.material.io/foundations/layout/breakpoints/overview , https://m3.material.io/foundations/layout/layout-overview/parts-of-layout , https://m3.material.io/foundations/layout/canonical-examples/overview , https://m3.material.io/components/navigation-rail/guidelines , https://m3.material.io/foundations/layout/grids-spacing/spacing

| breakpoint | width | panes | navigation | dialogs / supplemental |
|---|---|---|---|---|
| compact | < 600 | 1 | navigation bar, or modal expanded rail | full-screen/simple dialog, bottom sheet |
| medium | 600-839 | 1 (2 only if low density) | collapsed rail (bar if few destinations) or modal expanded rail | simple dialog, menu |
| expanded | 840-1199 | 1-2 (2 recommended) | collapsed rail; modal or standard expanded | simple dialog, menu |
| large | 1200-1599 | 1-2 (2 recommended) | same as expanded | simple dialog, menu |
| extra-large | 1600+ | 1-3 | same as expanded | simple dialog, menu |

- Design for breakpoints, not devices; window size changes with multi-window and foldables. Per step ask: what to reveal, divide, resize, reposition, swap. Swap only functionally equivalent components (bar <-> rail; bottom sheet <-> menu); never button <-> chip.
- Scaffold regions: bars (top app bar, navigation bar), rails (nav rail, toolbars, chat input, FAB, primary controls), panes (1-3; all content lives in a pane), drag handles to resize panes. Canonical layouts: feed, list-detail, supporting pane (primary ~2/3, secondary remainder). Chat/agent app = list-detail (conversations | thread) at expanded+, thread-only at compact; levitate (layered) pane for focused tasks.
- Panes may blend into the background (implicit grouping) with close spacing; use colour/outline for explicit grouping. Spacing: consistent rhythm, same spacing for similar items, buttons near the content they affect, generous negative space and brightest surface for the key area.
- Navigation rail: leading edge only, vertical only; 3-7 destinations; never hidden when collapsed; never together with a navigation bar; compact windows use the bar. Menu button and FAB always top-aligned; destination group top or centre aligned (centre on tablets); items stay fixed while content scrolls. FAB inside rail rests at elevation level 0 and becomes an extended FAB when the rail expands. Active indicator only on the one current destination; filled icon + indicator on selection; one-word labels, never truncated or shrunk. Badges: top-right of the icon in compact rail, next to the label when expanded. Expanded rail: standard (beside content, large windows) or modal (overlays; dense layouts or many items); can reveal secondary destinations. Container fill optional; if removed keep 3:1 for items. Baseline rail tokens: container width 80dp, icon 24dp, active indicator 56x32dp, label 12/16.
- Expressive nav: baseline rail is "not recommended"; use collapsed + expanded rail variants. Bottom app bar is replaced by docked toolbar; floating toolbar (horizontal or vertical, standard or vibrant, can pair with a FAB); never show a toolbar with a navigation bar.

## 7. Component specs for chat/agent web apps

Sources: https://m3.material.io/components/{floating-action-button,buttons,button-groups,chips,text-fields,dialogs,lists,loading-indicator,toolbars}/ (specs or overview pages) , https://m3.material.io/foundations/interaction/states/state-layers

- Web status: loading indicator, button groups and toolbars are listed "Web: Expressive unavailable"; build them from tokens (no official web component).
- FAB: 56dp container, 24dp icon (regular); medium and large exist in expressive; small is not recommended. Default colour primary-container/on-primary-container; also secondary/tertiary containers and solid primary/secondary/tertiary. State layer colour equals the icon colour. Hover 8% and elevation 4; focus and press 10%. Use for the single key action (new chat).
- Buttons: five colour styles (elevated, filled, tonal, outlined, text); five sizes XS/S/M/L/XL (default S); round (full) or square shape; toggle buttons (no text style) swap resting shape round -> square when selected. Square corner radius per size 12/12/16/28/28dp; pressed shape 8/8/12/16/16dp; pressed morphs towards square. Small padding 16dp (24dp is not recommended). Text/icon container pairs need 3:1. XS and S need a 48x48 target even if visually smaller.
- Button group: standard or connected (connected replaces segmented button); all sizes; single-select, multi-select or selection-required; shape morph on press and select.
- Emphasised type (section 3) on primary buttons, extended FAB, selected list/menu items.
- Chips: container height 32dp, corner 8dp, icon 18dp, padding 16dp (8dp with icon), gap 8dp. Filter chips use secondary container. Row of chips = one unified control.
- Text fields: container height 56dp; vertical padding 8dp; horizontal 16dp (12dp with icons); icon-text gap 16dp; supporting text top padding 4dp. Border uses `outline`; error uses error roles.
- Dialogs (basic): container radius 28dp; padding 24dp; icon 24dp; headline 24/32; body 14/20; icon-title gap 16, title-body gap 16, body-actions gap 24, between buttons 8dp. Full-screen dialog: radius 0, 56dp bottom action bar, 24dp padding, 8dp between elements. Compact may use full-screen or simple dialog; larger use basic dialog.
- Lists: item leading/trailing space 16dp, top/bottom 10dp, gap 12dp; avatar 40dp; icon 24dp (expressive 20dp); image 56x56; video 100x56 (small) or 114x64 (large); dividers use `outline-variant`. Selected items may use emphasised label.
- Loading indicator (replaces indeterminate circular progress): for waits under about 5s; contained or uncontained; shape morph plus motion; always reflects a real process, never decoration; not for indeterminate-to-determinate flows; use for pull-to-refresh. Determinate/longer waits: progress indicators (wavy waveform and thickness configurable).
- Snackbar/toast: inverse-surface with inverse-on-surface text and inverse-primary action.
- Message-send emphasis: make the Send action larger and nearer the input with a distinct role colour (research: spotted 4x faster).

## 8. Accessibility minimums

Sources: https://m3.material.io/foundations/designing/overview , https://m3.material.io/foundations/designing/structure , https://m3.material.io/foundations/designing/color-contrast

- Contrast: text under 18pt regular / 14pt bold needs 4.5:1; large text and graphics 3:1; clustered controls (button rows) need container-vs-background 3:1; standalone prominent controls like FAB are exempt; disabled states exempt. Provide medium (3:1) and high (7:1) contrast schemes via role mapping.
- Targets: touch 48x48dp minimum (visual 24dp icon plus padding; iOS notes 44), pointer minimum 44x44dp, 8dp spacing between targets.
- Focus: always visible focus indicator on every interactive element (keyboard); state layer focus 10%; focus order follows DOM order; M3 focus-indicator tokens on FAB show 3dp and 2dp (thickness, offset).
- Semantics: native elements first (real `<dialog>`, `<button>`, `<nav>`), custom widgets need extra assistive-tech testing. Landmarks: navigation, search, main (one), banner (one), complementary, contentinfo (one), region (labelled), form; label repeated landmarks uniquely without repeating the role word. Headings follow content hierarchy, no skipped levels, one H1; styling is independent of level. Write DOM in the intended reading order.
- Never convey state by colour alone: add icon, shape, text or motion cues. Do not shrink type to fit labels.

## 9. Build checklist

1. Generate scheme tokens from one source colour; light, dark and 3 contrast levels; map only via roles.
2. Type tokens from the section 3 table; brand slot = Google Sans Flex / Roboto Flex with fallback chain; rem units; tall-script safe heights.
3. Shape tokens 0-48 plus full; component mapping through tokens; 1-2 hero shapes max.
4. Motion tokens as CSS variables with the cubic-bezier table; real springs for gestures; reduced-motion branch.
5. App shell: bar at compact, rail at medium+, list-detail at expanded, modal rail when dense.
6. Agent surfaces: empty state = headline + composer centred, docked after the first message; tools and attachments behind a `+` menu inside the composer, filled from the live tool list; working state = ambient motion (tonal glow, morphing shape), never a bare spinner; answers render as typeset content (headings, lists, code blocks), not as bubbles.
7. Audit: contrast, 48px targets, visible focus, landmarks/headings, reduced motion, language height.
Reference build: `crates/fw-proto/ui` (live at https://fw.aphrody.com), Figma file `OOkkGnYgrk4DktxSpuphC2`.

## 10. Assets from the Material CDN (material.aphrody.com)

Never vendor Material Symbols, Google Sans Flex, Google Sans Code or Roboto Flex files into a UI; ask the CDN for exactly what the screen uses. Typed helpers: `tools/material-cdn/client.ts` (Bun/TS, dependency-free, usable from `site/build.ts`) and `fw_material::cdn::Request` (Rust); both return the canonical URL, so equal needs share one cache entry. Reference: `cloudflare/material-cdn/README.md`.

- Icons by name, outline and filled, the shape `site/build.ts` inlines (`<symbol id="o-home">`, `f-home`): `/v1/icons.svg?style=rounded&icons=home,search&fill=0..1&ids=of`. Weights 100..700 via `wght=`, JSON path list at `/v1/icons.json`, one SVG at `/v1/icon.svg?icons=home&fill=1&size=24&color=%23fff`. Static SVGs exist for opsz 24 and GRAD 0 only; other axes need the font.
- Icon font with exactly the icons used: `/v1/icons.css?style=outlined&icons=home,search&fill=0..1&wght=400` returns `@font-face` plus classes (`.ms-home::before`), its `src` is an immutable `icons.woff2?...&v=<rev>`. Axes not named are pinned to their default (FILL 0, wght 400, GRAD 0, opsz 24); `axes=all` keeps the rest whole; `wght=300..700` limits, `wght=400` pins. Real axes: FILL 0..1, wght 100..700, GRAD -50..200, opsz 20..48.
- Text fonts by characters: `/v1/fonts/google-sans-flex.css?text=Aphrody&wght=400..700&wdth=100` (or `subsets=latin,latin-ext` for `unicode-range` splits). Axes of Google Sans Flex: ROND, opsz, slnt, wdth, wght; Google Sans Code: wght; Roboto Flex: GRAD, XOPQ, XTRA, YOPQ, YTAS, YTDE, YTFI, YTLC, YTUC, opsz, slnt, wdth, wght. Pin every axis the design does not animate (measured on the live CDN, bytes on the wire with br, Google Sans Flex latin: all axes whole 1048100 B, wght 400 only 18964 B; 20 Outlined icons: all four axes whole 27744 B, defaults 4632 B, FILL 0..1 5572 B).
- Tokens: `/v1/tokens/m3.css` (custom properties, colour roles per `[data-scheme]`), `m3.json`, `m3.dtcg.json` (W3C DTCG), generated from `fw-m3/tokens/google_design_tokens.toml`; Aphrody's own icons at `/v1/aphrody/icons/<name>.svg` (`set=aphrody` for sprites); versioned shared assets (sprite sheets, GLB) at `/v1/assets/<name>/<version>/<file>`, immutable.
- Verify an output by its bytes, not its status: `uv run --with fonttools --with brotli tools/material-cdn/verify.py` fetches the live CDN and checks glyph and ligature counts of a returned font and the symbols of a sprite.
