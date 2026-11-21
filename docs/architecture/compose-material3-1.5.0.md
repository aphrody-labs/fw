# Compose Material 3 1.5.0 Stable & Pure WGPU Expressive Architecture

- **Primary source**: `https://m3.material.io/blog/compose-material-3-is-stable` (published `2026-11-18`)
- **Release notes**: `https://developer.android.com/jetpack/androidx/releases/compose-material3#1.5.0`
- **Jetpack Compose 1.13 blog**: `https://developer.android.com/blog/posts/jetpack-compose-november-2026-release`
- **Verified**: `2026-11-19` against Google Maven (`dl.google.com`), Maven Central (`repo1.maven.org`), and live rendered pages.

## 1. Verified Artifact & Version Matrix

| Coordinate | Version | Status | Notes |
| --- | --- | --- | --- |
| `androidx.compose:compose-bom` | `2026.11.01` | Stable | Requires Kotlin `2.2+` and `compileSdk 37.1` |
| `androidx.compose.material3:material3` | `1.5.0` | Stable | Core + Material 3 Expressive (`1.6.0-alpha01` also published) |
| `androidx.compose.material3:material3-ripple` | `1.5.0` | Stable | Dedicated artifact with inset focus rings (`LocalRippleThemeConfiguration`) |
| `androidx.compose.material3:material3-window-size-class` | `1.5.0` | Stable | Adaptive window size classes |
| `androidx.compose.material3:material3-adaptive-navigation-suite` | `1.5.0` | Stable | Adaptive rail/bar/drawer navigation |
| `androidx.compose.material3:material3-lint` | `1.5.0` | Stable | Android Lint checks |
| `androidx.compose.material3.adaptive:adaptive` | `1.3.0` | Stable | Adaptive panes (`AnimatedPane`, scaffolds) |
| `androidx.compose.ui:ui` | `1.13.0` | Stable | Progressive blur (`Modifier.blur { BlurRadiusSpec }`), `PolygonShape`, `MorphPolygonShape`, shared `VectorPainter` draw cache |
| `org.jetbrains.compose.material3:material3` | `1.13.0-rc01` | RC, not used | Compose Multiplatform port, based on androidx material3 `1.5.0-rc01` (see section 1.1) |
| `com.aphrody:aphrody-theme` | `0.4.0` | GitHub Packages | The Aphrody theme (48 colour roles light and dark from the ay brand tokens, `AphrodyTheme` on `MaterialExpressiveTheme` with `MotionScheme.expressive()`), owned by `aphrody-labs/ay` (`kotlin/aphrody-theme`), coordinates in `product.toml` [kotlin] |
| `com.aphrody.fw:fw-core` | `product.toml` `version` | Gradle build | Aphrody Labs Kotlin SDK (`core/kotlin/fw-core`, JVM) |
| `com.aphrody.fw:fw-material3` | `product.toml` `version` | Gradle build | Android library: fw components and `M3Catalog` on `aphrody-theme` (`core/kotlin/fw-material3`) |
| `com.aphrody.fw:fw-rust-ffi` | `product.toml` `version` | Gradle build | Bidirectional JDK 25 Panama FFM Kotlin <-> Rust bridge (`core/kotlin/fw-rust-ffi`, JVM) |

Group `com.aphrody` is the reversed root domain of `ay.toml` [domains] in `aphrody-labs/ay`; `product.toml` [kotlin] holds the fw group and the theme coordinates, `core/kotlin/settings.gradle.kts` reads them.

### 1.1 Why an Android library on androidx, not Compose Multiplatform

androidx `material3:1.5.0` publishes only Android artifacts (`material3-android`, `material3-jvmstubs`; there is no desktop target on Google Maven), so `fw-material3` is an Android library. The JVM desktop route would need Compose Multiplatform, whose material3 port does not yet exist as a stable release based on 1.5.0. Mapping, from the JetBrains/compose-multiplatform GitHub releases (checked `2026-11-19`):

| Compose Multiplatform | Date | `org.jetbrains.compose.material3` | Based on androidx material3 |
| --- | --- | --- | --- |
| `1.12.2` (latest stable) | `2026-10-15` | `1.12.0-alpha03` | `1.5.0-alpha22` |
| `1.13.0-rc01` (latest) | `2026-11-18` | `1.13.0-rc01` | `1.5.0-rc01` |

### 1.2 One theme, owned by ay

fw defines no colour, shape or motion token. `FwExpressiveTheme` calls `AphrodyTheme`; components read `MaterialTheme.colorScheme`, `MaterialTheme.shapes` and `MaterialTheme.motionScheme`. The generator is `tools/tokens-compose.ts` in `aphrody-labs/ay` (`brand/tokens/compose/AphrodyColorScheme.kt`), compiled in place by `kotlin/aphrody-theme` and published to the GitHub Packages Maven registry of that repository.

- Published artifact: needs `read:packages` (`GITHUB_ACTOR` and `GITHUB_TOKEN`, or `gpr.user` and `gpr.key`). The reading workflow of another repository must be granted access in the package settings.
- Local ay checkout: set the Gradle property `aphrody.ay.checkout` (for example in a user-level `gradle.properties`) and Gradle includes `<checkout>/kotlin` as a composite build that substitutes the artifact; nothing is copied.
- Gate: `bun tools/kotlin-check.ts [--ay <ay checkout>]` builds and tests on the build host (host-local settings in `~/.config/fw/kotlin-build.env` there: `ANDROID_HOME`, `GRADLE_USER_HOME`, `CARGO_TARGET_DIR`); the `fw-rust-ffi` tests load the `libfw_abi.so` that it builds first.


## 2. Key Changes in Material 3 1.5.0 & Compose 1.13

- **Experimental footprint reduced by 52%**: 120 down to 58 experimental annotations.
- **Material 2 deprecated**: `androidx.compose.material` is deprecated and no longer actively maintained; every core M2 component has a stable M3 equivalent.
- **Graduated core APIs**: `BottomAppBar`, `TopAppBar` / `MediumFlexibleTopAppBar` / `LargeTopAppBar` / `TwoRowsTopAppBar`, `TopAppBarScrollBehavior`, `SearchBarState`, stateful `Slider`/`RangeSlider`, multi-browse/uncontained `Carousel`, `LocalRippleThemeConfiguration`, `BasicAlertDialog`.
- **New stable Expressive components**: `ToggleButton` family, `SplitButton`, `ButtonGroup` (`Modifier.animateWidth(compressionLimit = ...)`), shape-morphing `FilterChip`/`InputChip`, `SelectableDropdownMenuItem`/`CheckableDropdownMenuItem`, `DropdownMenuPopupPositionProvider`, `VibrantTimePickerDialog`, `ScrollField`, standalone `Scrim`, `WavyLinearProgressIndicator`/`WavyCircularProgressIndicator`, `FloatingToolbar`/`HorizontalFloatingToolbar`, segmented `ListItem`, `ExpandedDockedSearchBarWithGap`, `ExpandedFullScreenContainedSearchBar`.
- **Theming & Typography**: `MaterialTheme.expressiveLightColorScheme`, single `MaterialTheme.LocalMaterialTheme.current` composition local (deprecating `LocalMotionScheme`), `Typography(fontFamily = ...)` brand font propagation, `Text(color = ColorProducer)` draw-phase color updates without recomposition.
- **Remaining experimental APIs**: `LoadingIndicator` / `ContainedLoadingIndicator`, `MaterialShapes` (35 normalized `RoundedPolygon`s), `AppBarWithSearch`, `Modifier.carouselParallaxScrollEffect`, standalone static `Sheet`.

## 3. Breaking Changes & Migration Checklist

1. **Stateful `Slider` & `RangeSlider`**: stateless overloads deprecated; `onValueChange` is required in the composable layer; `RangeSliderState` renames `activeRangeStart`/`activeRangeEnd` to `startValue`/`endValue`.
2. **`SecureTextField`**: default obfuscation changed from `TextObfuscationMode.RevealLastTyped` to `TextObfuscationMode.System`.
3. **`BottomSheet`**: `ModalBottomSheet` and `BottomSheetScaffold` no longer auto-drop `PartiallyExpanded` via height heuristics; control explicitly via `rememberBottomSheetState`.
4. **`TopAppBarDefaults` scroll behaviors**: take `ScrollableState` and rename `isAtTop` to `isAtStart`.
5. **Motion local**: replace `LocalMotionScheme.current` with `MaterialTheme.motionScheme`.

## 4. Pure WGPU / WGSL Implementation in `fw`

- Shader: `crates/fw-wayland-wgpu/shaders/fw_m3_expressive.wgsl`
- Rust scene & spring engine: `crates/fw-wayland-wgpu/src/m3_expressive.rs` (`#![forbid(unsafe_code)]`)
- Implements all 35 `MaterialShapes` SDFs + `MorphPolygonShape`, animated `WavyLinearProgressIndicator` and `WavyCircularProgressIndicator` SDFs, `material3-ripple:1.5.0` inset focus rings + radial ripples, `StatefulSlider`, `SplitButton`/`FloatingToolbar`/`SegmentedListItem`, and Compose 1.13 progressive blur gradients in pure WGSL.

## 5. Parametric Prototyping (`2026-11-16`)

- **Source**: `https://m3.material.io/blog/enhance-your-component-designs-with-parametric-prototyping` (Reed Hollett & Tiffany Wan, `2026-11-16`; controls generated by an AI coding agent using Material switches, sliders, button groups, lists, and buttons for expressive time pickers & scroll fields).
- **6 Practices Implemented in `fw`**:
  1. **Map sweet spots**: `FwParametricEngine.mapSweetSpotGrid` (`ParamRange` min/max + sweet-spot bounds).
  2. **Fine-tune parameters**: Decoupled `FwComponentParams` (corner radii, padding, type scale, elevation, spring damping/stiffness, wave amplitude/wavelength, focus ring inset).
  3. **Group/tether parameters**: `enforceConstraints` in Kotlin (`FwParametricEngine`) and Rust (`M3ParametricConfig`) keeps inner corner radii, padding, and bounds coherent.
  4. **Turn designs into specs**: `FwSpecOverlay` (`showKeylines`, `showColorTokens`, `showSpatialMeasurements`) + WGPU `Spec.Keyline.*` overlay instances.
  5. **Presets & JSON export**: `FwParametricPreset` + `exportPresetJson` / `importPresetJson`.
  6. **Randomize to break bias**: Deterministic seeded randomization (`FwParametricEngine.randomize` / `M3ParametricConfig::randomize_seeded`).
- **Files**: `core/kotlin/fw-material3/src/main/kotlin/com/aphrody/fw/material3/FwParametricStudio.kt`, `crates/fw-wayland-wgpu/src/m3_expressive.rs`.
