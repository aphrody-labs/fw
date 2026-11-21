//! The brand seed and variant of `product.toml` `[brand]`, the single source of the default theme color.

use crate::error::{Error, Result};
use fw_m3::scheme::{SchemeVariant, parse_seed};
use std::path::Path;

/// The `product.toml` this crate was built from; used when no `product.toml` is found from the working directory upward.
const EMBEDDED_PRODUCT: &str = include_str!("../../../product.toml");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Brand {
    pub seed: u32,
    pub variant: SchemeVariant,
    /// The variant as written in `product.toml`.
    pub variant_name: String,
}

impl Brand {
    pub fn parse(product_toml: &str, origin: &str) -> Result<Self> {
        let table: toml::Table = product_toml.parse().map_err(|source| Error::Toml { path: origin.into(), source })?;
        let brand = table
            .get("brand")
            .and_then(toml::Value::as_table)
            .ok_or_else(|| Error::Invalid(format!("{origin}: no [brand] table")))?;
        let text = |key: &str| brand.get(key).and_then(toml::Value::as_str);
        let seed_text = text("seed").ok_or_else(|| Error::Invalid(format!("{origin}: [brand] seed missing")))?;
        let seed = parse_seed(seed_text).ok_or_else(|| Error::Invalid(format!("{origin}: seed `{seed_text}` is not #rrggbb")))?;
        let variant_text = text("variant").unwrap_or("tonal-spot");
        let variant = SchemeVariant::from_slug(variant_text)
            .ok_or_else(|| Error::Invalid(format!("{origin}: variant `{variant_text}` is not one of {}", SchemeVariant::SLUGS.join(", "))))?;
        Ok(Self { seed, variant, variant_name: variant_text.to_owned() })
    }

    /// The nearest `product.toml` from `start` upward, or the one embedded at build time.
    pub fn discover(start: &Path) -> Result<Self> {
        for dir in start.ancestors() {
            let file = dir.join("product.toml");
            if file.is_file() {
                let text = std::fs::read_to_string(&file).map_err(|source| Error::io(&file, source))?;
                return Self::parse(&text, &file.display().to_string());
            }
        }
        Self::parse(EMBEDDED_PRODUCT, "embedded product.toml")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_product_has_a_brand() -> Result<()> {
        let brand = Brand::parse(EMBEDDED_PRODUCT, "embedded")?;
        assert!(brand.seed <= 0xff_ffff);
        Ok(())
    }

    #[test]
    fn rejects_a_bad_variant() {
        assert!(Brand::parse("[brand]\nseed = \"#112233\"\nvariant = \"nope\"\n", "test").is_err());
    }
}
