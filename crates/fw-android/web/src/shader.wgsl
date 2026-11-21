// One textured quad: `rect` is the clip-space box (x0, y0, x1, y1), `uv` the sampled sub-rectangle, `tint` multiplies
// the color. The alpha channel of the guest surfaces is XRGB padding, so the output is always opaque.
struct Quad {
    rect: vec4<f32>,
    uv: vec4<f32>,
    tint: vec4<f32>,
}

@group(0) @binding(0) var<uniform> quad: Quad;
@group(0) @binding(1) var surface_texture: texture_2d<f32>;
@group(0) @binding(2) var surface_sampler: sampler;

struct Vertex {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vertex_main(@builtin(vertex_index) index: u32) -> Vertex {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
    );
    let corner = corners[index];
    var vertex: Vertex;
    vertex.position = vec4<f32>(mix(quad.rect.xy, quad.rect.zw, corner), 0.0, 1.0);
    vertex.uv = mix(quad.uv.xy, quad.uv.zw, corner);
    return vertex;
}

@fragment
fn fragment_main(vertex: Vertex) -> @location(0) vec4<f32> {
    let color = textureSample(surface_texture, surface_sampler, vertex.uv);
    return vec4<f32>(color.rgb * quad.tint.rgb, 1.0);
}
