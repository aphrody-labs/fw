//! `0xRRGGBB` colors and their textual forms.

use crate::error::{Error, Result};

/// Parses `#rgb`, `#rrggbb` or `#rrggbbaa` (alpha is dropped) into `0xRRGGBB`.
pub fn parse_hex(text: &str) -> Result<u32> {
    let digits = text.trim().trim_start_matches('#');
    let expanded: String = match digits.len() {
        3 => digits.chars().flat_map(|c| [c, c]).collect(),
        6 => digits.to_owned(),
        8 => digits.chars().take(6).collect(),
        _ => return Err(Error::Invalid(format!("`{text}` is not a #rgb, #rrggbb or #rrggbbaa color"))),
    };
    u32::from_str_radix(&expanded, 16).map_err(|_| Error::Invalid(format!("`{text}` is not a hex color")))
}

pub fn hex(rgb: u32) -> String {
    format!("#{rgb:06x}")
}

pub fn bare(rgb: u32) -> String {
    format!("{rgb:06x}")
}

pub fn channels(rgb: u32) -> [u8; 3] {
    [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8]
}

pub fn from_channels([red, green, blue]: [u8; 3]) -> u32 {
    (u32::from(red) << 16) | (u32::from(green) << 8) | u32::from(blue)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_short_long_and_alpha_forms() -> Result<()> {
        assert_eq!(parse_hex("#abc")?, 0xaabbcc);
        assert_eq!(parse_hex("2e3440")?, 0x2e3440);
        assert_eq!(parse_hex("#2e3440ff")?, 0x2e3440);
        assert!(parse_hex("#12345").is_err());
        assert!(parse_hex("#gggggg").is_err());
        Ok(())
    }

    #[test]
    fn channels_round_trip() {
        assert_eq!(from_channels(channels(0x81a1c1)), 0x81a1c1);
        assert_eq!(hex(0x0a0b0c), "#0a0b0c");
    }
}
