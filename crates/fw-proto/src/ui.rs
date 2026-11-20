//! Web UI assets. `ui/` is the single copy: `fw serve` embeds these files and the
//! Cloudflare Worker publishes the same directory as static assets.

pub const CONTROL_PLANE_HTML: &str = include_str!("../ui/index.html");
pub const CONTROL_PLANE_CSS: &str = include_str!("../ui/app.css");
pub const CONTROL_PLANE_JS: &str = include_str!("../ui/app.js");
pub const WEBGPU_HTML_HARNESS: &str = include_str!("../ui/webgpu.html");

#[cfg(test)]
mod tests {
    use fw_m3::scheme::{css_custom_properties, parse_seed, splice_css, SchemeVariant};

    /// app.css carries the color roles generated from `[brand]` in product.toml; regenerate with
    /// `cargo run -p fw-m3 -- css --write crates/fw-proto/ui/app.css` when the seed changes.
    #[test]
    fn app_css_colors_match_the_brand_seed() {
        let product: toml::Table = include_str!("../../../product.toml").parse().unwrap();
        let brand = product["brand"].as_table().unwrap();
        let seed = parse_seed(brand["seed"].as_str().unwrap()).unwrap();
        let variant = SchemeVariant::from_slug(brand["variant"].as_str().unwrap()).unwrap();
        let expected = splice_css(super::CONTROL_PLANE_CSS, &css_custom_properties(seed, variant)).unwrap();
        assert!(expected == super::CONTROL_PLANE_CSS, "app.css colors are stale: run `cargo run -p fw-m3 -- css --write crates/fw-proto/ui/app.css`");
    }
}
