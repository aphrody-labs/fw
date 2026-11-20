// SPDX-License-Identifier: Apache-2.0
//! Pure-Rust embedded brand media, Zukan character dataset, Material 3 Symbols CSS,
//! lossless PNGs, multi-size ICOs, SVGs, and animated GIF89a for every `ay` & `aphrody-labs` product.

/// Structured EN/JA/FR Zukan codex data for Byron Love (Aphrody).
pub const ZUKAN_JSON: &str = include_str!("../../../../brand/character/zukan.json");
/// Material 3 Symbols & Aphrody era iconography CSS.
pub const M3_SYMBOLS_CSS: &str = include_str!("../../../../brand/tokens/m3-symbols.css");
/// Animated GIF89a cycling through IE1 (#185), GO (#2625), and Ares (#4526).
pub const ANIMATED_ERAS_GIF: &[u8] = include_bytes!("../../../../brand/assets/zukan/aphrody-eras.gif");

pub const IE1_SVG: &str = include_str!("../../../../brand/assets/zukan/aphrody-ie1.svg");
pub const GO_SVG: &str = include_str!("../../../../brand/assets/zukan/aphrody-go.svg");
pub const ARES_SVG: &str = include_str!("../../../../brand/assets/zukan/aphrody-ares.svg");

pub const IE1_ICO: &[u8] = include_bytes!("../../../../brand/assets/zukan/aphrody-ie1.ico");
pub const GO_ICO: &[u8] = include_bytes!("../../../../brand/assets/zukan/aphrody-go.ico");
pub const ARES_ICO: &[u8] = include_bytes!("../../../../brand/assets/zukan/aphrody-ares.ico");

pub const IE1_LOSSLESS_PNG: &[u8] =
    include_bytes!("../../../../brand/assets/zukan/aphrody-ie1-lossless.png");
pub const GO_LOSSLESS_PNG: &[u8] =
    include_bytes!("../../../../brand/assets/zukan/aphrody-go-lossless.png");
pub const ARES_LOSSLESS_PNG: &[u8] =
    include_bytes!("../../../../brand/assets/zukan/aphrody-ares-lossless.png");

/// Product-level pure-Rust media binding across `ay.toml` products and `aphrody-labs` repos.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProductMedia {
    pub product: &'static str,
    pub svg: &'static str,
    pub ico: &'static [u8],
    pub png: &'static [u8],
    pub css_class: &'static str,
}

pub const PRODUCT_MEDIA: &[ProductMedia] = &[
    ProductMedia {
        product: "os",
        svg: IE1_SVG,
        ico: IE1_ICO,
        png: IE1_LOSSLESS_PNG,
        css_class: "aphrody-symbol-ie1",
    },
    ProductMedia {
        product: "browser",
        svg: GO_SVG,
        ico: GO_ICO,
        png: GO_LOSSLESS_PNG,
        css_class: "aphrody-symbol-go",
    },
    ProductMedia {
        product: "search",
        svg: ARES_SVG,
        ico: ARES_ICO,
        png: ARES_LOSSLESS_PNG,
        css_class: "aphrody-symbol-ares",
    },
    ProductMedia {
        product: "cloud",
        svg: IE1_SVG,
        ico: IE1_ICO,
        png: IE1_LOSSLESS_PNG,
        css_class: "aphrody-symbol-ie1",
    },
    ProductMedia {
        product: "craft",
        svg: GO_SVG,
        ico: GO_ICO,
        png: GO_LOSSLESS_PNG,
        css_class: "aphrody-symbol-go",
    },
    ProductMedia {
        product: "brand",
        svg: ARES_SVG,
        ico: ARES_ICO,
        png: ARES_LOSSLESS_PNG,
        css_class: "aphrody-symbol-ares",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_media_headers_are_valid() {
        assert!(ZUKAN_JSON.contains("aphrody.character.zukan"));
        assert!(M3_SYMBOLS_CSS.contains("--aphrody-era-ie1"));
        assert!(ANIMATED_ERAS_GIF.starts_with(b"GIF89a"));
        for pm in PRODUCT_MEDIA {
            assert!(pm.svg.contains("<svg"));
            assert!(pm.ico.starts_with(&[0, 0, 1, 0]));
            assert!(pm.png.starts_with(&[0x89, b'P', b'N', b'G']));
        }
    }
}
