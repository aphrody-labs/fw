//! `fw android` core: pins, image fetch and the Linux container lifecycle.

pub mod cli;
pub mod image;
pub mod pins;
pub mod props;
pub mod state;

#[cfg(target_os = "linux")]
pub mod engine;
#[cfg(target_os = "linux")]
pub mod guest;
#[cfg(target_os = "linux")]
pub mod host;
#[cfg(target_os = "linux")]
pub mod session;
#[cfg(target_os = "linux")]
pub mod spec;
