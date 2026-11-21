# Design System: Jetpack Compose Glimmer (transparent screens)
**Project ID:** 2829566608476776032

Sources: [Designing for Transparent Screens](https://design.google/library/transparent-screens) (Google Design) and the Glimmer theme code in `androidx/androidx` (`xr/glimmer/glimmer/src/main/java/androidx/xr/glimmer`: `Colors.kt`, `Typography.kt`, `Shapes.kt`, `DepthEffectLevels.kt`, `ComponentSpacingValues.kt`), read 2026-11-21.

## 1. Visual Theme & Atmosphere

Calm, ambient and sparse. The interface lives on the lens of display AI glasses and appears about one meter away, so the real world is always the backdrop and every glance at the UI is a deliberate shift of focus. The display only adds light: black is not a color but full transparency, so the design paints with light on a void.

The mood is a quiet heads-up layer: neutral by default so it sits in harmony with whatever colors the world shows, high contrast so it stays legible against a bright sky or a dark room, and small enough to leave the world in view. It appears when needed and disappears when not. Nothing is decorative; every lit pixel costs attention, glare and battery.

## 2. Color Palette & Roles

Surfaces are always dark and content is always bright. Saturated phone colors fade into the scenery on an additive display, so the palette stays close to white and uses color only to point at what matters.

- **Void Black (#000000)** – background. Renders fully transparent on the glasses: the world shows through. Never use it as "dark text".
- **Smoked Graphite (#303030)** – surface for cards, buttons and list items. A dark "clean plate" behind content that keeps text legible over a busy world without emitting much light.
- **Glacier Blue (#9BBFFF)** – primary. Pale, light-heavy blue for the main action and focus; light enough to stay visible outdoors.
- **Signal Blue (#4C88E9)** – secondary. Deeper blue for secondary emphasis; use sparingly because darker colors add less light and read weaker.
- **Mint Glow (#63FEA8)** – positive. Confirmations and affirmative actions.
- **Coral Blush (#FFA7A0)** – negative. Destructive or declining actions, kept pale so it stays readable.
- **Bright White (#FFFFFF)** – content on surfaces (text and icons), computed from the surface color for maximum additive contrast.

Contrast is judged with the additive contrast ratio, which accounts for display brightness and environment brightness, rather than the usual screen contrast ratio.

## 3. Typography Rules

Google Sans Flex, tuned for glanceability at arm's length. Text size is reasoned in visual angle, not pixels: the minimum readable size is about 0.6 degrees, and every style sits comfortably above it. The optical size axis is pushed toward small sizes so counters in letters like a and e open up and the dots of i and j move away from the stems; this also widens spacing, so letter-spacing stays at 0 in code.

Weights are heavier than on phones and nothing is thin:

- **Title Large** – 30sp, line height 36sp, weight 750.
- **Title Medium** – 24sp, line height 28sp, weight 750.
- **Title Small** – 20sp, line height 28sp, weight 750.
- **Body Large** – 30sp, line height 36sp, weight 520.
- **Body Medium** – 24sp, line height 36sp, weight 520.
- **Body Small** – 20sp, weight 520.
- **Caption** – 18sp, line height 24sp, weight 650; the smallest style, used rarely.

Keep copy short enough to read in one glance; prefer one line of title and one or two lines of body.

## 4. Component Stylings

* **Buttons:** Fully pill-shaped (circular large shape) Smoked Graphite fills with bright white labels; the primary action may take Glacier Blue. Buttons can overlap the edge of a card to show they belong to it. Focus shows as a wider inner border with a highlight, and input feedback (voice or gesture) is immediate.
* **Cards/Containers:** Generously rounded corners, 24dp on small surfaces and 36dp on medium ones, filled with Smoked Graphite and outlined by a thin inner border that marks the edge against the world. Never light-filled: bright surfaces become glaring blocks of light and bleed into nearby text (halation).
* **Lists and title chips:** Same dark surface and bright content; one idea per item.
* **Inputs/Forms:** No dense forms. Input is voice and gesture; a voice input indicator and focus rings confirm that the system is listening and what is selected.
* **Notifications:** Arrive from the periphery over almost two seconds so they invite attention instead of popping in; a typical 500ms fade is too abrupt here.

## 5. Layout Principles

The display covers only a small part of the field of view, so layouts are a single focused column of a few elements, never a full-screen grid. Spacing steps are 6, 8, 12, 16 and 20dp (extra small to extra large).

Depth replaces elevation: because light cannot be subtracted, hierarchy comes from dark, rich shadows that occlude what is behind a component. Five depth levels each combine a wide shadow at 90% opacity with a tighter core shadow, growing from 12dp radius and 6dp spread at level 1 to 56dp radius and 32dp spread at level 5. System controls such as the volume slider use an exaggerated level to read as system-level.

Motion guides rather than distracts: slow, ambient transitions for anything the user did not ask for, instant responses to anything they did.

**Do:** dark surfaces with bright content; bold, optically sized type; neutral palette with color only on what needs attention; slow ambient motion and instant feedback.
**Don't:** light surfaces with dark text; thin or small type; saturated phone colors; large lit areas; fast pop-in for unprompted content.
