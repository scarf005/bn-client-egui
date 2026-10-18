// The 3D map: instanced quads, each `origin + u * a + v * b` (or an upright board that faces the eye) with an
// affine map from `(u, v)` to sprite pixels of one sheet strip.

struct Camera {
    view_proj: mat4x4<f32>,
    right: vec4<f32>,
    toward: vec4<f32>,
}

@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var sheet: texture_2d<f32>;
@group(1) @binding(1) var pixels: sampler;

struct Quad {
    @builtin(vertex_index) corner: u32,
    @location(0) origin: vec3<f32>,
    @location(1) a: vec3<f32>,
    @location(2) b: vec3<f32>,
    @location(3) uv: vec2<f32>,
    @location(4) uv_a: vec2<f32>,
    @location(5) uv_b: vec2<f32>,
    @location(6) flags: vec3<u32>,
}

struct Varying {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) flags: vec3<u32>,
}

@vertex
fn vs_main(q: Quad) -> Varying {
    // A strip of two triangles: (0, 0), (1, 0), (0, 1), (1, 1).
    let c = vec2<f32>(f32(q.corner & 1u), f32(q.corner >> 1u));
    var world: vec3<f32>;
    if (q.flags.x & 1u) != 0u {
        // A board: width across the screen from its left edge, height up from its bottom edge, and a step toward
        // the eye; a board at the near edge of its square steps there first, as `edge_push` does.
        var step = q.a.z;
        if (q.flags.x & 2u) != 0u {
            step += 0.5 / max(max(abs(camera.toward.x), abs(camera.toward.y)), 1e-6);
        }
        world = q.origin
            + camera.right.xyz * (q.a.y + c.x * q.a.x)
            + vec3<f32>(0.0, 0.0, q.b.y + c.y * q.b.x)
            + camera.toward.xyz * step;
    } else {
        world = q.origin + q.a * c.x + q.b * c.y;
    }
    var out: Varying;
    out.position = camera.view_proj * vec4<f32>(world, 1.0);
    out.uv = (q.uv + q.uv_a * c.x + q.uv_b * c.y) / vec2<f32>(textureDimensions(sheet));
    out.flags = q.flags;
    return out;
}

// The game's colour filters for what is not plainly in sight, as `Fx::pixel` on 0 to 255 channels.
fn filtered(mode: u32, rgb: vec3<f32>) -> vec3<f32> {
    if mode == 0u || (rgb.r == 0.0 && rgb.g == 0.0 && rgb.b == 0.0) {
        return rgb;
    }
    let average = floor(85.0 * (rgb.r + rgb.g + rgb.b) / 256.0);
    if mode == 1u {
        return vec3<f32>(max(floor(average * 5.0 / 8.0), 1.0));
    }
    let dark = vec3<f32>(39.0, 23.0, 19.0);
    let light = vec3<f32>(241.0, 220.0, 163.0);
    let shade = min(round(pow(average / 255.0, 1.6) * 150.0), 100.0);
    return floor((dark * (100.0 - shade) + light * shade) / 100.0);
}

@fragment
fn fs_main(v: Varying) -> @location(0) vec4<f32> {
    let texel = textureSampleLevel(sheet, pixels, v.uv, 0.0);
    if texel.a < 0.5 {
        discard;
    }
    let filtered_rgb = filtered((v.flags.x >> 8u) & 255u, round(texel.rgb * 255.0)) / 255.0;
    // Coloured light adds its colour, times the sprite's own, as the game draws it.
    let light = unpack4x8unorm(v.flags.z).rgb;
    let seen = min(filtered_rgb + filtered_rgb * light, vec3<f32>(1.0));
    // A level seen through open air is hazed toward cyan, more the deeper it is.
    let haze = f32(v.flags.y) / 255.0;
    return vec4<f32>(mix(seen, vec3<f32>(128.0, 255.0, 255.0) / 255.0, haze), 1.0);
}
