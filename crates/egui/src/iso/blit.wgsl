// Copies the finished 3D scene into the egui pass, pixel for pixel.

@group(0) @binding(0) var scene: texture_2d<f32>;
@group(0) @binding(1) var pixels: sampler;

struct Varying {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> Varying {
    // One triangle that covers the viewport.
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var out: Varying;
    out.position = vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2<f32>(p.x, 1.0 - p.y);
    return out;
}

@fragment
fn fs_main(v: Varying) -> @location(0) vec4<f32> {
    return textureSampleLevel(scene, pixels, v.uv, 0.0);
}

// For a framebuffer that converts to sRGB on write, while the scene holds sRGB-encoded colours already.
@fragment
fn fs_linear_framebuffer(v: Varying) -> @location(0) vec4<f32> {
    let c = textureSampleLevel(scene, pixels, v.uv, 0.0).rgb;
    let low = c / 12.92;
    let high = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return vec4<f32>(select(high, low, c <= vec3<f32>(0.04045)), 1.0);
}
