//! `settings.json`: the user's choices, in the Zed settings style (one JSON file with a JSON Schema, reloaded on change).
//!
//! Sections owned by other crates (terminal, uutils applets, rio) are kept verbatim in [`Settings::extra`] and described
//! by schema fragments that those crates register (see [`crate::schema`]).

use crate::error::{Error, Result};
use crate::resolve::Mode;
use crate::targets::Target;
use crate::theme::IconStyle;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

/// Which mode of a theme to use; `system` lets the theme pick (dark when it provides both).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ModePreference {
    #[default]
    System,
    Light,
    Dark,
}

impl ModePreference {
    pub fn mode(self) -> Option<Mode> {
        match self {
            Self::System => None,
            Self::Light => Some(Mode::Light),
            Self::Dark => Some(Mode::Dark),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(default)]
pub struct ThemeSettings {
    /// Name of a theme directory (`fw theme list`); the default theme when unset.
    pub name: Option<String>,
    pub mode: ModePreference,
}

/// Overrides of the icon set of the active theme.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IconSettings {
    pub style: Option<IconStyle>,
    /// Symbol weight, 100 to 700.
    #[schemars(range(min = 100, max = 700))]
    pub weight: Option<u16>,
    pub fill: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(default)]
pub struct MotionSettings {
    /// Replace spring motion by instant changes.
    pub reduced: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Density {
    Compact,
    #[default]
    Default,
    Comfortable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum TargetName {
    Css,
    Gpui,
    Rio,
    Hyprland,
    Web,
}

impl From<TargetName> for Target {
    fn from(name: TargetName) -> Self {
        match name {
            TargetName::Css => Self::Css,
            TargetName::Gpui => Self::Gpui,
            TargetName::Rio => Self::Rio,
            TargetName::Hyprland => Self::Hyprland,
            TargetName::Web => Self::Web,
        }
    }
}

/// The settings file.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Settings {
    /// Location of the JSON Schema of this file, for editors.
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub schema_ref: Option<String>,
    #[schemars(extend("x-group" = "appearance", "x-icon" = "palette"))]
    pub theme: ThemeSettings,
    /// BCP 47 language tag of the interface; the environment locale when unset.
    #[schemars(extend("x-group" = "language", "x-icon" = "language"))]
    pub locale: Option<String>,
    #[schemars(extend("x-group" = "appearance", "x-icon" = "interests"))]
    pub icons: IconSettings,
    /// Wallpaper file or URL; the theme's own wallpaper when unset.
    #[schemars(extend("x-group" = "appearance", "x-icon" = "wallpaper"))]
    pub wallpaper: Option<String>,
    #[schemars(extend("x-group" = "accessibility", "x-icon" = "animation"))]
    pub motion: MotionSettings,
    #[schemars(extend("x-group" = "appearance", "x-icon" = "density_medium"))]
    pub density: Density,
    /// Targets rendered on every theme change; all of them when unset.
    #[schemars(extend("x-group" = "appearance", "x-icon" = "output"))]
    pub targets: Option<Vec<TargetName>>,
    /// Commands run after a render, by target slug (css, gpui, rio, hyprland, web) or `all`; they see FW_THEME_NAME,
    /// FW_THEME_MODE and FW_THEME_DIR.
    #[schemars(extend("x-group" = "advanced", "x-icon" = "terminal", "x-restart-required" = false))]
    pub hooks: BTreeMap<String, Vec<String>>,
    /// Sections of other components, kept as written.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl Settings {
    /// A missing file is the default settings; a file that does not parse is an error naming the file.
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) if text.trim().is_empty() => Ok(Self::default()),
            Ok(text) => serde_json::from_str(&text).map_err(|source| Error::Json { path: path.to_owned(), source }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(Error::io(path, source)),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| Error::io(parent, source))?;
        }
        let mut text = serde_json::to_string_pretty(self).map_err(|source| Error::Json { path: path.to_owned(), source })?;
        text.push('\n');
        crate::targets::write_atomic(path, &text)
    }

    pub fn selected_targets(&self) -> Vec<Target> {
        self.targets.as_ref().map_or_else(|| Target::ALL.to_vec(), |names| names.iter().map(|name| Target::from(*name)).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_and_empty_files_are_defaults() -> Result<()> {
        let dir = tempfile::tempdir().map_err(|source| Error::io("tempdir", source))?;
        assert!(Settings::load(&dir.path().join("none.json"))?.theme.name.is_none());
        std::fs::write(dir.path().join("empty.json"), "  \n").map_err(|source| Error::io("empty", source))?;
        assert_eq!(Settings::load(&dir.path().join("empty.json"))?.density, Density::Default);
        Ok(())
    }

    #[test]
    fn unknown_sections_survive_a_round_trip() -> Result<()> {
        let dir = tempfile::tempdir().map_err(|source| Error::io("tempdir", source))?;
        let path = dir.path().join("settings.json");
        std::fs::write(&path, r#"{"theme":{"name":"nord","mode":"dark"},"terminal":{"font_size":14}}"#).map_err(|source| Error::io(&path, source))?;
        let settings = Settings::load(&path)?;
        assert_eq!(settings.theme.mode, ModePreference::Dark);
        settings.save(&path)?;
        let again = Settings::load(&path)?;
        assert_eq!(again.extra["terminal"]["font_size"], 14);
        assert_eq!(again.theme.name.as_deref(), Some("nord"));
        Ok(())
    }

    #[test]
    fn a_misspelled_key_inside_a_known_section_is_an_error() {
        let parsed: std::result::Result<Settings, _> = serde_json::from_str(r#"{"theme":{"nmae":"x"}}"#);
        assert!(parsed.is_err());
    }
}
