//! `fw theme switch`: write the choice to settings, render every target into the state directory and run the hooks.

use crate::brand::Brand;
use crate::error::{Error, Result};
use crate::paths;
use crate::resolve::{Mode, Resolved};
use crate::settings::{ModePreference, Settings};
use crate::targets::{self, Target};
use crate::theme::ThemeStore;
use std::path::PathBuf;
use std::process::Command;

pub struct Context {
    pub store: ThemeStore,
    pub brand: Brand,
    pub settings_path: PathBuf,
    pub out_dir: PathBuf,
}

#[derive(Debug)]
pub struct Applied {
    pub theme: String,
    pub mode: Mode,
    pub files: Vec<PathBuf>,
    /// Hook commands that failed to start or exited non-zero; the render itself succeeded.
    pub hook_failures: Vec<String>,
}

impl Context {
    pub fn discover() -> Result<Self> {
        let cwd = std::env::current_dir().map_err(|source| Error::io(".", source))?;
        Ok(Self {
            store: ThemeStore::discover(),
            brand: Brand::discover(&cwd)?,
            settings_path: paths::settings_path().ok_or_else(|| Error::Invalid("no config directory: set FW_CONFIG_DIR or HOME".to_owned()))?,
            out_dir: paths::current_dir().ok_or_else(|| Error::Invalid("no state directory: set FW_STATE_DIR or HOME".to_owned()))?,
        })
    }

    /// The theme the settings select, with the icon and wallpaper overrides of the settings applied.
    pub fn resolve(&self, settings: &Settings) -> Result<Resolved> {
        let name = match &settings.theme.name {
            Some(name) => name.clone(),
            None => self.store.default_name().ok_or_else(|| Error::Invalid("no theme found".to_owned()))?,
        };
        let mut resolved = self.store.load(&name)?.resolve(&self.brand, settings.theme.mode.mode())?;
        if let Some(style) = settings.icons.style {
            resolved.icons.style = style;
        }
        if let Some(weight) = settings.icons.weight {
            resolved.icons.weight = weight;
        }
        if let Some(fill) = settings.icons.fill {
            resolved.icons.fill = fill;
        }
        if let Some(wallpaper) = &settings.wallpaper {
            resolved.wallpaper = Some(wallpaper.clone());
        }
        Ok(resolved)
    }

    pub fn apply(&self, settings: &Settings) -> Result<Applied> {
        let resolved = self.resolve(settings)?;
        let targets = settings.selected_targets();
        let mut files = targets::render_all(&resolved, &targets, &self.out_dir)?;
        if let Some(copy) = copy_wallpaper(&resolved, &self.out_dir)? {
            files.push(copy);
        }
        let hook_failures = run_hooks(settings, &resolved, &targets, &self.out_dir);
        Ok(Applied { theme: resolved.name.clone(), mode: resolved.active, files, hook_failures })
    }

    /// Selects `name` (and optionally a mode preference), saves the settings and applies them.
    pub fn switch(&self, name: &str, mode: Option<ModePreference>) -> Result<Applied> {
        self.store.load(name)?;
        let mut settings = Settings::load(&self.settings_path)?;
        settings.theme.name = Some(name.to_owned());
        if let Some(mode) = mode {
            settings.theme.mode = mode;
        }
        let applied = self.apply(&settings)?;
        settings.save(&self.settings_path)?;
        Ok(applied)
    }
}

fn copy_wallpaper(theme: &Resolved, out: &std::path::Path) -> Result<Option<PathBuf>> {
    let Some(source) = theme.wallpaper.as_deref().map(PathBuf::from).filter(|path| path.is_file()) else { return Ok(None) };
    let name = source.extension().map_or_else(|| "wallpaper".to_owned(), |ext| format!("wallpaper.{}", ext.to_string_lossy()));
    let target = out.join(name);
    std::fs::copy(&source, &target).map_err(|error| Error::io(&source, error))?;
    Ok(Some(target))
}

fn shell(command: &str) -> Command {
    let mut shell = if cfg!(windows) { Command::new("cmd") } else { Command::new("sh") };
    shell.arg(if cfg!(windows) { "/C" } else { "-c" }).arg(command);
    shell
}

fn run_hooks(settings: &Settings, theme: &Resolved, targets: &[Target], out: &std::path::Path) -> Vec<String> {
    let keys: Vec<&str> = targets.iter().map(|target| target.slug()).chain(std::iter::once("all")).collect();
    let mut failures = Vec::new();
    for key in keys {
        for command in settings.hooks.get(key).into_iter().flatten() {
            let status = shell(command).env("FW_THEME_NAME", &theme.name).env("FW_THEME_MODE", theme.active.as_str()).env("FW_THEME_DIR", out).status();
            match status {
                Ok(status) if status.success() => {}
                Ok(status) => failures.push(format!("{key}: `{command}` exited with {status}")),
                Err(error) => failures.push(format!("{key}: `{command}`: {error}")),
            }
        }
    }
    failures
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(dir: &std::path::Path) -> Result<Context> {
        let themes = dir.join("themes/nord");
        std::fs::create_dir_all(&themes).map_err(|source| Error::io(&themes, source))?;
        std::fs::write(themes.join("theme.toml"), "name = \"nord\"\nmode = \"dark\"\n[colors]\nbackground = \"#2e3440\"\nforeground = \"#d8dee9\"\naccent = \"#81a1c1\"\n").map_err(|source| Error::io(&themes, source))?;
        Ok(Context {
            store: ThemeStore::new(vec![dir.join("themes")]),
            brand: Brand::parse("[brand]\nseed = \"#135b82\"\n", "test")?,
            settings_path: dir.join("config/settings.json"),
            out_dir: dir.join("state/current"),
        })
    }

    #[test]
    fn switch_saves_the_choice_renders_every_target_and_runs_hooks() -> Result<()> {
        let dir = tempfile::tempdir().map_err(|source| Error::io("tempdir", source))?;
        let context = context(dir.path())?;
        let marker = dir.path().join("hook-ran");
        let mut settings = Settings::default();
        settings.hooks.insert("all".to_owned(), vec![format!("echo \"$FW_THEME_NAME\" > {}", marker.display())]);
        settings.save(&context.settings_path)?;
        let applied = context.switch("nord", None)?;
        assert_eq!(applied.theme, "nord");
        assert_eq!(applied.mode, Mode::Dark);
        assert_eq!(applied.files.len(), Target::ALL.len());
        assert!(std::fs::read_to_string(context.out_dir.join("rio-theme.toml")).map_err(|source| Error::io("rio", source))?.contains("#2e3440"));
        assert_eq!(Settings::load(&context.settings_path)?.theme.name.as_deref(), Some("nord"));
        if cfg!(unix) {
            assert_eq!(std::fs::read_to_string(&marker).map_err(|source| Error::io(&marker, source))?.trim(), "nord");
            assert!(applied.hook_failures.is_empty());
        }
        Ok(())
    }

    #[test]
    fn switching_to_an_unknown_theme_changes_nothing() -> Result<()> {
        let dir = tempfile::tempdir().map_err(|source| Error::io("tempdir", source))?;
        let context = context(dir.path())?;
        assert!(matches!(context.switch("missing", None), Err(Error::UnknownTheme { .. })));
        assert!(!context.settings_path.exists());
        Ok(())
    }
}
