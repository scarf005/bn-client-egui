// The sky behind the first-person view: one triangle over the viewport, each pixel coloured by the way it looks.
// World axes: x east, y south, z up.

struct Sky {
    inverse: mat4x4<f32>,
    zenith: vec4<f32>,
    horizon: vec4<f32>,
    sun: vec4<f32>,
    moon: vec4<f32>,
    // The cosine of the moon's phase angle, 1 when it waxes else -1, the stars' brightness, the clouds' cover.
    phase: vec4<f32>,
}

@group(0) @binding(0) var<uniform> sky: Sky;

// The angular radius of the sun's and the moon's discs, in radians.
const DISC: f32 = 0.035;

struct Varying {
    @builtin(position) position: vec4<f32>,
    @location(0) ndc: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> Varying {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u)) * 2.0 - 1.0;
    var out: Varying;
    // At the far plane, behind everything the scene draws.
    out.position = vec4<f32>(p, 1.0, 1.0);
    out.ndc = p;
    return out;
}

fn hash(p: vec3<f32>) -> f32 {
    return fract(sin(dot(p, vec3<f32>(127.1, 311.7, 74.7))) * 43758.5453);
}

// Smooth value noise over the plane.
fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash(vec3<f32>(i, 0.0));
    let b = hash(vec3<f32>(i + vec2<f32>(1.0, 0.0), 0.0));
    let c = hash(vec3<f32>(i + vec2<f32>(0.0, 1.0), 0.0));
    let d = hash(vec3<f32>(i + vec2<f32>(1.0, 1.0), 0.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn clouds(p: vec2<f32>) -> f32 {
    return 0.5 * noise(p) + 0.25 * noise(p * 2.1) + 0.125 * noise(p * 4.3) + 0.0625 * noise(p * 8.7);
}

// The moon's disc at `d` (the way to it) seen along `look`: its colour and how much of it covers the pixel. Its lit
// side faces the sun under the horizon: to the west while it waxes, to the east while it wanes.
fn moon_disc(look: vec3<f32>, d: vec3<f32>) -> vec4<f32> {
    let sun = vec3<f32>(-sky.phase.y, 0.0, -0.5);
    let toward = normalize(sun - d * dot(sun, d));
    let across = cross(d, toward);
    let uv = vec2<f32>(dot(look, toward), dot(look, across)) / DISC;
    let r = length(uv);
    if (r > 1.0 || dot(look, d) < 0.0) {
        return vec4<f32>(0.0);
    }
    let edge = sky.phase.x * sqrt(max(1.0 - uv.y * uv.y, 0.0));
    let lit = mix(0.08, 0.92, smoothstep(edge - 0.06, edge + 0.06, uv.x));
    return vec4<f32>(vec3<f32>(lit, lit, lit * 0.95), 1.0 - smoothstep(0.9, 1.0, r));
}

@fragment
fn fs_main(v: Varying) -> @location(0) vec4<f32> {
    let near = sky.inverse * vec4<f32>(v.ndc, 0.0, 1.0);
    let far = sky.inverse * vec4<f32>(v.ndc, 1.0, 1.0);
    let look = normalize(far.xyz / far.w - near.xyz / near.w);
    let up = max(look.z, 0.0);
    var colour = mix(sky.horizon.rgb, sky.zenith.rgb, pow(up, 0.6));
    // The sun's glow and disc.
    if (sky.sun.w > 0.5) {
        let c = dot(look, sky.sun.xyz);
        colour += vec3<f32>(1.0, 0.85, 0.6) * pow(max(c, 0.0), 64.0) * 0.5;
        colour = mix(colour, vec3<f32>(1.0, 0.97, 0.88), smoothstep(cos(DISC), cos(DISC * 0.9), c));
    }
    // Stars, fixed to the sky.
    let star_cell = floor(look * 300.0);
    let star = step(0.997, hash(star_cell)) * sky.phase.z * smoothstep(0.0, 0.2, look.z);
    colour += vec3<f32>(star * (0.5 + 0.5 * hash(star_cell + 7.0)));
    if (sky.moon.w > 0.5) {
        let disc = moon_disc(look, sky.moon.xyz);
        colour = mix(colour, disc.rgb, disc.a);
    }
    // Clouds on a plane overhead; the more cover, the more of them.
    if (look.z > 0.01) {
        let at = look.xy / look.z * 2.0;
        let cover = sky.phase.w;
        let density = smoothstep(1.0 - cover * 0.9 - 0.15, 1.0 - cover * 0.9 + 0.2, clouds(at));
        let shade = mix(sky.horizon.rgb, vec3<f32>(dot(sky.zenith.rgb, vec3<f32>(0.4))), 0.5) * 1.15;
        colour = mix(colour, shade, density * smoothstep(0.01, 0.15, look.z) * (0.35 + 0.6 * cover));
    }
    // Below the horizon, where no ground is drawn, the horizon's colour fades to black within a few degrees, so a
    // square left undrawn under the eye stays dark as it is without a sky.
    if (look.z < 0.0) {
        colour = sky.horizon.rgb * 0.5 * (1.0 - smoothstep(0.0, 0.08, -look.z));
    }
    return vec4<f32>(clamp(colour, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
