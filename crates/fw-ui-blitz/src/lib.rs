#![forbid(unsafe_code)]

//! `fw-ui-blitz`: headless software rasterizer that draws into an RGBA buffer and encodes a PNG (`flate2` only).
//! It does not link blitz, vello or any GPU API; the name refers to the renderer it was meant to match.

use flate2::write::ZlibEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlitzRenderReport {
    pub ok: bool,
    pub renderer: String,
    pub width: u32,
    pub height: u32,
    pub png_bytes: usize,
    pub unique_colors: usize,
    pub cards: usize,
    pub output_path: String,
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in bytes {
        crc ^= b as u32;
        for _ in 0..8 {
            if (crc & 1) != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

fn write_chunk(out: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(tag);
    out.extend_from_slice(data);
    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(tag);
    crc_input.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

/// Encode RGBA8 pixels to a standard PNG file (zero external C/libpng dependencies).
pub fn encode_rgba_png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let mut png = Vec::new();
    png.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.push(8); // bit depth
    ihdr.push(6); // RGBA
    ihdr.push(0); // compression
    ihdr.push(0); // filter
    ihdr.push(0); // interlace
    write_chunk(&mut png, b"IHDR", &ihdr);

    let row_bytes = (width as usize) * 4;
    let mut raw = Vec::with_capacity((row_bytes + 1) * (height as usize));
    for y in 0..(height as usize) {
        raw.push(0); // filter type None
        let start = y * row_bytes;
        raw.extend_from_slice(&rgba[start..start + row_bytes]);
    }
    let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
    enc.write_all(&raw).unwrap();
    let compressed = enc.finish().unwrap();
    write_chunk(&mut png, b"IDAT", &compressed);
    write_chunk(&mut png, b"IEND", &[]);
    png
}

/// One node of the HUD: its card is drawn in the accelerator band or the CPU band, dimmed when it is not healthy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HudCard {
    pub accelerated: bool,
    pub healthy: bool,
}

fn card_color(card: HudCard) -> (u8, u8, u8) {
    match (card.accelerated, card.healthy) {
        (true, true) => (28, 110, 96),
        (false, true) => (36, 44, 128),
        (_, false) => (70, 70, 74),
    }
}

/// Render the cluster HUD to PNG: a header bar, one card per accelerated node in the upper band and one per other
/// node in the lower band. The cards come from the caller (the nodes of `cluster/build.toml` plus the measured local
/// machine); with no cards the bands stay empty.
pub fn render_cluster_hud_png(
    out_path: &Path,
    width: u32,
    height: u32,
    cards: &[HudCard],
) -> std::io::Result<BlitzRenderReport> {
    let mut rgba = vec![0u8; (width as usize) * (height as usize) * 4];
    let mut colors = std::collections::HashSet::new();
    let accelerated: Vec<HudCard> = cards.iter().copied().filter(|card| card.accelerated).collect();
    let others: Vec<HudCard> = cards.iter().copied().filter(|card| !card.accelerated).collect();
    let inner_width = width.saturating_sub(48).max(1);

    let card_at = |band: &[HudCard], x: u32| -> Option<HudCard> {
        if band.is_empty() || x < 24 || x >= width.saturating_sub(24) {
            return None;
        }
        let slot_width = (inner_width / band.len() as u32).max(1);
        let slot = ((x - 24) / slot_width) as usize;
        let local_x = (x - 24) % slot_width;
        if local_x <= 6 {
            return None;
        }
        band.get(slot).copied()
    };

    for y in 0..height {
        for x in 0..width {
            let idx = ((y * width + x) as usize) * 4;
            let mut r = 11u8;
            let mut g = (16 + (y * 14 / height.max(1))) as u8;
            let mut b = (32 + (x * 24 / width.max(1))) as u8;

            if (24..72).contains(&y) && x >= 24 && x < width.saturating_sub(24) {
                r = 18;
                g = 34;
                b = 68;
            }
            let band_card = if (96..196).contains(&y) {
                card_at(&accelerated, x)
            } else if y >= 216 && y < height.saturating_sub(24) {
                card_at(&others, x)
            } else {
                None
            };
            if let Some(card) = band_card {
                (r, g, b) = card_color(card);
            }

            rgba[idx] = r;
            rgba[idx + 1] = g;
            rgba[idx + 2] = b;
            rgba[idx + 3] = 255;
            colors.insert((r, g, b));
        }
    }

    let png = encode_rgba_png(width, height, &rgba);
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(out_path, &png)?;
    Ok(BlitzRenderReport {
        ok: true,
        renderer: "fw-ui-blitz software rasterizer".into(),
        cards: cards.len(),
        width,
        height,
        png_bytes: png.len(),
        unique_colors: colors.len(),
        output_path: out_path.to_string_lossy().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_valid_non_blank_png() {
        let tmp = std::env::temp_dir().join("fw-blitz-unit.png");
        let cards = [
            HudCard { accelerated: true, healthy: true },
            HudCard { accelerated: false, healthy: true },
            HudCard { accelerated: false, healthy: false },
        ];
        let rep = render_cluster_hud_png(&tmp, 320, 180, &cards).unwrap();
        assert!(rep.ok);
        assert_eq!(rep.cards, 3);
        assert!(rep.png_bytes > 200);
        assert!(rep.unique_colors >= 6);
        let bytes = fs::read(&tmp).unwrap();
        assert_eq!(&bytes[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
        fs::remove_file(&tmp).unwrap();
    }

    #[test]
    fn cards_change_the_palette_and_no_cards_leave_the_background() {
        let tmp = std::env::temp_dir().join("fw-blitz-unit-empty.png");
        let rep = render_cluster_hud_png(&tmp, 320, 400, &[]).unwrap();
        let populated = render_cluster_hud_png(&tmp, 320, 400, &[HudCard { accelerated: true, healthy: true }]).unwrap();
        // A card paints over gradient pixels, so the palette changes; with no cards nothing is painted over.
        assert_ne!(rep.unique_colors, populated.unique_colors);
        fs::remove_file(&tmp).unwrap();
    }
}
