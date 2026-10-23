//! The sky the first-person view looks into, as the engine describes it: its colour from the zenith down to the
//! horizon, the sun or the moon where they have crossed to, the stars at night and the clouds of the weather.

use bn_client_core::protocol::{MoonPhase, Sky};
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};

/// How far the arc of the sun and the moon leans to the south of the zenith.
const LEAN: f32 = 35.0 * std::f32::consts::PI / 180.0;
/// The weather's dimming at which the sky is wholly overcast.
const OVERCAST: f32 = 40.0;

/// What the sky shader is given.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct SkyUniform {
    /// From the clip space of the view back to the world, to find the way each pixel looks.
    inverse: [[f32; 4]; 4],
    zenith: [f32; 4],
    horizon: [f32; 4],
    /// The way to the sun, and 1 in `w` while it is up.
    sun: [f32; 4],
    /// The way to the moon, and 1 in `w` while it is up.
    moon: [f32; 4],
    /// The moon's phase: the cosine of its phase angle, 1 when waxing else -1, how bright the stars are, and how
    /// much of the sky the clouds cover.
    phase: [f32; 4],
}

impl SkyUniform {
    /// The sky of `sky` seen through the camera of `view_proj`.
    pub fn new(sky: &Sky, view_proj: Mat4) -> Self {
        let (zenith, horizon) = colors(sky);
        let up = |arc: Option<u8>| arc.map_or([0.0; 4], |arc| way(arc).extend(1.0).to_array());
        let (cos, waxing) = phase(sky.moon_phase);
        Self {
            inverse: view_proj.inverse().to_cols_array_2d(),
            zenith: zenith.extend(1.0).to_array(),
            horizon: horizon.extend(1.0).to_array(),
            sun: up(sky.sun),
            moon: up(sky.moon),
            phase: [cos, if waxing { 1.0 } else { -1.0 }, stars(sky), cover(sky)],
        }
    }
}

/// The way from the eye to the sun or the moon when it has crossed `arc` percent of its way from the east to the
/// west, in world axes (x east, y south, z up).
pub fn way(arc: u8) -> Vec3 {
    let angle = f32::from(arc.min(100)) / 100.0 * std::f32::consts::PI;
    let (high, along) = angle.sin_cos();
    Vec3::new(along, high * LEAN.sin(), high * LEAN.cos()).normalize()
}

/// How much of the sky the clouds cover, 0 to 1.
pub fn cover(sky: &Sky) -> f32 {
    (sky.dimming as f32 / OVERCAST).clamp(0.0, 1.0)
}

/// How bright the stars shine, 0 to 1: only in the dark, and not through clouds.
pub fn stars(sky: &Sky) -> f32 {
    let dark = 1.0 - (f32::from(sky.daylight.min(100)) / 30.0).min(1.0);
    dark * (1.0 - cover(sky))
}

/// The cosine of the moon's phase angle (1 new, -1 full) and whether it waxes, its lit side then toward the west.
pub fn phase(moon: MoonPhase) -> (f32, bool) {
    let step = match moon {
        MoonPhase::New => 0,
        MoonPhase::WaxingCrescent => 1,
        MoonPhase::HalfMoonWaxing => 2,
        MoonPhase::WaxingGibbous => 3,
        MoonPhase::Full => 4,
        MoonPhase::WaningGibbous => 5,
        MoonPhase::HalfMoonWaning => 6,
        MoonPhase::WaningCrescent => 7,
    };
    ((step as f32 * std::f32::consts::FRAC_PI_4).cos(), step < 4)
}

/// The colour of the sky overhead and at the horizon, sRGB from 0 to 1: blue by day and dark by night as the
/// daylight says, greyed by as much light as the weather takes away, and red at the horizon while the sun is low.
pub fn colors(sky: &Sky) -> (Vec3, Vec3) {
    let cloud = 0.85 * cover(sky);
    let light = f32::from(sky.daylight.min(100)) / 100.0;
    let grey = Vec3::new(0.55, 0.57, 0.59);
    let day = (
        Vec3::new(0.25, 0.47, 0.82).lerp(grey, cloud),
        Vec3::new(0.66, 0.78, 0.9).lerp(grey * 1.1, cloud),
    );
    let night = (Vec3::new(0.01, 0.02, 0.06), Vec3::new(0.04, 0.06, 0.12));
    // Low sun: the last part of the arc at either end.
    let low = sky.sun.map_or(0.0, |arc| {
        1.0 - (f32::from(arc.min(100).min(100 - arc.min(100))) / 15.0).min(1.0)
    });
    let dusk = Vec3::new(0.93, 0.5, 0.28).lerp(grey, cloud);
    (
        night.0.lerp(day.0, light),
        night.1.lerp(day.1, light).lerp(dusk, low * light.max(0.3)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sky(daylight: u8, dimming: u32) -> Sky {
        Sky {
            daylight,
            dimming,
            ..Sky::default()
        }
    }

    #[test]
    fn the_sun_rises_in_the_east_stands_high_in_the_south_at_noon_and_sets_in_the_west() {
        assert!((way(0) - Vec3::X).length() < 1e-5, "{:?}", way(0));
        assert!((way(100) + Vec3::X).length() < 1e-5, "{:?}", way(100));
        let noon = way(50);
        assert!(
            noon.x.abs() < 1e-5 && noon.y > 0.0 && noon.z > 0.8,
            "{noon:?}"
        );
        assert!(way(25).z < noon.z && way(25).x > 0.0);
    }

    #[test]
    fn the_sky_is_blue_by_day_dark_by_night_grey_under_clouds_and_red_low_by_a_low_sun() {
        let brightness = |c: Vec3| c.x + c.y + c.z;
        let (day, night, cloudy) = (
            colors(&sky(100, 0)),
            colors(&sky(0, 0)),
            colors(&sky(100, 40)),
        );
        assert!(day.0.z > day.0.x + 0.3, "{day:?}");
        assert!(brightness(night.0) < brightness(day.0) / 4.0, "{night:?}");
        assert!(cloudy.0.z - cloudy.0.x < 0.15, "{cloudy:?}");
        let half = colors(&sky(50, 0)).0;
        assert!(brightness(half) < brightness(day.0) && brightness(half) > brightness(night.0));
        // The horizon reddens as the sun goes down, not overhead.
        let setting = colors(&Sky {
            sun: Some(98),
            ..sky(60, 0)
        });
        let high = colors(&Sky {
            sun: Some(50),
            ..sky(60, 0)
        });
        assert!(
            setting.1.x - setting.1.z > high.1.x - high.1.z + 0.3,
            "{setting:?} {high:?}"
        );
        assert_eq!(setting.0, high.0);
    }

    #[test]
    fn stars_shine_only_in_a_dark_clear_sky() {
        assert_eq!(stars(&sky(100, 0)), 0.0);
        assert_eq!(stars(&sky(0, 0)), 1.0);
        assert_eq!(stars(&sky(0, 40)), 0.0);
        assert!(stars(&sky(0, 20)) > 0.0 && stars(&sky(0, 20)) < 1.0);
    }

    #[test]
    fn the_moon_is_dark_when_new_whole_when_full_and_lit_from_the_side_it_waxes_to() {
        assert_eq!(phase(MoonPhase::New), (1.0, true));
        assert!((phase(MoonPhase::Full).0 + 1.0).abs() < 1e-5);
        assert!(
            phase(MoonPhase::HalfMoonWaxing).0.abs() < 1e-5 && phase(MoonPhase::HalfMoonWaxing).1
        );
        assert!(!phase(MoonPhase::HalfMoonWaning).1);
    }

    #[test]
    fn only_a_risen_sun_or_moon_is_drawn() {
        let night = SkyUniform::new(
            &Sky {
                moon: Some(50),
                moon_phase: MoonPhase::Full,
                ..sky(0, 0)
            },
            Mat4::IDENTITY,
        );
        assert_eq!(night.sun[3], 0.0);
        assert_eq!(night.moon[3], 1.0);
        assert_eq!(night.phase[2], 1.0);
    }
}
