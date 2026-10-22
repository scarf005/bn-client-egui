// The 3D map: instanced quads, each `origin + u * a + v * b` (or an upright board that faces the eye) with an
// affine map from `(u, v)` to sprite pixels of one sheet strip.

struct Camera {
    view_proj: mat4x4<f32>,
    // The view of the light shadows are cast from.
    light: mat4x4<f32>,
    right: vec4<f32>,
    toward: vec4<f32>,
    // The horizontal unit vector toward the light.
    sun: vec4<f32>,
}

@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var sheet: texture_2d<f32>;
@group(1) @binding(1) var pixels: sampler;
@group(2) @binding(0) var shadows: texture_depth_2d;
@group(2) @binding(1) var shadowed: sampler_comparison;

// How bright a square in sight is where something stands between it and the light.
const SHADE: f32 = 0.6;

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
    // Where the point lies in the light's view.
    @location(2) light: vec3<f32>,
}

// Where the corner of quad `q` this vertex draws lies in the world, its boards facing the horizontal unit vector
// `toward` with `right` on their right.
fn world_of(q: Quad, right: vec3<f32>, toward: vec3<f32>) -> vec3<f32> {
    // A strip of two triangles: (0, 0), (1, 0), (0, 1), (1, 1).
    let c = vec2<f32>(f32(q.corner & 1u), f32(q.corner >> 1u));
    var world: vec3<f32>;
    if (q.flags.x & 1u) != 0u {
        // A board: width across the screen from its left edge, height up from its bottom edge, and a step toward
        // the eye; a board at the near edge of its square steps there first, as `edge_push` does.
        var step = q.a.z;
        if (q.flags.x & 2u) != 0u {
            step += 0.5 / max(max(abs(toward.x), abs(toward.y)), 1e-6);
        }
        world = q.origin
            + right * (q.a.y + c.x * q.a.x)
            + vec3<f32>(0.0, 0.0, q.b.y + c.y * q.b.x)
            + toward * step;
    } else {
        world = q.origin + q.a * c.x + q.b * c.y;
    }
    return world;
}

fn vertex(q: Quad, view_proj: mat4x4<f32>, toward: vec3<f32>) -> Varying {
    let c = vec2<f32>(f32(q.corner & 1u), f32(q.corner >> 1u));
    let world = vec4<f32>(world_of(q, vec3<f32>(toward.y, -toward.x, 0.0), toward), 1.0);
    var out: Varying;
    out.position = view_proj * world;
    out.uv = (q.uv + q.uv_a * c.x + q.uv_b * c.y) / vec2<f32>(textureDimensions(sheet));
    out.flags = q.flags;
    out.light = (camera.light * world).xyz;
    return out;
}

@vertex
fn vs_main(q: Quad) -> Varying {
    return vertex(q, camera.view_proj, camera.toward.xyz);
}

// The depth of the scene as the light sees it, its boards turned to the light so that they cast their whole shape.
@vertex
fn vs_shadow(q: Quad) -> Varying {
    return vertex(q, camera.light, camera.sun.xyz);
}

// Flag bits of an instance's first word, as `geometry` names them.
const GLOW: u32 = 4u;
const SHEER: u32 = 8u;

@fragment
fn fs_shadow(v: Varying) {
    // What gives light or is seen through casts no shadow.
    if textureSampleLevel(sheet, pixels, v.uv, 0.0).a < 0.5 || (v.flags.x & (GLOW | SHEER)) != 0u {
        discard;
    }
}

// 1 where the light reaches the point, `SHADE` where something stands before it.
fn lit(light: vec3<f32>) -> f32 {
    let at = vec2<f32>(light.x * 0.5 + 0.5, 0.5 - light.y * 0.5);
    if any(at < vec2<f32>(0.0)) || any(at > vec2<f32>(1.0)) || light.z > 1.0 {
        return 1.0;
    }
    return mix(SHADE, 1.0, textureSampleCompareLevel(shadows, shadowed, at, light.z - 0.0015));
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
    // What is seen through draws every other pixel, so what is behind it shows between them.
    let cell = vec2<u32>(v.position.xy);
    if (v.flags.x & SHEER) != 0u && ((cell.x + cell.y) & 1u) == 0u {
        discard;
    }
    let filtered_rgb = filtered((v.flags.x >> 8u) & 255u, round(texel.rgb * 255.0)) / 255.0;
    // Coloured light adds its colour, times the sprite's own, as the game draws it.
    let light = unpack4x8unorm(v.flags.z).rgb;
    let seen = min(filtered_rgb + filtered_rgb * light, vec3<f32>(1.0));
    // Only what is plainly in sight lies in the light's shadows.
    let shade = select(1.0, lit(v.light), ((v.flags.x >> 8u) & 255u) == 0u && (v.flags.x & GLOW) == 0u);
    // A level seen through open air is hazed toward cyan, more the deeper it is.
    let haze = f32(v.flags.y) / 255.0;
    return vec4<f32>(mix(seen * shade, vec3<f32>(128.0, 255.0, 255.0) / 255.0, haze), 1.0);
}
