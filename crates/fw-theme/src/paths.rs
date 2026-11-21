//! Where settings, user themes and rendered output live (XDG on Linux, `%APPDATA%` and `%LOCALAPPDATA%` on Windows).

use std::path::PathBuf;

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).filter(|value| !value.is_empty()).map(PathBuf::from)
}

fn home() -> Option<PathBuf> {
    env_path("HOME").or_else(|| env_path("USERPROFILE"))
}

/// `$FW_CONFIG_DIR`, `$XDG_CONFIG_HOME/fw`, `%APPDATA%\fw` or `~/.config/fw`.
pub fn config_dir() -> Option<PathBuf> {
    env_path("FW_CONFIG_DIR")
        .or_else(|| env_path("XDG_CONFIG_HOME").map(|dir| dir.join("fw")))
        .or_else(|| env_path("APPDATA").map(|dir| dir.join("fw")))
        .or_else(|| home().map(|dir| dir.join(".config").join("fw")))
}

/// `$FW_STATE_DIR`, `$XDG_STATE_HOME/fw`, `%LOCALAPPDATA%\fw` or `~/.local/state/fw`.
pub fn state_dir() -> Option<PathBuf> {
    env_path("FW_STATE_DIR")
        .or_else(|| env_path("XDG_STATE_HOME").map(|dir| dir.join("fw")))
        .or_else(|| env_path("LOCALAPPDATA").map(|dir| dir.join("fw")))
        .or_else(|| home().map(|dir| dir.join(".local").join("state").join("fw")))
}

pub fn settings_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("settings.json"))
}

/// Directory the render of the active theme is written to; compositors, terminals and apps include files from here.
pub fn current_dir() -> Option<PathBuf> {
    state_dir().map(|dir| dir.join("theme").join("current"))
}

/// Theme search path, highest priority first: `$FW_THEMES_DIR` (path-separated), the user directory, then every
/// `themes/` directory found from the working directory upward (the repository checkout).
pub fn theme_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(list) = std::env::var_os("FW_THEMES_DIR") {
        roots.extend(std::env::split_paths(&list));
    }
    if let Some(dir) = config_dir() {
        roots.push(dir.join("themes"));
    }
    if let Ok(cwd) = std::env::current_dir() {
        roots.extend(cwd.ancestors().map(|dir| dir.join("themes")).filter(|dir| dir.is_dir()));
    }
    roots.dedup();
    roots
}
