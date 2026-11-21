//! Material 3 color roles computed from one source color, in Rust.
//!
//! The HCT color space, tonal palettes and dynamic schemes come from `material-colors`, a Rust port of
//! `material-foundation/material-color-utilities` (tracked in `upstreams.toml`). This module turns a
//! seed into the named `--md-sys-color-*` roles that the web UI, Compose and GPUI themes consume, so
//! no palette is copied by hand.

use material_colors::color::Rgb;
use material_colors::dynamic_color::{DynamicScheme, Variant};

/// Scheme variants offered to callers (a subset of `material_colors::dynamic_color::Variant`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SchemeVariant {
    /// Default Material You variant: muted secondary and tertiary around the seed hue.
    TonalSpot,
    /// Keeps the seed chroma for the primary palette.
    Fidelity,
    /// Higher-chroma palettes.
    Vibrant,
    /// Hue-shifted, playful palettes.
    Expressive,
    /// Near-grey palettes.
    Neutral,
    /// Greyscale.
    Monochrome,
    /// Primary palette keeps the seed chroma; tertiary from a temperature analogue (MCU `Content`).
    Content,
    /// AOSP `RAINBOW`: chromatic accents on a greyscale neutral. Not for wallpaper extraction.
    Rainbow,
    /// AOSP `FRUIT_SALAD`: two-tone accents with hue rotation. Not for wallpaper extraction.
    FruitSalad,
}

impl SchemeVariant {
    /// Slugs accepted by [`Self::from_slug`], kebab-case as in `product.toml`.
    pub const SLUGS: &[&str] = &[
        "tonal-spot",
        "vibrant",
        "expressive",
        "spritz",
        "rainbow",
        "fruit-salad",
        "fidelity",
        "content",
        "neutral",
        "monochrome",
    ];

    /// Parses the kebab-case slug used in `product.toml` (`tonal-spot`, `fidelity`, ...), or an AOSP
    /// `android.theme.customization.theme_style` value (`TONAL_SPOT`, `VIBRANT`, `EXPRESSIVE`, `SPRITZ`,
    /// `RAINBOW`, `FRUIT_SALAD`; source.android.com/docs/core/display/dynamic-color). AOSP's `SPRITZ`
    /// is the low-vibrancy style that material-color-utilities ships as `Neutral`.
    pub fn from_slug(slug: &str) -> Option<Self> {
        Some(
            match slug.trim().to_ascii_lowercase().replace('_', "-").as_str() {
                "tonal-spot" => Self::TonalSpot,
                "fidelity" => Self::Fidelity,
                "vibrant" => Self::Vibrant,
                "expressive" => Self::Expressive,
                "neutral" | "spritz" => Self::Neutral,
                "monochrome" => Self::Monochrome,
                "content" => Self::Content,
                "rainbow" => Self::Rainbow,
                "fruit-salad" => Self::FruitSalad,
                _ => return None,
            },
        )
    }

    fn upstream(self) -> Variant {
        match self {
            Self::TonalSpot => Variant::TonalSpot,
            Self::Fidelity => Variant::Fidelity,
            Self::Vibrant => Variant::Vibrant,
            Self::Expressive => Variant::Expressive,
            Self::Neutral => Variant::Neutral,
            Self::Monochrome => Variant::Monochrome,
            Self::Content => Variant::Content,
            Self::Rainbow => Variant::Rainbow,
            Self::FruitSalad => Variant::FruitSalad,
        }
    }
}

macro_rules! roles {
    ($($css:literal => $method:ident,)*) => {
        /// CSS role names in `--md-sys-color-<name>` order, as published by Material.
        pub const ROLE_NAMES: &[&str] = &[$($css,)*];

        fn resolve(scheme: &DynamicScheme) -> Vec<(&'static str, u32)> {
            vec![$(($css, pack(scheme.$method())),)*]
        }
    };
}

roles! {
    "primary" => primary,
    "on-primary" => on_primary,
    "primary-container" => primary_container,
    "on-primary-container" => on_primary_container,
    "inverse-primary" => inverse_primary,
    "secondary" => secondary,
    "on-secondary" => on_secondary,
    "secondary-container" => secondary_container,
    "on-secondary-container" => on_secondary_container,
    "tertiary" => tertiary,
    "on-tertiary" => on_tertiary,
    "tertiary-container" => tertiary_container,
    "on-tertiary-container" => on_tertiary_container,
    "error" => error,
    "on-error" => on_error,
    "error-container" => error_container,
    "on-error-container" => on_error_container,
    "background" => background,
    "on-background" => on_background,
    "surface" => surface,
    "on-surface" => on_surface,
    "surface-variant" => surface_variant,
    "on-surface-variant" => on_surface_variant,
    "surface-dim" => surface_dim,
    "surface-bright" => surface_bright,
    "surface-container-lowest" => surface_container_lowest,
    "surface-container-low" => surface_container_low,
    "surface-container" => surface_container,
    "surface-container-high" => surface_container_high,
    "surface-container-highest" => surface_container_highest,
    "surface-tint" => surface_tint,
    "outline" => outline,
    "outline-variant" => outline_variant,
    "inverse-surface" => inverse_surface,
    "inverse-on-surface" => inverse_on_surface,
    "shadow" => shadow,
    "scrim" => scrim,
    "primary-fixed" => primary_fixed,
    "primary-fixed-dim" => primary_fixed_dim,
    "on-primary-fixed" => on_primary_fixed,
    "on-primary-fixed-variant" => on_primary_fixed_variant,
    "secondary-fixed" => secondary_fixed,
    "secondary-fixed-dim" => secondary_fixed_dim,
    "on-secondary-fixed" => on_secondary_fixed,
    "on-secondary-fixed-variant" => on_secondary_fixed_variant,
    "tertiary-fixed" => tertiary_fixed,
    "tertiary-fixed-dim" => tertiary_fixed_dim,
    "on-tertiary-fixed" => on_tertiary_fixed,
    "on-tertiary-fixed-variant" => on_tertiary_fixed_variant,
}

fn pack(c: Rgb) -> u32 {
    (u32::from(c.red) << 16) | (u32::from(c.green) << 8) | u32::from(c.blue)
}

/// Every color role for `seed` (`0xRRGGBB`) in light or dark, at standard contrast.
pub fn scheme_roles(seed: u32, variant: SchemeVariant, dark: bool) -> Vec<(&'static str, u32)> {
    let scheme = DynamicScheme::by_variant(Rgb::from_u32(seed), &variant.upstream(), dark, None);
    resolve(&scheme)
}

/// One role as `0xRRGGBB`, or `None` for an unknown role name.
pub fn role(seed: u32, variant: SchemeVariant, dark: bool, name: &str) -> Option<u32> {
    scheme_roles(seed, variant, dark)
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, c)| c)
}

/// CSS custom properties for both modes: light on `:root`, dark under `prefers-color-scheme`
/// unless `[data-theme="light"]` is set, and forced dark on `[data-theme="dark"]`.
pub fn css_custom_properties(seed: u32, variant: SchemeVariant) -> String {
    let block = |dark: bool| -> String {
        scheme_roles(seed, variant, dark)
            .into_iter()
            .map(|(n, c)| format!("  --md-sys-color-{n}: #{c:06x};\n"))
            .collect()
    };
    let (light, dark) = (block(false), block(true));
    let indented: String = dark.lines().map(|l| format!("  {l}\n")).collect();
    format!(
        ":root {{\n  color-scheme: light;\n{light}}}\n@media (prefers-color-scheme: dark) {{\n  :root:not([data-theme=\"light\"]) {{\n    color-scheme: dark;\n{indented}  }}\n}}\n:root[data-theme=\"dark\"] {{\n  color-scheme: dark;\n{dark}}}\n"
    )
}

/// First line of the generated block in a CSS file (the rest of that line is a comment).
pub const CSS_BEGIN: &str = "/* m3-colors:begin";
/// Line that closes the generated block.
pub const CSS_END: &str = "/* m3-colors:end */";

/// Replaces whatever sits between the `CSS_BEGIN` line and the `CSS_END` line of `file` with
/// `generated`. Returns `None` when the markers are missing or out of order.
pub fn splice_css(file: &str, generated: &str) -> Option<String> {
    let begin = file.find(CSS_BEGIN)?;
    let body_start = begin + file[begin..].find('\n')? + 1;
    let end = body_start + file[body_start..].find(CSS_END)?;
    Some(format!(
        "{}{}{}",
        &file[..body_start],
        generated,
        &file[end..]
    ))
}

/// AOSP rejects source colors below this CAM16 chroma (source.android.com/docs/core/display/material).
pub const MIN_SEED_CHROMA: f64 = 5.0;

/// The seed AOSP falls back to when no candidate color is usable (`0xFF1B6EF3`).
pub const AOSP_FALLBACK_SEED: u32 = 0x1B6EF3;

/// HCT chroma of a seed (HCT uses CAM16 hue and chroma).
pub fn seed_chroma(seed: u32) -> f64 {
    material_colors::hct::Hct::new(Rgb::from_u32(seed)).get_chroma()
}

/// The seed to build a scheme from, following AOSP: a seed under [`MIN_SEED_CHROMA`] gives a washed-out
/// scheme, so it is replaced by [`AOSP_FALLBACK_SEED`]. `Monochrome` is greyscale by design and keeps it.
pub fn usable_seed(seed: u32, variant: SchemeVariant) -> u32 {
    if variant != SchemeVariant::Monochrome && seed_chroma(seed) < MIN_SEED_CHROMA {
        AOSP_FALLBACK_SEED
    } else {
        seed
    }
}

/// Parses `#rrggbb` (or `rrggbb`) into `0xRRGGBB`.
pub fn parse_seed(hex: &str) -> Option<u32> {
    let h = hex.trim().trim_start_matches('#');
    (h.len() == 6)
        .then(|| u32::from_str_radix(h, 16).ok())
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Aphrody brand seed; expected values are aphrody-labs/ay brand/tokens/m3-tokens.json (tonal-spot, spec 2021).
    const APHRODY_SEED: u32 = 0xe2dbad;

    #[test]
    fn aosp_theme_styles_parse() {
        assert_eq!(
            SchemeVariant::from_slug("TONAL_SPOT"),
            Some(SchemeVariant::TonalSpot)
        );
        assert_eq!(
            SchemeVariant::from_slug("SPRITZ"),
            Some(SchemeVariant::Neutral)
        );
        assert_eq!(
            SchemeVariant::from_slug("FRUIT_SALAD"),
            Some(SchemeVariant::FruitSalad)
        );
        assert_eq!(
            SchemeVariant::from_slug("rainbow"),
            Some(SchemeVariant::Rainbow)
        );
        assert_eq!(SchemeVariant::from_slug("unknown"), None);
        for slug in SchemeVariant::SLUGS {
            let v = SchemeVariant::from_slug(slug).unwrap();
            assert_eq!(
                scheme_roles(APHRODY_SEED, v, false).len(),
                ROLE_NAMES.len(),
                "{slug}"
            );
        }
    }

    #[test]
    fn low_chroma_seeds_fall_back_like_aosp() {
        assert!(
            seed_chroma(APHRODY_SEED) >= MIN_SEED_CHROMA,
            "the brand seed is usable"
        );
        assert_eq!(
            usable_seed(APHRODY_SEED, SchemeVariant::TonalSpot),
            APHRODY_SEED
        );
        assert_eq!(
            usable_seed(0x808080, SchemeVariant::TonalSpot),
            AOSP_FALLBACK_SEED
        );
        assert_eq!(usable_seed(0x808080, SchemeVariant::Monochrome), 0x808080);
    }

    #[test]
    fn aphrody_seed_reproduces_the_brand_tokens() {
        let light = |n| role(APHRODY_SEED, SchemeVariant::TonalSpot, false, n).unwrap();
        let dark = |n| role(APHRODY_SEED, SchemeVariant::TonalSpot, true, n).unwrap();
        assert_eq!(light("primary"), 0x676013);
        assert_eq!(light("primary-container"), 0xefe58b);
        assert_eq!(light("tertiary"), 0x406653);
        assert_eq!(light("surface"), 0xfef9eb);
        assert_eq!(light("surface-container"), 0xf3eee0);
        assert_eq!(light("outline"), 0x7a7768);
        assert_eq!(dark("primary"), 0xd3c972);
        assert_eq!(dark("surface"), 0x15140c);
        assert_eq!(dark("on-surface"), 0xe7e2d5);
        assert_eq!(dark("surface-container-highest"), 0x37352c);
    }

    #[test]
    fn every_role_is_resolved_once() {
        let roles = scheme_roles(APHRODY_SEED, SchemeVariant::Vibrant, true);
        assert_eq!(roles.len(), ROLE_NAMES.len());
        assert!(role(APHRODY_SEED, SchemeVariant::TonalSpot, false, "not-a-role").is_none());
    }

    #[test]
    fn splice_replaces_only_the_block() {
        let file = "a\n/* m3-colors:begin note */\nold\n/* m3-colors:end */\nb\n";
        assert_eq!(
            splice_css(file, "new\n").unwrap(),
            "a\n/* m3-colors:begin note */\nnew\n/* m3-colors:end */\nb\n"
        );
        assert!(splice_css("no markers", "x").is_none());
        assert_eq!(parse_seed("#E2DBAD"), Some(APHRODY_SEED));
        assert_eq!(parse_seed("#e2db"), None);
        assert_eq!(
            SchemeVariant::from_slug("tonal-spot"),
            Some(SchemeVariant::TonalSpot)
        );
    }

    #[test]
    fn css_has_both_modes() {
        let css = css_custom_properties(APHRODY_SEED, SchemeVariant::TonalSpot);
        assert!(css.contains("--md-sys-color-primary: #676013;"));
        assert!(css.contains("--md-sys-color-primary: #d3c972;"));
        assert!(css.contains(":root[data-theme=\"dark\"]"));
    }
}
