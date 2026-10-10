//! The game's tint blend (`tint_blend_mode::tint`): a sprite takes the hue of the paint and part of its
//! saturation and value, as `apply_surf_blend_effect` does for vehicle parts, with the same integer maths.

const E: u32 = (1 << 16) - 1;

struct Hsv {
    h: u32,
    s: u16,
    v: u8,
}

fn rgb2hsv([r, g, b]: [u8; 3]) -> Hsv {
    let (min, max) = (r.min(g).min(b), r.max(g).max(b));
    let med = r.max(g).min(r.max(b)).min(g.max(b));
    if max == min || max == 0 {
        return Hsv { h: 0, s: 0, v: max };
    }
    let d = u32::from(max - min);
    let s = (((d << 16) - 1) / u32::from(max)) as u16;
    let sector = if max == r && min == b {
        0
    } else if max == g && min == b {
        1
    } else if max == g && min == r {
        2
    } else if max == b && min == r {
        3
    } else if max == b && min == g {
        4
    } else {
        5
    };
    let mut f = ((u32::from(med - min) << 16) / d) as i32 + 1;
    if sector & 1 == 1 {
        f = E as i32 - f;
    }
    Hsv {
        h: (E as i32 * sector + f) as u32,
        s,
        v: max,
    }
}

fn hsv2rgb(Hsv { h, s, v }: Hsv) -> [u8; 3] {
    if s == 0 || v == 0 {
        return [v; 3];
    }
    let sector = (h / E).min(5);
    let mut f = h.wrapping_sub(E * sector);
    if f == 0 {
        f = 1;
    }
    if !sector.is_multiple_of(2) {
        f = E.wrapping_sub(f);
    }
    let d = ((u32::from(s) * u32::from(v)) >> 16) + 1;
    let m = (u32::from(v).wrapping_sub(d)) as u8;
    let c = ((f.wrapping_mul(d) >> 16).wrapping_add(u32::from(m))) as u8;
    match sector {
        0 => [v, c, m],
        1 => [c, v, m],
        2 => [m, v, c],
        3 => [m, c, v],
        4 => [c, m, v],
        _ => [v, m, c],
    }
}

fn overlay(base: u8, blend: u8) -> u8 {
    let (base, blend) = (i32::from(base), i32::from(blend));
    let mixed = if base > 127 {
        255 - (255 - blend).max(1) * ((255 - base) * 255 / 127) / 255
    } else {
        blend * (base * 255 / 127) / 255
    };
    mixed.clamp(0, 255) as u8
}

/// One pixel of a sprite painted with `target`.
pub fn tint_pixel(base: [u8; 3], target: [u8; 3]) -> [u8; 3] {
    let (b, t) = (rgb2hsv(base), rgb2hsv(target));
    let lerp = |a: i32, b: i32| ((b * 127 + a * (255 - 127)) / 255) as u16;
    let tinted = Hsv {
        h: t.h,
        s: lerp(i32::from(b.s.min(t.s)), i32::from(t.s)),
        v: lerp(i32::from(b.v), i32::from(overlay(b.v, t.v))) as u8,
    };
    // The game converts the result through HSV once more before it is stored.
    hsv2rgb(rgb2hsv(hsv2rgb(tinted)))
}
