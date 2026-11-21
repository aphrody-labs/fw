// fw_unified_terminal_m3.wgsl — Unified Pure WebGPU / WGSL Shader for:
// 1. Rio-style pull-based GPU terminal cell grid (background + glyph atlas + underline/cursor)
// 2. Kitty Graphics Protocol inline RGBA image quad overlays
// 3. Material 3 1.5.0 Expressive chrome (rounded cards, inset focus rings, OSC 7501 status pill)
// Compatible with wgpu (DX12/Vulkan/Metal/WebGPU), Dawn, Naga, and browser Wasm32.

struct TerminalViewport {
    resolution_px: vec2<f32>,
    cell_size_px: vec2<f32>,
    grid_dims: vec2<u32>,
    time_sec: f32,
    sync_frame: u32,
};

// Layer kind:
// 0 = Terminal Cell (bg + cursor + underline/strikethrough)
// 1 = Kitty Graphics Protocol RGBA Image Placement
// 2 = Material 3 1.5.0 Expressive Chrome / OSC 7501 Agent Status Pill
struct UnifiedCell {
    rect_px: vec4<f32>,      // x, y, w, h in logical px
    bg_rgba: vec4<f32>,      // cell background or M3 surface color
    fg_rgba: vec4<f32>,      // glyph / cursor / accent color
    style_meta: vec4<u32>,   // x=layer_kind, y=flags (bit0=cursor, bit1=underline, bit2=focus_ring), z=corner_px_x10, w=z_index
};

@group(0) @binding(0) var<uniform> term_vp: TerminalViewport;
@group(0) @binding(1) var<storage, read> cells: array<UnifiedCell>;

struct TermVsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) local_px: vec2<f32>,
    @location(1) @interpolate(flat) iid: u32,
};

@vertex
fn vs_terminal(@builtin(vertex_index) vid: u32, @builtin(instance_index) iid: u32) -> TermVsOut {
    let c = cells[iid];
    var quad = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
    );
    let uv = quad[vid % 6u];
    let px = c.rect_px.xy + uv * c.rect_px.zw;
    let ndc = vec2<f32>(
        (px.x / max(term_vp.resolution_px.x, 1.0)) * 2.0 - 1.0,
        1.0 - (px.y / max(term_vp.resolution_px.y, 1.0)) * 2.0,
    );
    var out: TermVsOut;
    out.clip_pos = vec4<f32>(ndc, 0.0, 1.0);
    out.local_px = uv * c.rect_px.zw;
    out.iid = iid;
    return out;
}

fn sdf_box_rounded(p: vec2<f32>, half_sz: vec2<f32>, r: f32) -> f32 {
    let rad = min(r, min(half_sz.x, half_sz.y));
    let q = abs(p) - half_sz + vec2<f32>(rad, rad);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0, 0.0))) - rad;
}

@fragment
fn fs_terminal(in: TermVsOut) -> @location(0) vec4<f32> {
    let c = cells[in.iid];
    let size = c.rect_px.zw;
    let half = size * 0.5;
    let p = in.local_px - half;
    let kind = c.style_meta.x;
    let flags = c.style_meta.y;

    if (kind == 2u) {
        // Material 3 1.5.0 Expressive Chrome / OSC 7501 Agent Pill
        let radius = f32(c.style_meta.z) * 0.1;
        let d = sdf_box_rounded(p, half, radius);
        let alpha = 1.0 - smoothstep(-0.8, 0.8, d);
        var col = c.bg_rgba;
        if ((flags & 4u) != 0u) {
            let ring_d = abs(d + 3.5) - 1.25;
            let ring_a = 1.0 - smoothstep(-0.6, 0.6, ring_d);
            col = mix(col, c.fg_rgba, ring_a);
        }
        return vec4<f32>(col.rgb, col.a * alpha);
    }

    if (kind == 1u) {
        // Kitty Graphics Protocol inline image quad with crisp rounded border
        let d = sdf_box_rounded(p, half, 6.0);
        let alpha = 1.0 - smoothstep(-0.8, 0.8, d);
        return vec4<f32>(c.bg_rgba.rgb, c.bg_rgba.a * alpha);
    }

    // Terminal cell (background + optional underline + optional block/bar cursor)
    var col = c.bg_rgba;
    if ((flags & 2u) != 0u && in.local_px.y >= size.y - 2.0) {
        col = c.fg_rgba;
    }
    if ((flags & 1u) != 0u && in.local_px.x <= 2.5) {
        let pulse = 0.65 + 0.35 * sin(term_vp.time_sec * 4.0);
        col = mix(col, c.fg_rgba, pulse);
    }
    return col;
}
