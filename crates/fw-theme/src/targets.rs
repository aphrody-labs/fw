//! One render per target from the same [`Resolved`] theme.
//!
//! css: `--md-sys-color-*` and `--fw-*` custom properties; gpui: a Zed/GPUI theme family JSON (`data/gpui-map.toml`);
//! rio and hyprland: omarchy-style templates (`data/templates`); web: the theme as JSON for wasm and scripts.
//! Extra `*.tpl` files in the theme's `templates/` directory render to the same output directory, as in omarchy.

use crate::color::hex;
use crate::error::{Error, Result};
use crate::resolve::{Mode, Resolved, View};
use crate::template::{self, Var, Vars};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const GPUI_MAP: &str = include_str!("../data/gpui-map.toml");
const RIO_TEMPLATE: &str = include_str!("../data/templates/rio.toml.tpl");
const HYPRLAND_TEMPLATE: &str = include_str!("../data/templates/hyprland.conf.tpl");

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Target {
    Css,
    Gpui,
    Rio,
    Hyprland,
    Web,
}

impl Target {
    pub const ALL: [Self; 5] = [Self::Css, Self::Gpui, Self::Rio, Self::Hyprland, Self::Web];

    pub fn slug(self) -> &'static str {
        match self {
            Self::Css => "css",
            Self::Gpui => "gpui",
            Self::Rio => "rio",
            Self::Hyprland => "hyprland",
            Self::Web => "web",
        }
    }

    pub fn file_name(self) -> &'static str {
        match self {
            Self::Css => "theme.css",
            Self::Gpui => "gpui-theme.json",
            Self::Rio => "rio-theme.toml",
            Self::Hyprland => "hyprland-colors.conf",
            Self::Web => "theme.json",
        }
    }

    pub fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|target| target.slug() == slug)
    }
}

pub fn render(theme: &Resolved, target: Target) -> Result<String> {
    match target {
        Target::Css => css(theme),
        Target::Gpui => gpui(theme),
        Target::Rio => template::render(RIO_TEMPLATE, &vars(theme)?, "rio.toml.tpl"),
        Target::Hyprland => template::render(HYPRLAND_TEMPLATE, &vars(theme)?, "hyprland.conf.tpl"),
        Target::Web => web(theme),
    }
}

/// Template variables of the active mode: palette keys, terminal keys, `m3_<role>` roles and the theme facts.
pub fn vars(theme: &Resolved) -> Result<Vars> {
    let view = theme.active_view()?;
    let mut vars = Vars::new();
    for (key, color) in &view.palette {
        vars.insert(key.clone(), Var::Color(*color));
    }
    for (key, color) in &view.terminal {
        vars.insert(key.clone(), Var::Color(*color));
    }
    for (role, color) in &view.roles {
        vars.insert(format!("m3_{}", role.replace('-', "_")), Var::Color(*color));
    }
    vars.insert("name".to_owned(), Var::Text(theme.name.clone()));
    vars.insert("mode".to_owned(), Var::Text(view.mode.as_str().to_owned()));
    vars.insert("icon_style".to_owned(), Var::Text(icon_style(theme).to_owned()));
    vars.insert("wallpaper".to_owned(), Var::Text(theme.wallpaper.clone().unwrap_or_default()));
    Ok(vars)
}

fn icon_style(theme: &Resolved) -> &'static str {
    match theme.icons.style {
        crate::theme::IconStyle::Outlined => fw_material::Style::Outlined.slug(),
        crate::theme::IconStyle::Rounded => fw_material::Style::Rounded.slug(),
        crate::theme::IconStyle::Sharp => fw_material::Style::Sharp.slug(),
    }
}

fn css_block(view: &View, indent: &str) -> String {
    let mut block = String::new();
    for (role, color) in &view.roles {
        block.push_str(&format!("{indent}--md-sys-color-{role}: {};\n", hex(*color)));
    }
    for (key, color) in &view.palette {
        block.push_str(&format!("{indent}--fw-{}: {};\n", key.replace('_', "-"), hex(*color)));
    }
    for (key, color) in &view.terminal {
        block.push_str(&format!("{indent}--fw-term-{}: {};\n", key.replace('_', "-"), hex(*color)));
    }
    block
}

fn css(theme: &Resolved) -> Result<String> {
    let mut out = format!("/* fw-theme {} (seed {}, {}); generated, do not edit */\n", theme.name, hex(theme.seed), theme.variant);
    match (theme.views.get(&Mode::Light), theme.views.get(&Mode::Dark)) {
        (Some(light), Some(dark)) => {
            out.push_str(&format!(":root {{\n  color-scheme: light;\n{}}}\n", css_block(light, "  ")));
            out.push_str(&format!("@media (prefers-color-scheme: dark) {{\n  :root:not([data-theme=\"light\"]) {{\n    color-scheme: dark;\n{}  }}\n}}\n", css_block(dark, "    ")));
            out.push_str(&format!(":root[data-theme=\"dark\"] {{\n  color-scheme: dark;\n{}}}\n", css_block(dark, "  ")));
        }
        _ => {
            let view = theme.active_view()?;
            out.push_str(&format!(":root {{\n  color-scheme: {};\n{}}}\n", view.mode.as_str(), css_block(view, "  ")));
        }
    }
    Ok(out)
}

#[derive(Debug, Deserialize)]
struct GpuiMap {
    schema: String,
    style: BTreeMap<String, String>,
    players: Vec<String>,
    selection_alpha: String,
    syntax: GpuiSyntax,
}

#[derive(Debug, Deserialize)]
struct GpuiSyntax {
    color: BTreeMap<String, String>,
    style: BTreeMap<String, GpuiSyntaxStyle>,
}

#[derive(Debug, Deserialize)]
struct GpuiSyntaxStyle {
    font_style: Option<String>,
    font_weight: Option<u32>,
}

fn gpui_map() -> Result<&'static GpuiMap> {
    static PARSED: OnceLock<std::result::Result<GpuiMap, String>> = OnceLock::new();
    PARSED
        .get_or_init(|| toml::from_str(GPUI_MAP).map_err(|error| format!("data/gpui-map.toml: {error}")))
        .as_ref()
        .map_err(|message| Error::Invalid(message.clone()))
}

/// `m3.surface@3d` to `#rrggbb3d`.
fn gpui_color(view: &View, expression: &str) -> Result<String> {
    let (source, alpha) = expression.split_once('@').map_or((expression, ""), |(source, alpha)| (source, alpha));
    let color = view.source(source).ok_or_else(|| Error::Invalid(format!("gpui-map.toml: `{expression}` names no color of the theme")))?;
    Ok(format!("{}{alpha}", hex(color)))
}

fn gpui_style(view: &View, map: &GpuiMap) -> Result<Value> {
    let mut style = Map::new();
    for (key, expression) in &map.style {
        style.insert(key.clone(), Value::String(gpui_color(view, expression)?));
    }
    let mut players = Vec::new();
    for expression in &map.players {
        let color = gpui_color(view, expression)?;
        players.push(json!({ "cursor": color, "background": color, "selection": format!("{color}{}", map.selection_alpha) }));
    }
    style.insert("players".to_owned(), Value::Array(players));
    let mut syntax = Map::new();
    for (group, expression) in &map.syntax.color {
        let extra = map.syntax.style.get(group);
        syntax.insert(
            group.clone(),
            json!({
                "color": gpui_color(view, expression)?,
                "font_style": extra.and_then(|extra| extra.font_style.clone()),
                "font_weight": extra.and_then(|extra| extra.font_weight),
            }),
        );
    }
    style.insert("syntax".to_owned(), Value::Object(syntax));
    Ok(Value::Object(style))
}

fn gpui(theme: &Resolved) -> Result<String> {
    let map = gpui_map()?;
    let mut themes = Vec::new();
    for (mode, view) in &theme.views {
        let suffix = if theme.views.len() > 1 { format!(" {}", if mode.is_dark() { "Dark" } else { "Light" }) } else { String::new() };
        themes.push(json!({ "name": format!("{}{suffix}", theme.title), "appearance": mode.as_str(), "style": gpui_style(view, map)? }));
    }
    let family = json!({ "$schema": map.schema, "name": theme.title, "author": theme.author.clone().unwrap_or_default(), "themes": themes });
    to_pretty(&family)
}

fn hex_map(colors: &BTreeMap<String, u32>) -> Value {
    Value::Object(colors.iter().map(|(key, color)| (key.clone(), Value::String(hex(*color)))).collect())
}

fn web(theme: &Resolved) -> Result<String> {
    let modes: Map<String, Value> = theme
        .views
        .iter()
        .map(|(mode, view)| (mode.as_str().to_owned(), json!({ "roles": hex_map(&view.roles), "palette": hex_map(&view.palette), "terminal": hex_map(&view.terminal) })))
        .collect();
    let style = icon_style(theme);
    let fill = if theme.icons.fill { "-fill" } else { "" };
    let base = fw_material::cdn::DEFAULT_BASE;
    let value = json!({
        "name": theme.name,
        "title": theme.title,
        "mode": theme.active.as_str(),
        "seed": hex(theme.seed),
        "variant": theme.variant,
        "modes": modes,
        "icons": {
            "set": "material-symbols",
            "style": style,
            "weight": theme.icons.weight,
            "fill": theme.icons.fill,
            "sprite": format!("{base}/sprites/{style}{fill}.svg"),
            "font_css": format!("{base}/fonts/material-symbols.css"),
        },
        "wallpaper": theme.wallpaper,
    });
    to_pretty(&value)
}

fn to_pretty(value: &Value) -> Result<String> {
    let mut text = serde_json::to_string_pretty(value).map_err(|source| Error::Json { path: PathBuf::from("<render>"), source })?;
    text.push('\n');
    Ok(text)
}

/// Writes every target and every `templates/*.tpl` of the theme into `out`, each file through a temporary file and a rename.
pub fn render_all(theme: &Resolved, targets: &[Target], out: &Path) -> Result<Vec<PathBuf>> {
    std::fs::create_dir_all(out).map_err(|source| Error::io(out, source))?;
    let mut written = Vec::new();
    for target in targets {
        let path = out.join(target.file_name());
        write_atomic(&path, &render(theme, *target)?)?;
        written.push(path);
    }
    if let Some(dir) = &theme.dir {
        let templates = dir.join("templates");
        let vars = vars(theme)?;
        let mut files: Vec<PathBuf> = std::fs::read_dir(&templates).into_iter().flatten().flatten().map(|entry| entry.path()).filter(|path| path.extension().is_some_and(|ext| ext == "tpl")).collect();
        files.sort();
        for file in files {
            let Some(stem) = file.file_stem() else { continue };
            let text = std::fs::read_to_string(&file).map_err(|source| Error::io(&file, source))?;
            let path = out.join(stem);
            write_atomic(&path, &template::render(&text, &vars, &file.display().to_string())?)?;
            written.push(path);
        }
    }
    Ok(written)
}

pub fn write_atomic(path: &Path, text: &str) -> Result<()> {
    let temporary = path.with_extension(format!("{}.tmp", path.extension().and_then(|ext| ext.to_str()).unwrap_or("out")));
    std::fs::write(&temporary, text).map_err(|source| Error::io(&temporary, source))?;
    std::fs::rename(&temporary, path).map_err(|source| Error::io(path, source))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brand::Brand;
    use crate::theme::Theme;

    fn resolved(text: &str) -> Result<Resolved> {
        let brand = Brand::parse("[brand]\nseed = \"#135b82\"\nvariant = \"vibrant\"\n", "test")?;
        Theme::parse(text, Path::new("test"), None)?.resolve(&brand, None)
    }

    #[test]
    fn every_target_renders_without_an_unresolved_key() -> Result<()> {
        let theme = resolved("name = \"t\"\n[m3]\nseed = \"brand\"\n")?;
        for target in Target::ALL {
            let text = render(&theme, target)?;
            assert!(!text.contains("{{"), "{}", target.slug());
            assert!(!text.is_empty());
        }
        Ok(())
    }

    #[test]
    fn css_has_both_modes_for_a_both_theme_and_one_block_otherwise() -> Result<()> {
        let both = css(&resolved("name = \"b\"\n")?)?;
        assert!(both.contains("prefers-color-scheme: dark"));
        assert!(both.contains("--md-sys-color-primary:"));
        let one = css(&resolved("name = \"o\"\nmode = \"dark\"\n[colors]\nbackground = \"#101010\"\nforeground = \"#eeeeee\"\naccent = \"#ff8800\"\n")?)?;
        assert!(!one.contains("prefers-color-scheme"));
        assert!(one.contains("--fw-accent: #ff8800;"));
        Ok(())
    }

    #[test]
    fn gpui_theme_is_a_zed_family_with_players_and_syntax() -> Result<()> {
        let value: Value = serde_json::from_str(&gpui(&resolved("name = \"g\"\n")?)?).map_err(|source| Error::Json { path: "gpui".into(), source })?;
        assert_eq!(value["themes"].as_array().map(Vec::len), Some(2));
        let style = &value["themes"][1]["style"];
        assert_eq!(style["players"].as_array().map(Vec::len), Some(8));
        assert!(style["syntax"]["keyword"]["color"].as_str().is_some_and(|c| c.starts_with('#')));
        assert!(style["ghost_element.hover"].as_str().is_some_and(|c| c.len() == 9));
        Ok(())
    }

    #[test]
    fn hyprland_and_rio_carry_the_explicit_palette() -> Result<()> {
        let theme = resolved("name = \"n\"\nmode = \"dark\"\n[colors]\nbackground = \"#2e3440\"\nforeground = \"#d8dee9\"\naccent = \"#81a1c1\"\n")?;
        assert!(render(&theme, Target::Hyprland)?.contains("$fw_accent = rgb(81a1c1)"));
        assert!(render(&theme, Target::Rio)?.contains("background = \"#2e3440\""));
        Ok(())
    }

    #[test]
    fn render_all_writes_target_files_and_theme_templates() -> Result<()> {
        let dir = tempfile::tempdir().map_err(|source| Error::io("tempdir", source))?;
        let theme_dir = dir.path().join("t");
        std::fs::create_dir_all(theme_dir.join("templates")).map_err(|source| Error::io(&theme_dir, source))?;
        std::fs::write(theme_dir.join("templates/btop.theme.tpl"), "main_bg=\"{{ background | bare }}\"\n").map_err(|source| Error::io(&theme_dir, source))?;
        let brand = Brand::parse("[brand]\nseed = \"#135b82\"\n", "test")?;
        let theme = Theme::parse("name = \"t\"\n", Path::new("test"), Some(theme_dir))?.resolve(&brand, None)?;
        let out = dir.path().join("out");
        let written = render_all(&theme, &Target::ALL, &out)?;
        assert_eq!(written.len(), Target::ALL.len() + 1);
        assert!(std::fs::read_to_string(out.join("btop.theme")).map_err(|source| Error::io(&out, source))?.starts_with("main_bg=\""));
        Ok(())
    }

    #[test]
    fn every_shipped_theme_resolves_and_renders_every_target() -> Result<()> {
        let brand = Brand::discover(Path::new(env!("CARGO_MANIFEST_DIR")))?;
        for (name, text) in crate::theme::BUNDLED {
            let theme = Theme::parse(text, Path::new(name), None)?.resolve(&brand, None)?;
            for target in Target::ALL {
                let text = render(&theme, target)?;
                assert!(!text.contains("{{"), "{name} {}", target.slug());
            }
        }
        Ok(())
    }
}
