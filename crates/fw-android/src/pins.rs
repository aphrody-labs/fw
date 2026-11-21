//! `configs/android/android.toml`: the pinned guest images and their licenses. The file is embedded at build time,
//! `FW_ANDROID_PINS` points at another copy.

use std::path::PathBuf;

use serde::Deserialize;

const EMBEDDED: &str = include_str!("../../../configs/android/android.toml");

#[derive(Debug, Deserialize)]
pub struct Pins {
    pub schema: u32,
    pub arch: String,
    pub channel: Channel,
    #[serde(rename = "image")]
    pub images: Vec<Image>,
    #[serde(rename = "license")]
    pub licenses: Vec<License>,
}

#[derive(Debug, Deserialize)]
pub struct Channel {
    pub system: String,
    pub vendor: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Image {
    pub role: String,
    pub filename: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
    pub member: String,
}

#[derive(Debug, Deserialize, serde::Serialize)]
pub struct License {
    pub component: String,
    pub spdx: String,
    pub source: String,
}

impl Pins {
    pub fn load() -> Result<Self, String> {
        let text = match std::env::var_os("FW_ANDROID_PINS") {
            Some(path) => {
                let path = PathBuf::from(path);
                std::fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?
            }
            None => EMBEDDED.to_owned(),
        };
        let pins: Pins = toml::from_str(&text).map_err(|error| format!("android.toml: {error}"))?;
        if pins.schema != 1 {
            return Err(format!("android.toml schema {} is not supported", pins.schema));
        }
        Ok(pins)
    }

    pub fn image(&self, role: &str) -> Result<&Image, String> {
        self.images.iter().find(|image| image.role == role).ok_or_else(|| format!("no {role} image pinned"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_pins_have_system_and_vendor_with_sha256() {
        let pins = Pins::load().expect("pins");
        for role in ["system", "vendor"] {
            let image = pins.image(role).expect("image");
            assert_eq!(image.sha256.len(), 64, "{role} sha256");
            assert!(image.url.starts_with("https://"));
        }
        assert!(!pins.licenses.is_empty());
    }
}
