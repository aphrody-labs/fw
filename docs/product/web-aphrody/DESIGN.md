---
name: Aphrody web
brand: Material 3 (no custom brand)
source: Material 3 as shipped in androidx Compose material3 (read at androidx/androidx androidx-main ebe7964173, 2026-11-21), the M3 Figma Design Kit, and android/nowinandroid core/designsystem. Nothing is taken from the inherited aphrody tokens.
typography:
  family: "Google Sans Flex"
  code: "Google Sans Code"
  # TypeScaleTokens.kt: size / line height / tracking, in sp (= CSS px at 1x)
  displayLarge: { size: 57, line: 64, tracking: -0.25 }
  displayMedium: { size: 45, line: 52, tracking: 0 }
  displaySmall: { size: 36, line: 44, tracking: 0 }
  headlineLarge: { size: 32, line: 40, tracking: 0 }
  headlineMedium: { size: 28, line: 36, tracking: 0 }
  headlineSmall: { size: 24, line: 32, tracking: 0 }
  titleLarge: { size: 22, line: 28, tracking: 0 }
  titleMedium: { size: 16, line: 24, tracking: 0.15 }
  titleSmall: { size: 14, line: 20, tracking: 0.1 }
  bodyLarge: { size: 16, line: 24, tracking: 0.5 }
  bodyMedium: { size: 14, line: 20, tracking: 0.25 }
  bodySmall: { size: 12, line: 16, tracking: 0.4 }
  labelLarge: { size: 14, line: 20, tracking: 0.1 }
  labelMedium: { size: 12, line: 16, tracking: 0.5 }
  labelSmall: { size: 11, line: 16, tracking: 0.5 }
rounded:
  # ShapeTokens.kt, dp
  extraSmall: 4
  large: 16
  largeIncreased: 20
  extraLarge: 28
  extraLargeIncreased: 32
  extraExtraLarge: 48
  full: 9999
motion:
  # ExpressiveMotionTokens.kt: spring damping ratio / stiffness
  spatialFast: { damping: 0.6, stiffness: 800 }
  spatialDefault: { damping: 0.8, stiffness: 380 }
  spatialSlow: { damping: 0.8, stiffness: 200 }
  effectsFast: { damping: 1.0, stiffness: 3800 }
  effectsDefault: { damping: 1.0, stiffness: 1600 }
  effectsSlow: { damping: 1.0, stiffness: 800 }
colors:
  # The brand IS Material 3 (owner, 2026-11-21): the M3 baseline scheme exactly as androidx Compose ships it
  # (ColorLightTokens/ColorDarkTokens through PaletteTokens), synced from upstream, never a custom seed.
  m3:
    light: { primary: "#6750A4", onPrimary: "#FFFFFF", primaryContainer: "#EADDFF", secondary: "#625B71", tertiary: "#7D5260", surface: "#FEF7FF", surfaceContainer: "#F3EDF7", onSurface: "#1D1B20", outline: "#79747E", error: "#B3261E" }
    dark: { primary: "#D0BCFF", onPrimary: "#381E72", primaryContainer: "#4F378B", secondary: "#CCC2DC", tertiary: "#EFB8C8", surface: "#141218", surfaceContainer: "#211F26", onSurface: "#E6E0E9", outline: "#938F99", error: "#F2B8B5" }
---

# Design System: Aphrody web (web.aphrody.com)

## 1. Visual theme and atmosphere

The product is shown, not described: a live desktop in a frame, then sections shaped by their content. The surfaces are calm and tonal, as in Now in Android's `NiaTheme` (light and dark default schemes plus an optional dynamic scheme). One expressive moment is allowed: the desktop booting in the hero frame.

## 2. Color palette and roles

All colours are M3 roles. They are never typed into components; components read the role (`--md-sys-color-<role>`).
- Primary: the single main action per view (install).
- Tertiary: the live-desktop frame accent only.
- Surface and its containers: page and section backgrounds, layered by elevation through tonal containers, not shadows.

These are the Material 3 baseline roles, synced from androidx; no custom seed exists. Dynamic color (material-color-utilities) is allowed only where M3 itself uses it (a user wallpaper or seed on the device), never as a brand.

## 3. Typography rules

- Google Sans Flex carries the whole M3 type scale above.
- The one hero headline may use the width and optical-size axes; body stays at bodyLarge (16/24).
- Google Sans Code is used only for real commands. Lines stay under 72 characters.

## 4. Component stylings

| Component | Styling |
|---|---|
| Buttons | M3 filled button for the one primary action, tonal or outlined for secondary; shape `full`; 40 dp minimum height with a 48 dp touch target; label in labelLarge. |
| Containers | Corner from the shape scale by role: extraLarge (28) for the hero frame, large (16) for media, extraSmall (4) for chips. Never one radius everywhere. |
| Inputs | M3 filled text field, outline role on focus. |

## 5. Layout principles

- **Window size classes** (Compose adaptive and NiA): compact up to 600 dp, then a navigation bar; medium 600–840, then a navigation rail; expanded 840 and up, then a rail or drawer plus list-detail.
- **Spacing:** a 4 dp grid.
- **Motion:** spatial springs (spatialDefault 0.8/380) for movement, effects springs for colour and opacity. Respect reduced motion.
