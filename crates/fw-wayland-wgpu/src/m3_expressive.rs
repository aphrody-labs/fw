//! Pure-Rust & Pure-WGPU/WGSL implementation of Compose Material 3 `1.5.0` Expressive
//! and Jetpack Compose `1.13` graphics primitives (`PolygonShape`/`MorphPolygonShape`,
//! 35 `MaterialShapes`, `WavyLinearProgressIndicator`, `WavyCircularProgressIndicator`,
//! `material3-ripple:1.5.0` inset focus rings, `SplitButton`, `FloatingToolbar`,
//! segmented `ListItem`, `SliderState`, and progressive blur).
//!
//! Strictly `#![forbid(unsafe_code)]` and zero C dependencies.

use serde::{Deserialize, Serialize};

/// Embedded pure WGSL shader implementing Material 3 1.5.0 Expressive SDFs,
/// wavy progress indicators, inset focus rings, and Compose 1.13 progressive blur.
pub const M3_EXPRESSIVE_WGSL: &str = include_str!("../../../core/webgpu/fw_m3_expressive.wgsl");

/// All 35 predefined Material Design 3 1.5.0 `MaterialShapes` with harmonic lobe counts
/// for GPU SDF evaluation and smooth `MorphPolygonShape` transitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum M3MaterialShape {
    Circle,
    Square,
    Slanted,
    Arch,
    Fan,
    Arrow,
    SemiCircle,
    Oval,
    Pill,
    Triangle,
    Diamond,
    ClamShell,
    Pentagon,
    Gem,
    Sunny,
    VerySunny,
    Cookie4Sided,
    Cookie6Sided,
    Cookie7Sided,
    Cookie9Sided,
    Cookie12Sided,
    Ghostish,
    Clover4Leaf,
    Clover8Leaf,
    Burst,
    SoftBurst,
    Boom,
    SoftBoom,
    Flower,
    Puffy,
    PuffyDiamond,
    PixelCircle,
    PixelTriangle,
    Bun,
    Heart,
}

impl M3MaterialShape {
    pub const ALL_35: [Self; 35] = [
        Self::Circle,
        Self::Square,
        Self::Slanted,
        Self::Arch,
        Self::Fan,
        Self::Arrow,
        Self::SemiCircle,
        Self::Oval,
        Self::Pill,
        Self::Triangle,
        Self::Diamond,
        Self::ClamShell,
        Self::Pentagon,
        Self::Gem,
        Self::Sunny,
        Self::VerySunny,
        Self::Cookie4Sided,
        Self::Cookie6Sided,
        Self::Cookie7Sided,
        Self::Cookie9Sided,
        Self::Cookie12Sided,
        Self::Ghostish,
        Self::Clover4Leaf,
        Self::Clover8Leaf,
        Self::Burst,
        Self::SoftBurst,
        Self::Boom,
        Self::SoftBoom,
        Self::Flower,
        Self::Puffy,
        Self::PuffyDiamond,
        Self::PixelCircle,
        Self::PixelTriangle,
        Self::Bun,
        Self::Heart,
    ];

    pub const fn shader_id(self) -> u32 {
        self as u32
    }

    pub const fn harmonic_lobes(self) -> f32 {
        match self {
            Self::Circle | Self::Oval | Self::Pill | Self::PixelCircle => 0.0,
            Self::Triangle | Self::PixelTriangle => 3.0,
            Self::Square | Self::Diamond | Self::Cookie4Sided | Self::Clover4Leaf | Self::PuffyDiamond => 4.0,
            Self::Pentagon => 5.0,
            Self::Cookie6Sided | Self::Gem => 6.0,
            Self::Cookie7Sided => 7.0,
            Self::Clover8Leaf | Self::Sunny | Self::Flower => 8.0,
            Self::Cookie9Sided | Self::SoftBurst => 9.0,
            Self::Cookie12Sided | Self::VerySunny | Self::Burst | Self::Boom | Self::SoftBoom => 12.0,
            Self::Heart => 2.0,
            _ => 4.0,
        }
    }
}

/// GPU-aligned (`vec4` packed) Material 3 1.5.0 Expressive draw instance for `fw_m3_expressive.wgsl`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct M3WgpuInstance {
    pub label: String,
    pub bounds: [f32; 4],
    pub fill_rgba: [f32; 4],
    pub accent_rgba: [f32; 4],
    pub shape_params: [f32; 4],
    pub effect_params: [f32; 4],
    pub widget_meta: [u32; 4],
}

/// Material 3 1.5.0 Expressive Spring physics solver (standard & expressive spatial/effects).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct M3SpringSpec {
    pub damping_ratio: f32,
    pub stiffness: f32,
}

impl M3SpringSpec {
    pub const EXPRESSIVE_FAST_SPATIAL: Self = Self { damping_ratio: 0.6, stiffness: 800.0 };
    pub const EXPRESSIVE_DEFAULT_SPATIAL: Self = Self { damping_ratio: 0.8, stiffness: 380.0 };
    pub const EXPRESSIVE_SLOW_SPATIAL: Self = Self { damping_ratio: 0.8, stiffness: 200.0 };
    pub const EXPRESSIVE_FAST_EFFECTS: Self = Self { damping_ratio: 1.0, stiffness: 3800.0 };
    pub const EXPRESSIVE_DEFAULT_EFFECTS: Self = Self { damping_ratio: 1.0, stiffness: 1600.0 };

    /// Semi-implicit Euler spring integration step for GPU uniform animation.
    pub fn step(self, pos: f32, vel: f32, target: f32, dt_sec: f32) -> (f32, f32) {
        let omega = self.stiffness.sqrt();
        let damping = 2.0 * self.damping_ratio * omega;
        let accel = -self.stiffness * (pos - target) - damping * vel;
        let next_vel = vel + accel * dt_sec;
        let next_pos = pos + next_vel * dt_sec;
        (next_pos, next_vel)
    }
}

/// Pure-WGPU Material 3 1.5.0 Expressive retained scene graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct M3WgpuScene {
    pub spec_version: String,
    pub compose_bom: String,
    pub viewport: (u32, u32),
    pub instances: Vec<M3WgpuInstance>,
    pub wgsl_bytes: usize,
}

impl M3WgpuScene {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            spec_version: "compose-material3-1.5.0".into(),
            compose_bom: "2026.11.01".into(),
            viewport: (width, height),
            instances: Vec::new(),
            wgsl_bytes: M3_EXPRESSIVE_WGSL.len(),
        }
    }

    pub fn push_surface(
        &mut self,
        label: &str,
        bounds: [f32; 4],
        fill_rgba: [f32; 4],
        accent_rgba: [f32; 4],
        corner_radius: f32,
        focused: bool,
    ) {
        self.instances.push(M3WgpuInstance {
            label: label.to_string(),
            bounds,
            fill_rgba,
            accent_rgba,
            shape_params: [corner_radius, 0.0, 0.0, 0.0],
            effect_params: [0.0, 0.0, 3.0, 0.0],
            widget_meta: [0, 0, 0, u32::from(focused)],
        });
    }

    pub fn push_morph_shape(
        &mut self,
        label: &str,
        bounds: [f32; 4],
        fill_rgba: [f32; 4],
        start: M3MaterialShape,
        end: M3MaterialShape,
        progress: f32,
    ) {
        self.instances.push(M3WgpuInstance {
            label: label.to_string(),
            bounds,
            fill_rgba,
            accent_rgba: fill_rgba,
            shape_params: [0.0, start.harmonic_lobes(), end.harmonic_lobes(), progress.clamp(0.0, 1.0)],
            effect_params: [progress.clamp(0.0, 1.0), 0.0, 0.0, 0.0],
            widget_meta: [1, start.shader_id(), end.shader_id(), 0],
        });
    }

    pub fn push_wavy_linear(
        &mut self,
        label: &str,
        bounds: [f32; 4],
        track_rgba: [f32; 4],
        active_rgba: [f32; 4],
        progress: f32,
        phase: f32,
    ) {
        self.instances.push(M3WgpuInstance {
            label: label.to_string(),
            bounds,
            fill_rgba: track_rgba,
            accent_rgba: active_rgba,
            shape_params: [4.0, 0.0, 0.0, 0.0],
            effect_params: [progress.clamp(0.0, 1.0), phase, 0.0, 0.0],
            widget_meta: [2, 0, 0, 0],
        });
    }

    pub fn push_stateful_slider(
        &mut self,
        label: &str,
        bounds: [f32; 4],
        track_rgba: [f32; 4],
        active_rgba: [f32; 4],
        value: f32,
    ) {
        self.instances.push(M3WgpuInstance {
            label: label.to_string(),
            bounds,
            fill_rgba: track_rgba,
            accent_rgba: active_rgba,
            shape_params: [4.0, 0.0, 0.0, 0.0],
            effect_params: [value.clamp(0.0, 1.0), 0.0, 0.0, 0.0],
            widget_meta: [4, 0, 0, 0],
        });
    }

    /// Builds a complete Material 3 1.5.0 Expressive showcase scene for WGPU / WebGPU rendering.
    pub fn expressive_showcase(width: u32, height: u32) -> Self {
        let mut s = Self::new(width, height);
        let w = width as f32;
        // TopAppBar (graduated stable in 1.5.0)
        s.push_surface("TopAppBar", [16.0, 16.0, w - 32.0, 56.0], [0.09, 0.13, 0.24, 1.0], [0.65, 0.71, 0.99, 1.0], 20.0, false);
        // SplitButton (focused with material3-ripple:1.5.0 inset focus ring)
        s.push_surface("SplitButton.Primary", [24.0, 92.0, 180.0, 44.0], [0.22, 0.19, 0.64, 1.0], [0.65, 0.71, 0.99, 1.0], 20.0, true);
        s.push_surface("SplitButton.Menu", [208.0, 92.0, 56.0, 44.0], [0.19, 0.18, 0.51, 1.0], [0.65, 0.71, 0.99, 1.0], 14.0, false);
        // MorphPolygonShape (Cookie9Sided -> SoftBurst)
        s.push_morph_shape("LoadingIndicator.Morph", [288.0, 84.0, 64.0, 64.0], [0.37, 0.92, 0.83, 1.0], M3MaterialShape::Cookie9Sided, M3MaterialShape::SoftBurst, 0.45);
        // WavyLinearProgressIndicator (stable in 1.5.0)
        s.push_wavy_linear("WavyLinearProgressIndicator", [24.0, 164.0, w - 48.0, 18.0], [0.18, 0.25, 0.36, 1.0], [0.65, 0.71, 0.99, 1.0], 0.72, 0.8);
        // StatefulSlider (SliderState in 1.5.0)
        s.push_stateful_slider("StatefulSlider", [24.0, 200.0, w - 48.0, 28.0], [0.18, 0.25, 0.36, 1.0], [0.37, 0.92, 0.83, 1.0], 0.64);
        // Segmented ListItems (stable in 1.5.0)
        s.push_surface("SegmentedListItem.0", [24.0, 248.0, w - 48.0, 48.0], [0.12, 0.16, 0.26, 1.0], [0.65, 0.71, 0.99, 1.0], 18.0, false);
        s.push_surface("SegmentedListItem.1", [24.0, 300.0, w - 48.0, 48.0], [0.12, 0.16, 0.26, 1.0], [0.65, 0.71, 0.99, 1.0], 18.0, false);
        // HorizontalFloatingToolbar (stable in 1.5.0)
        s.push_surface("HorizontalFloatingToolbar", [(w - 280.0).max(24.0) * 0.5, (height as f32) - 72.0, 280.0, 52.0], [0.19, 0.18, 0.51, 1.0], [0.99, 0.73, 0.45, 1.0], 26.0, true);
        s
    }

    /// Builds a parametric prototype WGPU scene driven by live decoupled design tokens
    /// and optional interactive spec overlays (keylines / spatial measurements).
    pub fn from_parametric(cfg: &M3ParametricConfig, width: u32, height: u32) -> Self {
        let safe = cfg.enforce_constraints();
        let mut s = Self::new(width, height);
        let w = width as f32;
        let pad = safe.padding_px;
        let inner_w = (w - pad * 2.0).max(64.0);

        s.push_surface(
            "Parametric.Container",
            [pad, pad, inner_w, 96.0],
            [0.12, 0.16, 0.26, 1.0],
            [0.65, 0.71, 0.99, 1.0],
            safe.outer_corner_radius,
            safe.spec_keylines,
        );
        s.push_surface(
            "Parametric.InnerChip",
            [pad * 2.0, pad + 20.0, (inner_w - pad * 2.0).max(48.0), 48.0],
            [0.22, 0.19, 0.64, 1.0],
            [0.37, 0.92, 0.83, 1.0],
            safe.inner_corner_radius,
            safe.spec_keylines,
        );
        s.push_wavy_linear(
            "Parametric.WavyProgress",
            [pad, pad + 116.0, inner_w, 18.0],
            [0.18, 0.25, 0.36, 1.0],
            [0.65, 0.71, 0.99, 1.0],
            0.68,
            safe.wave_phase,
        );
        if safe.spec_keylines {
            // Vertical keyline guide bars for interactive spec mode
            s.push_surface("Spec.Keyline.Left", [pad, 0.0, 1.0, height as f32], [0.99, 0.73, 0.45, 0.65], [0.99, 0.73, 0.45, 1.0], 0.0, false);
            s.push_surface("Spec.Keyline.Right", [w - pad, 0.0, 1.0, height as f32], [0.99, 0.73, 0.45, 0.65], [0.99, 0.73, 0.45, 1.0], 0.0, false);
        }
        s
    }
}

/// Decoupled parametric prototyping settings for pure-WGPU Material 3 components
/// (aligned with https://m3.material.io/blog/enhance-your-component-designs-with-parametric-prototyping).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct M3ParametricConfig {
    pub outer_corner_radius: f32,
    pub inner_corner_radius: f32,
    pub padding_px: f32,
    pub wave_phase: f32,
    pub spec_keylines: bool,
}

impl M3ParametricConfig {
    /// Tethers inner corner radius to outer corner radius and padding so components never clip.
    pub fn enforce_constraints(self) -> Self {
        let outer = self.outer_corner_radius.clamp(0.0, 36.0);
        let pad = self.padding_px.clamp(4.0, 32.0);
        let max_inner = (outer - pad * 0.5).max(2.0);
        Self {
            outer_corner_radius: outer,
            inner_corner_radius: self.inner_corner_radius.clamp(2.0, max_inner),
            padding_px: pad,
            wave_phase: self.wave_phase,
            spec_keylines: self.spec_keylines,
        }
    }

    /// Deterministic xorshift64 randomization to break bias and stress-test edge cases.
    pub fn randomize_seeded(mut seed: u64, spec_keylines: bool) -> Self {
        if seed == 0 {
            seed = 0x9E37_79B9_7F4A_7C15;
        }
        let mut next_f32 = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            ((seed >> 40) as f32) / ((1u64 << 24) as f32)
        };
        let raw = Self {
            outer_corner_radius: 12.0 + next_f32() * 16.0,
            inner_corner_radius: 4.0 + next_f32() * 14.0,
            padding_px: 8.0 + next_f32() * 12.0,
            wave_phase: next_f32() * 6.283_185_5,
            spec_keylines,
        };
        raw.enforce_constraints()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn m3_expressive_wgpu_shader_and_35_shapes_are_valid() {
        assert_eq!(M3MaterialShape::ALL_35.len(), 35);
        assert!(M3_EXPRESSIVE_WGSL.contains("fn vs_main"));
        assert!(M3_EXPRESSIVE_WGSL.contains("fn fs_main"));
        assert!(M3_EXPRESSIVE_WGSL.contains("sdf_wavy_linear"));
        assert!(M3_EXPRESSIVE_WGSL.contains("sdf_material_polygon"));

        let scene = M3WgpuScene::expressive_showcase(1280, 720);
        assert_eq!(scene.spec_version, "compose-material3-1.5.0");
        assert_eq!(scene.compose_bom, "2026.11.01");
        assert_eq!(scene.instances.len(), 9);

        // Verify parametric WGPU scene with constraint tethering and keyline spec overlay
        let cfg = M3ParametricConfig::randomize_seeded(20261116, true);
        assert!(cfg.inner_corner_radius <= (cfg.outer_corner_radius - cfg.padding_px * 0.5).max(2.0) + 1e-4);
        let p_scene = M3WgpuScene::from_parametric(&cfg, 960, 540);
        assert_eq!(p_scene.instances.len(), 5);
        assert!(p_scene.instances.iter().any(|i| i.label == "Spec.Keyline.Left"));

        // Verify expressive spring converges smoothly toward target 1.0
        let (mut pos, mut vel) = (0.0f32, 0.0f32);
        for _ in 0..60 {
            (pos, vel) = M3SpringSpec::EXPRESSIVE_DEFAULT_SPATIAL.step(pos, vel, 1.0, 1.0 / 60.0);
        }
        assert!((pos - 1.0).abs() < 0.05, "spring pos={pos}");
    }
}
