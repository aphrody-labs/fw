---
name: kotlin-compose-m3
description: Compose Material 3 1.5.0 stable (Expressive), Jetpack Compose 1.13 (BOM 2026.11.01), pure WGPU/WGSL M3 renderer, Android CLI, official Android skills, Kotlin LSP, and Tree-sitter Kotlin for fw and aphrody-labs.
---

# Kotlin, Compose Material 3 1.5.0 & Pure WGPU Toolkit

## 1. Verified Stack (Nov 2026)

- **Compose BOM**: `androidx.compose:compose-bom:2026.11.01` (requires Kotlin `2.2+`, `compileSdk 37.1`)
- **Material 3**: `androidx.compose.material3:material3:1.5.0`, `material3-ripple:1.5.0`, `material3-window-size-class:1.5.0`, `material3-adaptive-navigation-suite:1.5.0`, `androidx.compose.material3.adaptive:adaptive:1.3.0`
- **Target**: Android library on androidx material3 (the only target of 1.5.0). No stable Compose Multiplatform port is based on 1.5.0 (CMP `1.12.2` ships material3 `1.12.0-alpha03` = androidx `1.5.0-alpha22`; `1.13.0-rc01` = androidx `1.5.0-rc01`).
- **Theme (owned by `aphrody-labs/ay`)**: `com.aphrody:aphrody-theme` (`AphrodyTheme`, `AphrodyLightColors`, `AphrodyDarkColors`), generated from the brand tokens and published to GitHub Packages; coordinates in `product.toml` [kotlin]. fw never redefines colours, shapes or motion.
- **Aphrody Labs Libraries** (`core/kotlin/`, Gradle):
  - `com.aphrody.fw:fw-core:4.2.0`
  - `com.aphrody.fw:fw-material3:4.2.0`
  - `com.aphrody.fw:fw-rust-ffi:4.2.0`
- **Gate**: `bun tools/kotlin-check.ts [--ay <ay checkout>]` (Gradle and cargo run on the build host).
- **Pure WGPU / WGSL M3 1.5.0**: `core/webgpu/fw_m3_expressive.wgsl` + `crates/fw-wayland-wgpu/src/m3_expressive.rs` (35 `MaterialShapes` SDFs, morphing, wavy linear/circular progress, inset focus rings, progressive blur).

## 2. Installed CLI, LSP, Skills & AST Tools

- **Android CLI**: `android --version` (`1.1.16919734`), `android preview`, `android build`, `android docs`, `android skills update --all`
- **Kotlin LSP**: `%LOCALAPPDATA%\kotlin-lsp\bin\intellij-server.exe --stdio` (`v271.2085.0`)
- **Tree-sitter Kotlin**: `tree-sitter-kotlin-ng 1.1.0` embedded in `native/fw-abi` (`fw_parse_summary("kotlin", ...)`, `fw_ast_symbols("kt", ...)`)

## 3. Compose Material 3 1.5.0 Best Practices

1. Never add `androidx.compose.material` (Material 2 is deprecated).
2. Use stateful `Slider(state = remember { SliderState(...) }, onValueChange = { ... })` and `RangeSliderState(startValue, endValue)`.
3. Set brand typography globally via `Typography(fontFamily = brandFont)`.
4. Use `Text(color = ColorProducer { ... })` for animated text colors without recomposition.
5. Prefer `MaterialTheme.motionScheme` over deprecated `LocalMotionScheme` and `material3-ripple:1.5.0` inset focus rings for keyboard accessibility.
6. Colour contrast (`>= 4.5:1` WCAG AA) is proven in `aphrody-labs/ay` (`AphrodyColorSchemeTest`); for other pairs use Rust `fw_m3_contrast_ratio`.
   Wrap UI in `FwExpressiveTheme` (it is `AphrodyTheme`); read `MaterialTheme.colorScheme` / `shapes` / `motionScheme`, never literal colours.
7. For complex components (time pickers, scroll fields, toolbars, tabs), use `FwParametricEngine` / `FwParametricControlPanel` (Kotlin) or `M3ParametricConfig` (pure WGPU) to map sweet-spot grids, tether dependent parameters, toggle keyline/token spec overlays, export JSON presets, and run seeded randomization.
