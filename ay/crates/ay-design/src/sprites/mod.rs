// SPDX-License-Identifier: Apache-2.0
//! Sprite atlas to web sheet: CSS (image or mask), SVG `<symbol>` and a JSON manifest.
//!
//! An atlas is a single image and a list of named regions (name plus rectangle). This crate maps that
//! description as is onto the forms the web expects:
//!
//! - **CSS**: one class per region, a negative `background-position` on the full atlas. This is the
//!   cheapest form: one cached image, no slicing, the browser does the rest.
//! - **SVG**: one `<symbol>` per region around a single `data:` `<image>`. The file is self-contained
//!   (no external resource), so it can be pasted into a page or imported into a component.
//!
//! In both cases the rectangles are **copied**, never recomputed.
//!
//! The crate knows no input format: a caller supplies the assembled atlas image and the regions, from
//! whatever container format they come. It only needs `alloc` (the module uses no `std` item), WebAssembly included.

extern crate alloc;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// An atlas region ready to be written, with its sanitized name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sprite {
    /// Original region name, as it appears in the source atlas.
    pub name: String,
    /// Name usable as a CSS class / SVG id (see [`sanitize_name`]).
    pub class: String,
    /// Left corner in the atlas, in pixels.
    pub x: i32,
    /// Top corner in the atlas, in pixels.
    pub y: i32,
    /// Width in pixels.
    pub width: i32,
    /// Height in pixels.
    pub height: i32,
}

/// Sprite sheet extracted from an atlas.
#[derive(Debug, Clone)]
pub struct SpriteSheet {
    /// Name of the carrying texture (the base of the file names).
    pub name: String,
    /// Atlas width.
    pub width: i32,
    /// Atlas height.
    pub height: i32,
    /// Regions, in file order.
    pub sprites: Vec<Sprite>,
}

/// Prefix of the generated CSS classes and SVG identifiers.
pub const PREFIX: &str = "sprite";

/// How the CSS sheet lays the atlas out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CssMode {
    /// `background-image`: renders the atlas pixels as they are.
    Image,
    /// `mask-image` + `background-color: currentColor`: the icon takes the text colour.
    ///
    /// This lets a raster icon follow the theme like a vector icon. It assumes an atlas flattened to a
    /// mask (only alpha counts).
    Mask,
}

/// Sanitize a region name into a valid CSS/SVG identifier.
///
/// Every character outside `[A-Za-z0-9_-]` becomes `-`, the result is lower-cased, and a name that
/// starts with a digit is prefixed, since a CSS identifier cannot start with a digit.
#[must_use]
pub fn sanitize_name(name: &str) -> String {
    let mut s = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            s.push(c.to_ascii_lowercase());
        } else {
            s.push('-');
        }
    }
    if s.is_empty() {
        return "sprite".to_string();
    }
    if s.starts_with(|c: char| c.is_ascii_digit()) {
        s.insert(0, 's');
    }
    s
}

impl SpriteSheet {
    /// Number of regions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.sprites.len()
    }

    /// `true` when the atlas describes no region (a plain image).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sprites.is_empty()
    }

    /// Generate the CSS sheet in image mode, `image_url` being the atlas URL (relative path or `data:`).
    ///
    /// The base selector carries the image; each region class only carries its size and position.
    /// An element is written `<i class="sprite-base sprite-icon_01"></i>`.
    #[must_use]
    pub fn to_css(&self, image_url: &str) -> String {
        self.to_css_mode(image_url, CssMode::Image)
    }

    /// Generate the CSS sheet in the requested mode.
    ///
    /// In [`CssMode::Mask`] the atlas is the `mask-image` and the colour comes from `currentColor`: the
    /// icons follow the text colour, hence the theme, light or dark. The atlas must have been flattened
    /// to a mask beforehand (alpha kept, colour ignored).
    #[must_use]
    pub fn to_css_mode(&self, image_url: &str, mode: CssMode) -> String {
        let url = escape_url(image_url);
        let mut css = String::new();
        css.push_str(&format!(
            "/* {} - {} region(s), atlas {}x{}. */\n",
            self.name,
            self.sprites.len(),
            self.width,
            self.height
        ));

        // `background-size` / `mask-size` on the base selector: without it, scaling the atlas shifts
        // every region and the positions below become wrong.
        match mode {
            CssMode::Image => css.push_str(&format!(
                ".{PREFIX}-base {{\n  display: inline-block;\n  background-image: url(\"{url}\");\n  \
                 background-repeat: no-repeat;\n  background-size: {}px {}px;\n  \
                 image-rendering: pixelated;\n}}\n\n",
                self.width, self.height
            )),
            CssMode::Mask => css.push_str(&format!(
                ".{PREFIX}-base {{\n  display: inline-block;\n  background-color: currentColor;\n  \
                 -webkit-mask-image: url(\"{url}\");\n  mask-image: url(\"{url}\");\n  \
                 -webkit-mask-repeat: no-repeat;\n  mask-repeat: no-repeat;\n  \
                 -webkit-mask-size: {0}px {1}px;\n  mask-size: {0}px {1}px;\n}}\n\n",
                self.width, self.height
            )),
        }

        for s in &self.sprites {
            let (x, y) = (-s.x, -s.y);
            match mode {
                CssMode::Image => css.push_str(&format!(
                    ".{PREFIX}-{} {{ width: {}px; height: {}px; background-position: {x}px {y}px; }}\n",
                    s.class, s.width, s.height
                )),
                CssMode::Mask => css.push_str(&format!(
                    ".{PREFIX}-{} {{ width: {}px; height: {}px; \
                     -webkit-mask-position: {x}px {y}px; mask-position: {x}px {y}px; }}\n",
                    s.class, s.width, s.height
                )),
            }
        }
        css
    }

    /// Generate a self-contained SVG: a single `<image>` and one `<symbol>` per region.
    ///
    /// The atlas is embedded as `data:`: the file stands on its own, which is what lets it cross a web
    /// page or a bundle without losing its pixels. Page side:
    /// `<svg><use href="sheet.svg#sprite-icon_01"/></svg>`.
    #[must_use]
    pub fn to_svg(&self, image_data_uri: &str) -> String {
        let mut svg = String::new();
        svg.push_str(&format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" \
             width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">\n",
            self.width, self.height, self.width, self.height
        ));
        svg.push_str(&format!(
            "  <title>{} - {} region(s)</title>\n  <defs>\n",
            escape_xml(&self.name),
            self.sprites.len()
        ));
        svg.push_str(&format!(
            "    <image id=\"{PREFIX}-atlas\" width=\"{}\" height=\"{}\" href=\"{}\" \
             style=\"image-rendering: pixelated\"/>\n",
            self.width,
            self.height,
            escape_xml_attr(image_data_uri)
        ));
        for s in &self.sprites {
            svg.push_str(&format!(
                "    <symbol id=\"{PREFIX}-{}\" viewBox=\"{} {} {} {}\" width=\"{}\" height=\"{}\">\
                 <use href=\"#{PREFIX}-atlas\"/></symbol>\n",
                s.class, s.x, s.y, s.width, s.height, s.width, s.height
            ));
        }
        svg.push_str("  </defs>\n");
        // Without a rendered element an SVG opened alone looks empty: show the whole atlas.
        svg.push_str(&format!("  <use href=\"#{PREFIX}-atlas\"/>\n</svg>\n"));
        svg
    }

    /// Generate the JSON manifest of the regions: the same information, for code.
    ///
    /// Lets a viewer or a web page index regions by name without parsing the CSS.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut j = String::new();
        j.push_str(&format!(
            "{{\n  \"name\": \"{}\",\n  \"width\": {},\n  \"height\": {},\n  \"sprites\": [\n",
            escape_json(&self.name),
            self.width,
            self.height
        ));
        for (i, s) in self.sprites.iter().enumerate() {
            j.push_str(&format!(
                "    {{ \"name\": \"{}\", \"class\": \"{}\", \"x\": {}, \"y\": {}, \"width\": {}, \"height\": {} }}{}\n",
                escape_json(&s.name),
                escape_json(&s.class),
                s.x,
                s.y,
                s.width,
                s.height,
                if i + 1 == self.sprites.len() { "" } else { "," }
            ));
        }
        j.push_str("  ]\n}\n");
        j
    }
}

/// Escape backslashes and quotes of a URL placed in `url("...")`.
fn escape_url(u: &str) -> String {
    u.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Escape text inserted in an XML node.
fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Escape text inserted in a double-quoted XML attribute.
fn escape_xml_attr(s: &str) -> String {
    escape_xml(s).replace('"', "&quot;")
}

/// Escape a JSON string.
fn escape_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out
}

/// Build a `data:` URI for an encoded atlas.
#[must_use]
pub fn data_uri(bytes: &[u8], mime: &str) -> String {
    format!("data:{mime};base64,{}", base64(bytes))
}

/// Standard base64 with padding: three lines are not worth a dependency.
fn base64(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(input.len().div_ceil(3) * 4);
    for c in input.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        s.push(T[(n >> 18) as usize & 63] as char);
        s.push(T[(n >> 12) as usize & 63] as char);
        s.push(if c.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        s.push(if c.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example_sheet() -> SpriteSheet {
        SpriteSheet {
            name: "game_icons".to_string(),
            width: 512,
            height: 256,
            sprites: alloc::vec![
                Sprite {
                    name: "rarity01_05".into(),
                    class: "rarity01_05".into(),
                    x: 128,
                    y: 64,
                    width: 96,
                    height: 32
                },
                Sprite {
                    name: "icon 01".into(),
                    class: "icon-01".into(),
                    x: 0,
                    y: 0,
                    width: 32,
                    height: 32
                },
            ],
        }
    }

    #[test]
    fn css_positions_are_negative() {
        let css = example_sheet().to_css("game_icons.webp");
        assert!(css.contains("background-image: url(\"game_icons.webp\")"));
        // The CSS offset is the opposite of the region corner: the sprite rule.
        assert!(
            css.contains(
                ".sprite-rarity01_05 { width: 96px; height: 32px; background-position: -128px -64px; }"
            ),
            "{css}"
        );
        assert!(
            css.contains("image-rendering: pixelated"),
            "icons must not be smoothed"
        );
    }

    /// Without `background-size`, scaling the atlas shifts every region and the computed positions
    /// become wrong, so the base selector must carry it.
    #[test]
    fn css_pins_the_atlas_size() {
        let css = example_sheet().to_css("a.webp");
        assert!(css.contains("background-size: 512px 256px;"), "{css}");
    }

    #[test]
    fn mask_mode_tints_by_current_color() {
        let css = example_sheet().to_css_mode("a.webp", CssMode::Mask);
        assert!(css.contains("background-color: currentColor;"), "{css}");
        assert!(css.contains("mask-image: url(\"a.webp\")"));
        assert!(
            css.contains("-webkit-mask-image: url(\"a.webp\")"),
            "WebKit prefix required"
        );
        assert!(css.contains("mask-size: 512px 256px;"));
        assert!(
            css.contains(
                ".sprite-rarity01_05 { width: 96px; height: 32px; \
                 -webkit-mask-position: -128px -64px; mask-position: -128px -64px; }"
            ),
            "{css}"
        );
        // In mask mode no background image is set: the colour alone fills the silhouette.
        assert!(
            !css.contains("background-image"),
            "mask mode sets no background image"
        );
    }

    #[test]
    fn svg_is_self_contained_with_one_symbol_per_region() {
        let svg = example_sheet().to_svg("data:image/png;base64,AAAA");
        assert!(svg.starts_with("<svg xmlns="));
        assert!(
            svg.contains("href=\"data:image/png;base64,AAAA\""),
            "atlas embedded"
        );
        assert!(svg.contains("<symbol id=\"sprite-rarity01_05\" viewBox=\"128 64 96 32\""));
        assert_eq!(svg.matches("<symbol").count(), 2, "one symbol per region");
        assert!(svg.ends_with("</svg>\n"));
    }

    #[test]
    fn json_lists_the_regions() {
        let j = example_sheet().to_json();
        assert!(j.contains("\"name\": \"game_icons\""));
        assert!(j.contains("\"class\": \"rarity01_05\""));
        // No comma after the last element, or the JSON is invalid.
        assert!(!j.contains("},\n  ]"));
        let v: serde_json::Value = serde_json::from_str(&j).expect("valid JSON");
        assert_eq!(v["sprites"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn json_escapes_control_characters() {
        let mut sheet = example_sheet();
        sheet.name = "a\"b\\c\nd".to_string();
        let v: serde_json::Value = serde_json::from_str(&sheet.to_json()).expect("valid JSON");
        assert_eq!(v["name"], "a\"b\\c\nd");
    }

    #[test]
    fn names_become_valid_identifiers() {
        assert_eq!(sanitize_name("rarity01_05"), "rarity01_05");
        assert_eq!(sanitize_name("icon 01"), "icon-01");
        assert_eq!(sanitize_name("A/B.c"), "a-b-c");
        assert_eq!(
            sanitize_name("01_start"),
            "s01_start",
            "a CSS id does not start with a digit"
        );
        assert_eq!(sanitize_name(""), "sprite");
        assert_eq!(sanitize_name("CAPS"), "caps", "classes are lower case");
    }

    #[test]
    fn base64_matches_rfc4648() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert!(data_uri(b"foo", "image/png").starts_with("data:image/png;base64,Zm9v"));
    }
}
