//! The game's tint blends (`apply_surf_blend_effect`): by default a sprite takes the hue of the paint and part of its
//! saturation and value, as for vehicle parts; a tileset can ask for other blends. The same integer maths.

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

/// How a paint is blended into a sprite (`tint_blend_mode`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Blend {
    #[default]
    Tint,
    Additive,
    Subtract,
    Multiply,
    Normal,
    Divide,
    Screen,
    Softlight,
    Hardlight,
    Overlay,
}

impl Blend {
    /// The blend a tileset names; an unknown or empty name is the default tint.
    pub fn named(name: &str) -> Self {
        match name {
            "additive" => Self::Additive,
            "subtract" => Self::Subtract,
            "multiply" => Self::Multiply,
            "normal" => Self::Normal,
            "divide" => Self::Divide,
            "screen" => Self::Screen,
            "softlight" => Self::Softlight,
            "hardlight" => Self::Hardlight,
            "overlay" => Self::Overlay,
            _ => Self::Tint,
        }
    }
}

/// A color and how it is blended into a sprite; the adjustments are in thousandths, 1000 leaving things alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Paint {
    pub color: [u8; 3],
    pub blend: Blend,
    pub contrast: u32,
    pub saturation: u32,
    pub brightness: u32,
}

impl Paint {
    /// The paint of a vehicle part or any plain color: the default tint blend.
    pub fn of(color: [u8; 3]) -> Self {
        Self {
            color,
            blend: Blend::Tint,
            contrast: 1000,
            saturation: 1000,
            brightness: 1000,
        }
    }
}

fn blend_channel(blend: Blend, base: u8, target: u8) -> u8 {
    let (b, t) = (i32::from(base), i32::from(target));
    let clamp = |v: i32| v.clamp(0, 255) as u8;
    match blend {
        Blend::Additive => clamp(b + t),
        Blend::Subtract => clamp(b - (255 - t)),
        Blend::Multiply => (b * t / 256) as u8,
        // The paint's alpha is full, so a normal blend is the paint.
        Blend::Normal => target,
        Blend::Divide => (b / t.max(1)) as u8,
        Blend::Screen => clamp(255 - (255 - b) * (255 - t) / 128),
        Blend::Softlight => clamp(((255 - 2 * t) * b * b / 256 + 2 * t * b) / 256),
        Blend::Hardlight => {
            if t > 127 {
                clamp(255 - (255 - t) * ((255 - b).max(1) * 255 / 127) / 255)
            } else {
                clamp(t * (b * 255 / 127) / 255)
            }
        }
        Blend::Overlay => overlay(base, target),
        Blend::Tint => unreachable!("the tint blend works on hue, saturation and value"),
    }
}

/// One pixel of a sprite painted with `paint`. A tint mask pixel sets how much of the paint each channel takes
/// (red: how far the colour moves, green: saturation, blue: value); without a mask the sprite is painted whole,
/// with the saturation and value halfway.
pub fn tint_pixel(base: [u8; 3], paint: Paint, mask: Option<[u8; 3]>) -> [u8; 3] {
    let [red, green, blue] = mask.unwrap_or([255, 127, 127]);
    let lerp =
        |a: i32, b: i32, by: u8| ((b * i32::from(by) + a * (255 - i32::from(by))) / 255) as u16;
    let blended = if paint.blend == Blend::Tint {
        let (b, t) = (rgb2hsv(base), rgb2hsv(paint.color));
        hsv2rgb(Hsv {
            h: t.h,
            s: lerp(i32::from(b.s.min(t.s)), i32::from(t.s), green),
            v: lerp(i32::from(b.v), i32::from(overlay(b.v, t.v)), blue) as u8,
        })
    } else {
        [0, 1, 2].map(|i| blend_channel(paint.blend, base[i], paint.color[i]))
    };
    let moved = [0, 1, 2].map(|i| lerp(i32::from(base[i]), i32::from(blended[i]), red) as u8);
    // The game converts the result through HSV once more, adjusted, before it is stored.
    let Hsv { h, mut s, mut v } = rgb2hsv(moved);
    let thousandths = |value: u32| value as f32 / 1000.0;
    if paint.contrast != 1000 {
        let adjusted = (f32::from(v) - 128.0) * thousandths(paint.contrast) + 128.0;
        v = adjusted.clamp(0.0, 255.0) as u8;
    }
    if paint.saturation != 1000 {
        s = (f32::from(s) * thousandths(paint.saturation)).clamp(0.0, 65535.0) as u16;
    }
    if paint.brightness != 1000 {
        v = (f32::from(v) * thousandths(paint.brightness)).clamp(0.0, 255.0) as u8;
    }
    hsv2rgb(Hsv { h, s, v })
}
