//! The configuration and personalization layer of the monorepo.
//!
//! A theme is a directory (`themes/<name>/theme.toml`, optional omarchy `colors.toml`, `backgrounds/`, `templates/`).
//! [`Resolved`] turns it into Material 3 roles (through `fw-m3`), a terminal palette, an icon set and a wallpaper;
//! [`targets`] renders that to CSS variables, GPUI theme JSON, rio, Hyprland and web JSON; [`settings`] and [`schema`]
//! hold the user settings file and its JSON Schema; [`apply`] is `fw theme switch`; [`watch`] reloads on file changes.

pub mod apply;
pub mod brand;
pub mod catalog;
pub mod cli;
pub mod color;
pub mod derive;
pub mod error;
pub mod paths;
pub mod resolve;
pub mod schema;
pub mod settings;
pub mod targets;
pub mod template;
pub mod theme;
pub mod watch;

pub use error::{Error, Result};
pub use resolve::{Mode, Resolved, View};
pub use schema::SchemaFragment;
pub use settings::Settings;
pub use targets::Target;
pub use theme::{Theme, ThemeFile, ThemeStore};
