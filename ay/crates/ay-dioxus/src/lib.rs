// SPDX-License-Identifier: Apache-2.0
//! Unified `DioxusLabs/dioxus` (`aphrody-labs/dioxus` 0.8) + `taffy` + fused Aphrody GUI/NSH SDK crate.
//!
//! Combines:
//! - Dioxus `VirtualDom`, `rsx!`, `Signal<T>`, and `dioxus_ssr::render` from our patched fork,
//! - Taffy Flexbox/Grid layout computation (`aphrody-labs/taffy`),
//! - Aphrody Native Shell (`nsh` / `nysh`) TSX <-> RSX component descriptors & live WebGPU terminal slots,
//! - Material 3 brand seed & surface tokens from `ay-brand` and COSMIC Epoch responsive breakpoints.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use taffy::prelude::*;

pub use ::dioxus;
pub use ::dioxus_core as core;
pub use ::dioxus_signals as signals;
pub use ::dioxus_ssr as ssr;
pub use ::taffy;

/// COSMIC Epoch & Material 3 adaptive layout breakpoint classes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CosmicBreakpoint {
    Compact,
    Medium,
    Expanded,
    UltraWide,
}

impl CosmicBreakpoint {
    #[must_use]
    pub const fn from_width_px(width: f32) -> Self {
        if width < 600.0 {
            Self::Compact
        } else if width < 1024.0 {
            Self::Medium
        } else if width < 1600.0 {
            Self::Expanded
        } else {
            Self::UltraWide
        }
    }
}

/// Fused Aphrody GUI + Native Shell (`nsh` / `nysh`) surface descriptor rendered via Dioxus RSX or GPUI.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NshSurfaceSpec {
    pub id: String,
    pub title: String,
    pub entry_tsx_or_rsx: String,
    pub nysh_command: Option<String>,
    pub webgpu_canvas: bool,
}

/// Props for the canonical Aphrody Dioxus + NSH App Shell component.
#[derive(Props, Clone, PartialEq)]
pub struct NshShellProps {
    pub app_title: String,
    pub active_route: String,
    pub vpn_exit: String,
    pub surfaces: Vec<NshSurfaceSpec>,
}

/// Canonical Dioxus 0.8 RSX component rendering an Aphrody M3 + COSMIC + NSH shell cleanly to HTML/VDOM.
#[component]
pub fn AphrodyNshShell(props: NshShellProps) -> Element {
    let digest = &ay_brand::sha256_hex(props.app_title.as_bytes())[..12];
    rsx! {
        div {
            class: "ay-dioxus-nsh-shell",
            "data-shell-digest": "{digest}",
            "data-vpn-exit": "{props.vpn_exit}",
            header {
                class: "ay-cosmic-top-panel",
                h1 { "{props.app_title}" }
                span { class: "ay-route-pill", "{props.active_route}" }
                span { class: "ay-vpn-pill", "VPN:{props.vpn_exit}" }
            }
            main {
                class: "ay-nsh-workspace-grid",
                for surface in props.surfaces.iter() {
                    section {
                        key: "{surface.id}",
                        "data-surface-id": "{surface.id}",
                        "data-webgpu": "{surface.webgpu_canvas}",
                        h2 { "{surface.title}" }
                        code { "{surface.entry_tsx_or_rsx}" }
                    }
                }
            }
        }
    }
}

/// Renders [`AphrodyNshShell`] deterministically via `dioxus_ssr` for edge Workers, SSG, and conformance tests.
#[must_use]
pub fn render_nsh_shell_ssr(props: NshShellProps) -> String {
    let mut vdom = VirtualDom::new_with_props(AphrodyNshShell, props);
    vdom.rebuild_in_place();
    dioxus_ssr::render(&vdom)
}

/// Computes a 2-region COSMIC/NSH split layout (navigation rail + WebGPU workspace) using our `taffy` fork.
pub fn compute_nsh_split_layout(
    viewport_w: f32,
    viewport_h: f32,
) -> Result<([f32; 4], [f32; 4]), taffy::TaffyError> {
    let mut tree: TaffyTree<()> = TaffyTree::new();
    let rail_w = match CosmicBreakpoint::from_width_px(viewport_w) {
        CosmicBreakpoint::Compact => 0.0,
        CosmicBreakpoint::Medium => 72.0,
        CosmicBreakpoint::Expanded | CosmicBreakpoint::UltraWide => 260.0,
    };
    let rail = tree.new_leaf(Style {
        size: Size {
            width: Dimension::length(rail_w),
            height: Dimension::percent(1.0),
        },
        ..Default::default()
    })?;
    let main = tree.new_leaf(Style {
        flex_grow: 1.0,
        size: Size {
            width: Dimension::auto(),
            height: Dimension::percent(1.0),
        },
        ..Default::default()
    })?;
    let root = tree.new_with_children(
        Style {
            display: Display::Flex,
            size: Size {
                width: Dimension::length(viewport_w),
                height: Dimension::length(viewport_h),
            },
            ..Default::default()
        },
        &[rail, main],
    )?;
    tree.compute_layout(
        root,
        Size {
            width: AvailableSpace::Definite(viewport_w),
            height: AvailableSpace::Definite(viewport_h),
        },
    )?;
    let r = tree.layout(rail)?;
    let m = tree.layout(main)?;
    Ok((
        [r.location.x, r.location.y, r.size.width, r.size.height],
        [m.location.x, m.location.y, m.size.width, m.size.height],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dioxus_rsx_and_nsh_shell_render_valid_ssr_markup() {
        let html = render_nsh_shell_ssr(NshShellProps {
            app_title: "Aphrody SDK Studio".into(),
            active_route: "/workspace/webgpu".into(),
            vpn_exit: "us".into(),
            surfaces: vec![
                NshSurfaceSpec {
                    id: "nysh-term".into(),
                    title: "Nysh WebGPU Terminal".into(),
                    entry_tsx_or_rsx: "surfaces/nysh.tsx".into(),
                    nysh_command: Some("ay check".into()),
                    webgpu_canvas: true,
                },
                NshSurfaceSpec {
                    id: "cf-os-gadgets".into(),
                    title: "Cloudflare-OS Gadgets".into(),
                    entry_tsx_or_rsx: "surfaces/gadgets.rs".into(),
                    nysh_command: None,
                    webgpu_canvas: false,
                },
            ],
        });
        assert!(html.contains("Aphrody SDK Studio"));
        assert!(html.contains("VPN:us"));
        assert!(html.contains("Nysh WebGPU Terminal"));
        assert!(html.contains("data-webgpu=\"true\""));
    }

    #[test]
    fn taffy_computes_responsive_cosmic_nsh_split() {
        let (rail, main) = compute_nsh_split_layout(1440.0, 900.0).expect("taffy layout");
        assert_eq!(rail[2], 260.0);
        assert_eq!(main[0], 260.0);
        assert_eq!(main[2], 1180.0);
    }
}
