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
| `org.jetbrains.compose.material3:material3` | `1.13.0-rc01` | RC | Compose Multiplatform Material 3 (`compose-gradle-plugin` `1.12.2` stable / `1.13.0-rc01`) |
| `dev.aphrody.fw:fw-core` | `4.2.0` | Local Maven | Aphrody Labs Kotlin SDK (`core/kotlin/fw-core`) |
| `dev.aphrody.fw:fw-material3` | `4.2.0` | Local Maven | Aphrody Labs M3 1.5.0 Expressive tokens, components, and `M3Catalog` (`core/kotlin/fw-material3`) |
| `dev.aphrody.fw:fw-rust-ffi` | `4.2.0` | Local Maven | Bidirectional JDK 25 Panama FFM Kotlin <-> Rust bridge (`core/kotlin/fw-rust-ffi`) |

## 2. Key Changes in Material 3 1.5.0 & Compose 1.13

- **Experimental footprint reduced by 52%**: 120 down to 58 experimental annotations.
- **Material 2 deprecated**: `androidx.compose.material` is deprecated and no longer actively maintained; every core M2 component has a stable M3 equivalent.
- **Graduated core APIs**: `BottomAppBar`, `TopAppBar` / `MediumFlexibleTopAppBar` / `LargeTopAppBar` / `TwoRowsTopAppBar`, `TopAppBarScrollBehavior`, `SearchBarState`, stateful `Slider`/`RangeSlider`, multi-browse/uncontained `Carousel`, `LocalRippleThemeConfiguration`, `BasicAlertDialog`.
- **New stable Expressive components**: `ToggleButton` family, `SplitButton`, `ButtonGroup` (`Modifier.animateWidth(compressionLimit = ...)`), shape-morphing `FilterChip`/`InputChip`, `SelectableDropdownMenuItem`/`CheckableDropdownMenuItem`, `DropdownMenuPopupPositionProvider`, `VibrantTimePickerDialog`, `ScrollField`, standalone `Scrim`, `WavyLinearProgressIndicator`/`WavyCircularProgressIndicator`, `FloatingToolbar`/`HorizontalFloatingToolbar`, segmented `ListItem`, `ExpandedDockedSearchBarWithGap`, `ExpandedFullScreenContainedSearchBar`.
- **Theming & Typography**: `MaterialTheme.expressiveLightColorScheme`, single `MaterialTheme.LocalMaterialTheme.current` composition local (deprecating `LocalMotionScheme`), `Typography(defaultFontFamily = ...)` brand font propagation, `Text(color = ColorProducer)` draw-phase color updates without recomposition.
- **Remaining experimental APIs**: `LoadingIndicator` / `ContainedLoadingIndicator`, `MaterialShapes` (35 normalized `RoundedPolygon`s), `AppBarWithSearch`, `Modifier.carouselParallaxScrollEffect`, standalone static `Sheet`.

## 3. Breaking Changes & Migration Checklist

1. **Stateful `Slider` & `RangeSlider`**: stateless overloads deprecated; `onValueChange` is required in the composable layer; `RangeSliderState` renames `activeRangeStart`/`activeRangeEnd` to `startValue`/`endValue`.
2. **`SecureTextField`**: default obfuscation changed from `TextObfuscationMode.RevealLastTyped` to `TextObfuscationMode.System`.
3. **`BottomSheet`**: `ModalBottomSheet` and `BottomSheetScaffold` no longer auto-drop `PartiallyExpanded` via height heuristics; control explicitly via `rememberBottomSheetState`.
4. **`TopAppBarDefaults` scroll behaviors**: take `ScrollableState` and rename `isAtTop` to `isAtStart`.
5. **Motion local**: replace `LocalMotionScheme.current` with `MaterialTheme.motionScheme`.

## 4. Pure WGPU / WGSL Implementation in `fw`

- Shader: `core/webgpu/fw_m3_expressive.wgsl`
- Rust scene & spring engine: `crates/fw-wayland-wgpu/src/m3_expressive.rs` (`#![forbid(unsafe_code)]`)
- Implements all 35 `MaterialShapes` SDFs + `MorphPolygonShape`, animated `WavyLinearProgressIndicator` and `WavyCircularProgressIndicator` SDFs, `material3-ripple:1.5.0` inset focus rings + radial ripples, `StatefulSlider`, `SplitButton`/`FloatingToolbar`/`SegmentedListItem`, and Compose 1.13 progressive blur gradients in pure WGSL.

## 5. Parametric Prototyping (`2026-11-16`)

- **Source**: `https://m3.material.io/blog/enhance-your-component-designs-with-parametric-prototyping` (Reed Hollett & Tiffany Wan, `2026-11-16`; controls generated with Antigravity using Material switches, sliders, button groups, lists, and buttons for expressive time pickers & scroll fields).
- **6 Practices Implemented in `fw`**:
  1. **Map sweet spots**: `FwParametricEngine.mapSweetSpotGrid` (`ParamRange` min/max + sweet-spot bounds).
  2. **Fine-tune parameters**: Decoupled `FwComponentParams` (corner radii, padding, type scale, elevation, spring damping/stiffness, wave amplitude/wavelength, focus ring inset).
  3. **Group/tether parameters**: `enforceConstraints` in Kotlin (`FwParametricEngine`) and Rust (`M3ParametricConfig`) keeps inner corner radii, padding, and bounds coherent.
  4. **Turn designs into specs**: `FwSpecOverlay` (`showKeylines`, `showColorTokens`, `showSpatialMeasurements`) + WGPU `Spec.Keyline.*` overlay instances.
  5. **Presets & JSON export**: `FwParametricPreset` + `exportPresetJson` / `importPresetJson`.
  6. **Randomize to break bias**: Deterministic seeded randomization (`FwParametricEngine.randomize` / `M3ParametricConfig::randomize_seeded`).
- **Files**: `core/kotlin/fw-material3/src/main/kotlin/dev/aphrody/fw/material3/FwParametricStudio.kt`, `crates/fw-wayland-wgpu/src/m3_expressive.rs`.
