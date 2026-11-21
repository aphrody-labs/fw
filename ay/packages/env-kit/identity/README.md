# @aphrody/ay-identity

<!-- catalog:begin unit -->
> Aphrody product identity for the M3 surfaces: brand emblem and icons, the Aphrody avatar, the animated pet (React component and <aphrody-pet> Custom Element, Codex-composed tracks, 16-way gaze) drawn from brand/identity.

| Type | Path |
| :-- | :-- |
| Bun package | `packages/env-kit/identity` |

**Commands**

```sh
bun run --cwd packages/env-kit/identity sprite
bun run --cwd packages/env-kit/identity prepack
bun run --cwd packages/env-kit/identity gen
bun run --cwd packages/env-kit/identity typecheck
bun run --cwd packages/env-kit/identity test
bun run --cwd packages/env-kit/identity measure
```

Part of Aphrody: [aphrody-labs/ay](../../../README.md).
<!-- catalog:end -->

The Aphrody product identity for React/M3 surfaces, drawn from `brand/identity` at the root of this repository.

| Import                      | Contents                                                                                                                                                                                                                                                                                                    | Needs                           |
| --------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------- |
| `@aphrody/ay-identity`         | `AphrodyAvatar`, `AphrodyPet`, `BRAND_ICONS`, `brandIcon`, `brandSrcSet`, pet data                                                                                                                                                                                                                          | the `@aphrody/m3` `?url` loader |
| `@aphrody/ay-identity/brand`   | brand emblem icons (`icon-64/128/192.png`) as URLs                                                                                                                                                                                                                                                          | `?url` loader                   |
| `@aphrody/ay-identity/theme`   | `APHRODY_M3_SEED`: the shared Aphrody/Bun product seed for native and web M3 themes                                                                                                                                                                                                                         | nothing                         |
| `@aphrody/ay-identity/atlas`   | `PET_ATLAS_URL` (sprite sheet)                                                                                                                                                                                                                                                                              | `?url` loader                   |
| `@aphrody/ay-identity/pet`     | `PET`, `frameAt`, `gazeFrame`, `spriteStyle` (timed tracks, 16-way gaze)                                                                                                                                                                                                                                    | nothing                         |
| `@aphrody/ay-identity/runtime` | `PET_WEB`, `resolve`, `currentFrame`, `Ambient`, `gaze`: the crate clock (Codex-composed tracks, state lifetimes), tick for tick                                                                                                                                                                            | nothing                         |
| `@aphrody/ay-identity/element` | `<aphrody-pet>` Custom Element, `defineAphrodyPet()`                                                                                                                                                                                                                        | an atlas URL in `src`           |
| `@aphrody/ay-identity/load`    | `loadIcon(source, { size, scale, tint, fit, output })`: PNG, JPEG, WebP, GIF, SVG, ICO (best entry for `size × scale`) and `symbol:<name>?style=…&fill=…&wght=…&grad=…&opsz=…` from cdn.aphrody.com, to an `ImageBitmap` or SVG markup; the browser side of the crate loader | a browser for bitmaps           |

`src/brand.ts` imports `assets/icon-*.png`; `src/atlas.ts` imports `dist/spritesheet.webp`, staged by
`bun run sprite` from `brand/identity/assets/aphrody/sprites/spritesheet.webp`. `src/animations.generated.ts`
is generated from `brand/identity/assets/aphrody/animations.json` by `bun run gen`; `bun run test` (`src/pet.test.ts`) fails when
the two disagree. Typecheck: `bun run typecheck`.

`<aphrody-pet src="…/spritesheet.webp" size="104">`: one timer per frame change, `prefers-reduced-motion`
shows the first frame, `setState("running" | "waiting" | "review" | "failed")`, `clearState()`, `play(name | null)`,
gaze follows the pointer at rest (`no-gaze` disables it). `src/web.generated.ts` comes from the crate's
`assets/aphrody/web.json`; `src/runtime.test.ts` replays the crate's `tests/fixtures/parite-web.json`.
`bun run measure [--wasm file]` prints bundle sizes and timings.
