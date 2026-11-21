//! A theme with every color settled: Material 3 roles from `fw-m3`, the palette and terminal colors, icons and wallpaper.

use crate::brand::Brand;
use crate::color::{hex, parse_hex};
use crate::derive::{self, Derive};
use crate::error::{Error, Result};
use crate::theme::{IconSpec, ModeSpec, Theme, ThemeFile};
use fw_m3::scheme::{SchemeVariant, scheme_roles, usable_seed};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Light,
    Dark,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    pub fn is_dark(self) -> bool {
        self == Self::Dark
    }
}

/// The colors of one mode.
#[derive(Debug, Clone)]
pub struct View {
    pub mode: Mode,
    /// Material 3 roles (`primary`, `on-surface`, ...), `0xRRGGBB`.
    pub roles: BTreeMap<String, u32>,
    /// omarchy palette names: background, foreground, accent, red, bright_red, ...
    pub palette: BTreeMap<String, u32>,
    /// black .. bright_white, cursor, selection_background, selection_foreground.
    pub terminal: BTreeMap<String, u32>,
}

impl View {
    /// `m3.<role>`, `pal.<key>` or `term.<key>`.
    pub fn source(&self, source: &str) -> Option<u32> {
        let (table, key) = source.split_once('.')?;
        match table {
            "m3" => self.roles.get(key),
            "pal" => self.palette.get(key),
            "term" => self.terminal.get(key),
            _ => None,
        }
        .copied()
    }
}

#[derive(Debug, Clone)]
pub struct Resolved {
    pub name: String,
    pub title: String,
    pub author: Option<String>,
    pub seed: u32,
    pub variant: String,
    pub icons: IconSpec,
    pub wallpaper: Option<String>,
    pub dir: Option<PathBuf>,
    pub active: Mode,
    pub views: BTreeMap<Mode, View>,
}

impl Resolved {
    pub fn active_view(&self) -> Result<&View> {
        self.views.get(&self.active).ok_or_else(|| Error::Invalid(format!("theme {} has no {} view", self.name, self.active.as_str())))
    }
}

impl Theme {
    /// Resolves the theme; `preference` picks the active mode when the theme provides it, otherwise dark, otherwise its only mode.
    pub fn resolve(&self, brand: &Brand, preference: Option<Mode>) -> Result<Resolved> {
        let file = &self.file;
        let derive = derive::data()?;
        let explicit = !file.colors.is_empty();
        let modes: &[Mode] = match file.mode {
            ModeSpec::Dark => &[Mode::Dark],
            ModeSpec::Light => &[Mode::Light],
            ModeSpec::Both if explicit => {
                return Err(Error::Invalid(format!("theme {}: an explicit [colors] palette needs mode = \"dark\" or \"light\"", file.name)));
            }
            ModeSpec::Both => &[Mode::Light, Mode::Dark],
        };
        let (seed, variant, variant_name) = seed_of(file, brand)?;
        let mut views = BTreeMap::new();
        for mode in modes {
            views.insert(*mode, build_view(file, derive, seed, variant, *mode).map_err(|error| Error::Invalid(format!("theme {}: {error}", file.name)))?);
        }
        let active = preference.filter(|mode| views.contains_key(mode)).or_else(|| views.contains_key(&Mode::Dark).then_some(Mode::Dark)).or_else(|| modes.first().copied()).unwrap_or(Mode::Dark);
        Ok(Resolved {
            name: file.name.clone(),
            title: file.title.clone().unwrap_or_else(|| file.name.clone()),
            author: file.author.clone(),
            seed,
            variant: variant_name,
            icons: file.icons.clone(),
            wallpaper: wallpaper_of(file, self.dir.as_deref()),
            dir: self.dir.clone(),
            active,
            views,
        })
    }
}

fn seed_of(file: &ThemeFile, brand: &Brand) -> Result<(u32, SchemeVariant, String)> {
    let m3 = file.m3.as_ref();
    let (seed, from_brand) = match m3.and_then(|spec| spec.seed.as_deref()) {
        Some("brand") => (brand.seed, true),
        Some(text) => (parse_hex(text)?, false),
        None => match file.colors.get("accent") {
            Some(accent) => (parse_hex(accent)?, false),
            None => (brand.seed, true),
        },
    };
    let (variant, name) = match m3.and_then(|spec| spec.variant.as_deref()) {
        Some(text) => (
            SchemeVariant::from_slug(text).ok_or_else(|| Error::Invalid(format!("theme {}: variant `{text}` is not one of {}", file.name, SchemeVariant::SLUGS.join(", "))))?,
            text.to_owned(),
        ),
        None if from_brand => (brand.variant, brand.variant_name.clone()),
        None => (SchemeVariant::TonalSpot, "tonal-spot".to_owned()),
    };
    Ok((usable_seed(seed, variant), variant, name))
}

fn build_view(file: &ThemeFile, derive: &Derive, seed: u32, variant: SchemeVariant, mode: Mode) -> Result<View> {
    let dark = mode.is_dark();
    let mut roles: BTreeMap<String, u32> = scheme_roles(seed, variant, dark).into_iter().map(|(name, color)| (name.to_owned(), color)).collect();

    let mut palette = BTreeMap::new();
    for (key, role) in &derive.palette {
        let color = roles.get(role).ok_or_else(|| Error::Invalid(format!("derive.toml palette `{key}` names unknown role `{role}`")))?;
        palette.insert(key.clone(), *color);
    }
    for name in derive.hues.keys() {
        palette.insert(name.clone(), derive.hue_color(name, seed, dark, false)?);
    }
    for name in &derive.bright {
        palette.insert(format!("bright_{name}"), derive.hue_color(name, seed, dark, true)?);
    }
    for (key, value) in &file.colors {
        let color = parse_hex(value).map_err(|error| Error::Invalid(format!("[colors] {key}: {error}")))?;
        palette.insert(key.clone(), color);
    }
    for name in &derive.bright {
        let bright = format!("bright_{name}");
        if file.colors.contains_key(name)
            && !file.colors.contains_key(&bright)
            && let Some(color) = palette.get(name).copied()
        {
            palette.insert(bright, color);
        }
    }

    if !file.colors.is_empty() {
        for (role, key) in &derive.roles {
            if let Some(color) = palette.get(key) {
                roles.insert(role.clone(), *color);
            }
        }
        if let (Some(accent), Some(background), Some(foreground)) = (palette.get("accent"), palette.get("background"), palette.get("foreground")) {
            let on_accent = if fw_m3::contrast_ratio_u32(*accent, *background) >= fw_m3::contrast_ratio_u32(*accent, *foreground) { *background } else { *foreground };
            roles.insert("on-primary".to_owned(), on_accent);
        }
    }

    let mut terminal = BTreeMap::new();
    for (key, source) in &derive.terminal {
        let color = palette.get(source).ok_or_else(|| Error::Invalid(format!("derive.toml terminal `{key}` names unknown palette key `{source}`")))?;
        terminal.insert(key.clone(), *color);
    }
    for key in derive.terminal.keys() {
        if let Some(color) = palette.get(key).filter(|_| file.colors.contains_key(key)) {
            terminal.insert(key.clone(), *color);
        }
    }
    for (key, value) in &file.terminal {
        let color = parse_hex(value).map_err(|error| Error::Invalid(format!("[terminal] {key}: {error}")))?;
        terminal.insert(key.clone(), color);
    }
    Ok(View { mode, roles, palette, terminal })
}

fn wallpaper_of(file: &ThemeFile, dir: Option<&Path>) -> Option<String> {
    if let Some(spec) = &file.wallpaper {
        if let Some(url) = &spec.url {
            return Some(url.clone());
        }
        if let Some(path) = &spec.path {
            return Some(dir.map_or_else(|| PathBuf::from(path), |dir| dir.join(path)).display().to_string());
        }
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir?.join("backgrounds")).ok()?.flatten().map(|entry| entry.path()).filter(|path| path.is_file()).collect();
    files.sort();
    files.first().map(|path| path.display().to_string())
}

/// `#rrggbb` of a role, for display.
pub fn describe(view: &View, role: &str) -> Option<String> {
    view.roles.get(role).map(|color| hex(*color))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brand() -> Result<Brand> {
        Brand::parse("[brand]\nseed = \"#135b82\"\nvariant = \"tonal-spot\"\n", "test")
    }

    fn theme(text: &str) -> Result<Theme> {
        Theme::parse(text, Path::new("test"), None)
    }

    #[test]
    fn a_seed_only_theme_derives_both_modes_and_a_full_terminal() -> Result<()> {
        let resolved = theme("name = \"seeded\"\n[m3]\nseed = \"brand\"\n")?.resolve(&brand()?, None)?;
        assert_eq!(resolved.views.len(), 2);
        assert_eq!(resolved.active, Mode::Dark);
        let dark = resolved.active_view()?;
        for key in ["black", "red", "bright_white", "cursor", "selection_background"] {
            assert!(dark.terminal.contains_key(key), "{key}");
        }
        assert_eq!(dark.palette["background"], dark.roles["surface"]);
        Ok(())
    }

    #[test]
    fn an_explicit_palette_overrides_roles_and_keeps_its_mode() -> Result<()> {
        let text = "name = \"explicit\"\nmode = \"dark\"\n[colors]\nbackground = \"#2e3440\"\nforeground = \"#d8dee9\"\naccent = \"#81a1c1\"\nred = \"#bf616a\"\n";
        let resolved = theme(text)?.resolve(&brand()?, Some(Mode::Light))?;
        assert_eq!(resolved.active, Mode::Dark);
        let view = resolved.active_view()?;
        assert_eq!(view.roles["surface"], 0x2e3440);
        assert_eq!(view.roles["primary"], 0x81a1c1);
        assert_eq!(view.terminal["red"], 0xbf616a);
        assert_eq!(view.palette["bright_red"], 0xbf616a);
        assert_eq!(view.terminal["black"], 0x2e3440);
        Ok(())
    }

    #[test]
    fn explicit_colors_with_both_modes_are_rejected() -> Result<()> {
        let text = "name = \"bad\"\n[colors]\nbackground = \"#000000\"\n";
        assert!(theme(text)?.resolve(&brand()?, None).is_err());
        Ok(())
    }

    #[test]
    fn preference_selects_the_light_view_of_a_both_theme() -> Result<()> {
        let resolved = theme("name = \"both\"\n")?.resolve(&brand()?, Some(Mode::Light))?;
        assert_eq!(resolved.active, Mode::Light);
        Ok(())
    }
}
