//! The on-disk theme model and the search path of theme directories.

use crate::error::{Error, Result};
use crate::paths;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

include!(concat!(env!("OUT_DIR"), "/bundled.rs"));

/// Modes a theme provides. `both` derives a light and a dark scheme from the seed; an explicit palette needs one mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ModeSpec {
    Dark,
    Light,
    #[default]
    Both,
}

/// Material 3 seed of the theme. Without it the `accent` color is the seed, and without that the brand seed.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct M3Spec {
    /// `#rrggbb`, or `brand` for `product.toml` [brand].
    pub seed: Option<String>,
    /// Scheme variant slug (tonal-spot, vibrant, expressive, fidelity, content, neutral, monochrome, rainbow, fruit-salad).
    pub variant: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum IconStyle {
    #[default]
    Outlined,
    Rounded,
    Sharp,
}

/// Icon set of the theme: Material Symbols as served by material.aphrody.com and compiled into `fw-material`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, default)]
pub struct IconSpec {
    pub style: IconStyle,
    /// Symbol weight, 100 to 700.
    pub weight: u16,
    pub fill: bool,
}

impl Default for IconSpec {
    fn default() -> Self {
        Self { style: IconStyle::Outlined, weight: 400, fill: false }
    }
}

/// A wallpaper: a file relative to the theme directory, or a URL. Without it the first file of `backgrounds/` is used.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WallpaperSpec {
    pub path: Option<String>,
    pub url: Option<String>,
}

/// `theme.toml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ThemeFile {
    pub name: String,
    pub title: Option<String>,
    pub author: Option<String>,
    pub description: Option<String>,
    /// The theme used when settings name none (one shipped theme sets it).
    #[serde(default)]
    pub default: bool,
    #[serde(default)]
    pub mode: ModeSpec,
    pub m3: Option<M3Spec>,
    /// Explicit palette with the omarchy `colors.toml` names (background, foreground, accent, red, bright_red, ...), `#rrggbb`.
    #[serde(default)]
    pub colors: BTreeMap<String, String>,
    /// Explicit terminal colors (black .. bright_white, cursor, selection_background, selection_foreground).
    #[serde(default)]
    pub terminal: BTreeMap<String, String>,
    #[serde(default)]
    pub icons: IconSpec,
    pub wallpaper: Option<WallpaperSpec>,
}

#[derive(Debug, Clone)]
pub struct Theme {
    pub file: ThemeFile,
    /// The directory of the theme; `None` for a theme compiled into the binary.
    pub dir: Option<PathBuf>,
}

impl Theme {
    pub fn parse(text: &str, origin: &Path, dir: Option<PathBuf>) -> Result<Self> {
        let mut file: ThemeFile = toml::from_str(text).map_err(|source| Error::Toml { path: origin.to_owned(), source })?;
        if let Some(dir) = &dir {
            file.merge_omarchy_colors(&dir.join("colors.toml"))?;
        }
        Ok(Self { file, dir })
    }

    pub fn load_dir(dir: &Path) -> Result<Self> {
        let path = dir.join("theme.toml");
        let text = std::fs::read_to_string(&path).map_err(|source| Error::io(&path, source))?;
        Self::parse(&text, &path, Some(dir.to_owned()))
    }
}

impl ThemeFile {
    /// An omarchy `colors.toml` next to `theme.toml` adds to `[colors]` (its `mode` key sets the mode when the theme has none).
    fn merge_omarchy_colors(&mut self, path: &Path) -> Result<()> {
        if !path.is_file() {
            return Ok(());
        }
        let text = std::fs::read_to_string(path).map_err(|source| Error::io(path, source))?;
        let table: toml::Table = text.parse().map_err(|source| Error::Toml { path: path.to_owned(), source })?;
        for (key, value) in table {
            let Some(text) = value.as_str() else { continue };
            if key == "mode" {
                self.mode = match text {
                    "dark" => ModeSpec::Dark,
                    "light" => ModeSpec::Light,
                    other => return Err(Error::Invalid(format!("{}: mode `{other}` is not dark or light", path.display()))),
                };
            } else {
                self.colors.entry(key).or_insert_with(|| text.to_owned());
            }
        }
        Ok(())
    }
}

/// Theme directories searched in order; a name found in an earlier root shadows later ones and the bundled set.
#[derive(Debug, Clone)]
pub struct ThemeStore {
    roots: Vec<PathBuf>,
}

impl ThemeStore {
    pub fn new(roots: Vec<PathBuf>) -> Self {
        Self { roots }
    }

    pub fn discover() -> Self {
        Self::new(paths::theme_roots())
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    /// Names of every theme: directories of the roots and the bundled set, sorted and unique.
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = BUNDLED.iter().map(|(name, _)| (*name).to_owned()).collect();
        for root in &self.roots {
            let entries = std::fs::read_dir(root).into_iter().flatten().flatten();
            names.extend(entries.filter(|entry| entry.path().join("theme.toml").is_file()).filter_map(|entry| entry.file_name().into_string().ok()));
        }
        names.sort();
        names.dedup();
        names
    }

    /// The theme that marks itself `default = true`, else the first by name.
    pub fn default_name(&self) -> Option<String> {
        let names = self.names();
        names.iter().find(|name| self.load(name).is_ok_and(|theme| theme.file.default)).or_else(|| names.first()).cloned()
    }

    pub fn load(&self, name: &str) -> Result<Theme> {
        for root in &self.roots {
            let dir = root.join(name);
            if dir.join("theme.toml").is_file() {
                return Theme::load_dir(&dir);
            }
        }
        if let Some((_, text)) = BUNDLED.iter().find(|(bundled, _)| *bundled == name) {
            return Theme::parse(text, Path::new(name), None);
        }
        Err(Error::UnknownTheme { name: name.to_owned(), available: self.names().join(", ") })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bundled_theme_parses() -> Result<()> {
        assert!(!BUNDLED.is_empty());
        for (name, text) in BUNDLED {
            let theme = Theme::parse(text, Path::new(name), None)?;
            assert_eq!(&theme.file.name, name);
        }
        Ok(())
    }

    #[test]
    fn omarchy_colors_toml_merges_into_the_palette() -> Result<()> {
        let dir = tempfile::tempdir().map_err(|e| Error::io("tempdir", e))?;
        std::fs::write(dir.path().join("theme.toml"), "name = \"x\"\n[colors]\naccent = \"#112233\"\n").map_err(|e| Error::io("theme.toml", e))?;
        std::fs::write(dir.path().join("colors.toml"), "mode = \"light\"\naccent = \"#ffffff\"\nbackground = \"#fafafa\"\n").map_err(|e| Error::io("colors.toml", e))?;
        let theme = Theme::load_dir(dir.path())?;
        assert_eq!(theme.file.mode, ModeSpec::Light);
        assert_eq!(theme.file.colors["accent"], "#112233");
        assert_eq!(theme.file.colors["background"], "#fafafa");
        Ok(())
    }
}
