//! Derivation data (`data/derive.toml`) and the hue placement used for ANSI and accent colors.

use crate::error::{Error, Result};
use material_colors::{blend::harmonize, color::Rgb, hct::Hct};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::OnceLock;

const DATA: &str = include_str!("../data/derive.toml");

#[derive(Debug, Deserialize)]
pub struct Hue {
    pub hue: f64,
    pub chroma: f64,
}

#[derive(Debug, Deserialize)]
pub struct Tones {
    pub dark_normal: f64,
    pub dark_bright: f64,
    pub light_normal: f64,
    pub light_bright: f64,
}

#[derive(Debug, Deserialize)]
pub struct Derive {
    pub hues: BTreeMap<String, Hue>,
    pub bright: Vec<String>,
    pub tone: Tones,
    pub palette: BTreeMap<String, String>,
    pub terminal: BTreeMap<String, String>,
    pub roles: BTreeMap<String, String>,
}

pub fn data() -> Result<&'static Derive> {
    static PARSED: OnceLock<std::result::Result<Derive, String>> = OnceLock::new();
    PARSED
        .get_or_init(|| toml::from_str(DATA).map_err(|error| format!("data/derive.toml: {error}")))
        .as_ref()
        .map_err(|message| Error::Invalid(message.clone()))
}

impl Derive {
    /// The color of hue `name` for a seed: the table hue harmonized toward the seed, at the tone of the mode.
    pub fn hue_color(&self, name: &str, seed: u32, dark: bool, bright: bool) -> Result<u32> {
        let hue = self.hues.get(name).ok_or_else(|| Error::Invalid(format!("no hue `{name}` in derive.toml")))?;
        let tone = match (dark, bright) {
            (true, false) => self.tone.dark_normal,
            (true, true) => self.tone.dark_bright,
            (false, false) => self.tone.light_normal,
            (false, true) => self.tone.light_bright,
        };
        let placed: Rgb = Hct::from(hue.hue, hue.chroma, tone).into();
        let [red, green, blue] = crate::color::channels(seed);
        let harmonized = harmonize(placed, Rgb::new(red, green, blue));
        Ok(crate::color::from_channels([harmonized.red, harmonized.green, harmonized.blue]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fw_m3::contrast_ratio_u32;

    #[test]
    fn data_parses_and_every_mapping_target_exists() -> Result<()> {
        let derive = data()?;
        for hue in &derive.bright {
            assert!(derive.hues.contains_key(hue), "bright hue {hue}");
        }
        for role in derive.roles.keys().chain(derive.palette.values()) {
            assert!(fw_m3::scheme::ROLE_NAMES.contains(&role.as_str()) || derive.roles.contains_key(role) || derive.palette.contains_key(role), "{role}");
        }
        Ok(())
    }

    #[test]
    fn derived_hues_are_readable_on_a_dark_surface() -> Result<()> {
        let derive = data()?;
        let surface = fw_m3::scheme::role(0x135b82, fw_m3::scheme::SchemeVariant::TonalSpot, true, "surface").ok_or_else(|| Error::Invalid("surface".into()))?;
        for name in derive.hues.keys().filter(|name| name.as_str() != "brown") {
            let color = derive.hue_color(name, 0x135b82, true, false)?;
            assert!(contrast_ratio_u32(color, surface) >= 4.5, "{name} {color:06x} on {surface:06x}");
        }
        Ok(())
    }
}
