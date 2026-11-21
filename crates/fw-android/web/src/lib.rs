//! Browser client of `fw android`: a WebGPU viewer for the windows the guest compositor streams with fwa-stream v1.
//!
//! The geometry, keymap, demo compositor and page generator are plain Rust and tested on the host; the viewer and the
//! feasibility probes need `wasm32-unknown-unknown` and a browser with WebGPU (`fwa-web-pack` builds them).

pub mod demo;
pub mod keymap;
pub mod layout;
pub mod loader;
pub mod palette;

#[cfg(target_arch = "wasm32")]
mod probe;
#[cfg(target_arch = "wasm32")]
mod viewer;

#[cfg(target_arch = "wasm32")]
pub use probe::probe;
#[cfg(target_arch = "wasm32")]
pub use viewer::{start, Viewer};
