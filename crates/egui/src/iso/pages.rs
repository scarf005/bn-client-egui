//! The tileset's sheets as the 3D scene's textures: decoded in the background, a strip of rows per page.

use super::geometry::{PageId, SpriteSource};
use crate::display::{STRIP, load};
use crate::tileset::Sheet;
use crate::tint::{Paint, tint_pixel};
use eframe::egui;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// The sheet number of the pages of painted sprites, after those of the tileset's own sheets.
const PAINTED_SHEET: u16 = u16::MAX;

/// A sprite, the sprite masking its paint, and the paint: pixels of the pages and a color.
type Painted = (PageId, [u32; 2], Option<(PageId, [u32; 2])>, Paint);

/// A sprite by its page, its corner there and its size.
type Cut = (PageId, [u32; 2], [u32; 2]);

/// The decoded pages, filling as the sheets finish.
pub struct Pages {
    images: Mutex<HashMap<PageId, Arc<image::RgbaImage>>>,
    /// The pages of sprites painted with a color, by what they were painted from.
    painted: Mutex<HashMap<Painted, PageId>>,
    /// The first and the last column and row of each sprite's opaque pixels, none for a clear sprite.
    opaque: Mutex<HashMap<Cut, Option<[u32; 4]>>>,
    /// Sheets not decoded yet.
    waiting: AtomicUsize,
}

impl Pages {
    /// Starts decoding `sheets`; each finished sheet wakes the UI.
    pub fn start(sheets: Vec<Sheet>, ctx: egui::Context) -> Arc<Self> {
        let pages = Arc::new(Self {
            images: Mutex::default(),
            painted: Mutex::default(),
            opaque: Mutex::default(),
            waiting: AtomicUsize::new(sheets.len()),
        });
        let filling = pages.clone();
        std::thread::spawn(move || {
            for (n, sheet) in sheets.iter().enumerate() {
                for (strip, image) in strips(sheet).into_iter().enumerate() {
                    let id = PageId {
                        sheet: n as u16,
                        strip: strip as u16,
                    };
                    if let Ok(mut images) = filling.images.lock() {
                        images.insert(id, Arc::new(image));
                    }
                }
                filling.waiting.fetch_sub(1, Ordering::Release);
                ctx.request_repaint();
            }
        });
        pages
    }

    pub fn get(&self, id: PageId) -> Option<Arc<image::RgbaImage>> {
        self.images.lock().ok()?.get(&id).cloned()
    }

    /// `sprite` painted with `paint` where its `mask` lets the paint through, as a page of its own, like the flat
    /// map's tinted sprites; none while the pages it is cut from are being decoded.
    pub fn painted(
        &self,
        sprite: &SpriteSource,
        mask: Option<&SpriteSource>,
        paint: Paint,
    ) -> Option<SpriteSource> {
        let corner = |s: &SpriteSource| [s.at.x as u32, s.at.y as u32];
        let key = (
            sprite.page,
            corner(sprite),
            mask.map(|m| (m.page, corner(m))),
            paint,
        );
        let mut painted = self.painted.lock().ok()?;
        let page = match painted.get(&key) {
            Some(page) => *page,
            None => {
                let cut = |s: &SpriteSource| {
                    let (at, size) = (corner(s), [s.src.x as u32, s.src.y as u32]);
                    let image = self.get(s.page)?;
                    Some(
                        image::imageops::crop_imm(&*image, at[0], at[1], size[0], size[1])
                            .to_image(),
                    )
                };
                let mut image = cut(sprite)?;
                let mask = match mask {
                    Some(m) => Some(cut(m)?),
                    None => None,
                };
                for (x, y, px) in image
                    .enumerate_pixels_mut()
                    .filter(|(_, _, px)| px.0[3] > 0)
                {
                    let rgb = |p: &image::Rgba<u8>| [p.0[0], p.0[1], p.0[2]];
                    let covered = mask
                        .as_ref()
                        .and_then(|m| m.get_pixel_checked(x, y))
                        .map(rgb);
                    let tinted = tint_pixel(rgb(px), paint, covered);
                    px.0[..3].copy_from_slice(&tinted);
                }
                let page = PageId {
                    sheet: PAINTED_SHEET,
                    strip: u16::try_from(painted.len()).ok()?,
                };
                self.images.lock().ok()?.insert(page, Arc::new(image));
                painted.insert(key, page);
                page
            }
        };
        Some(SpriteSource {
            page,
            at: glam::Vec2::ZERO,
            ..*sprite
        })
    }

    /// `sprite` without the transparent rows above its first opaque one, which draw nothing; as it is while its page
    /// is being decoded. An isometric sprite is as tall as its tile, whatever it draws: a floor's diamond lies at
    /// the bottom of it, and only what is left tells ground from a wall.
    pub fn trimmed(&self, sprite: SpriteSource) -> SpriteSource {
        let Some(image) = self.get(sprite.page) else {
            return sprite;
        };
        let at = [sprite.at.x as u32, sprite.at.y as u32];
        let size = [sprite.src.x as u32, sprite.src.y as u32];
        let opaque = |row: u32| {
            (0..size[0]).any(|col| {
                let pixel = image.get_pixel_checked(at[0] + col, at[1] + row);
                pixel.is_some_and(|px| px.0[3] >= 128)
            })
        };
        // The fragment shader discards what is less opaque than half.
        let blank = (0..size[1]).take_while(|row| !opaque(*row)).count() as f32;
        match blank < sprite.src.y {
            true => SpriteSource {
                at: sprite.at + glam::Vec2::new(0.0, blank),
                src: sprite.src - glam::Vec2::new(0.0, blank),
                offset: sprite.offset + glam::Vec2::new(0.0, blank * sprite.scale),
                ..sprite
            },
            false => sprite,
        }
    }

    /// `sprite` cut to the rectangle of its opaque pixels, in its place on the screen; as it is while its page is
    /// being decoded, or when it is clear.
    pub fn cropped(&self, sprite: SpriteSource) -> SpriteSource {
        let at = [sprite.at.x as u32, sprite.at.y as u32];
        let size = [sprite.src.x as u32, sprite.src.y as u32];
        let key = (sprite.page, at, size);
        let known = self.opaque.lock().ok().and_then(|o| o.get(&key).copied());
        let bounds = match known {
            Some(bounds) => bounds,
            None => {
                let Some(image) = self.get(sprite.page) else {
                    return sprite;
                };
                // The fragment shader discards what is less opaque than half.
                let bounds = (0..size[1])
                    .flat_map(|y| (0..size[0]).map(move |x| (x, y)))
                    .filter(|(x, y)| {
                        let pixel = image.get_pixel_checked(at[0] + x, at[1] + y);
                        pixel.is_some_and(|px| px.0[3] >= 128)
                    })
                    .fold(None, |b: Option<[u32; 4]>, (x, y)| {
                        Some(b.map_or([x, y, x, y], |[x0, y0, x1, y1]| {
                            [x0.min(x), y0.min(y), x1.max(x), y1.max(y)]
                        }))
                    });
                if let Ok(mut opaque) = self.opaque.lock() {
                    opaque.insert(key, bounds);
                }
                bounds
            }
        };
        let Some([x0, y0, x1, y1]) = bounds else {
            return sprite;
        };
        let corner = glam::Vec2::new(x0 as f32, y0 as f32);
        SpriteSource {
            at: sprite.at + corner,
            src: glam::Vec2::new((x1 + 1 - x0) as f32, (y1 + 1 - y0) as f32),
            offset: sprite.offset + corner * sprite.scale,
            ..sprite
        }
    }

    /// Some sheet is still being decoded.
    pub fn loading(&self) -> bool {
        self.waiting.load(Ordering::Acquire) > 0
    }
}

/// A sheet with its transparency key made transparent, cut into strips of whole sprite rows.
fn strips(sheet: &Sheet) -> Vec<image::RgbaImage> {
    let Some(image) = load(sheet) else {
        return vec![];
    };
    let tall = sheet.sprite.1.max(1);
    let rows = (STRIP / tall).max(1) * tall;
    let usable = image.height() / tall * tall;
    (0..usable)
        .step_by(rows as usize)
        .map(|y| {
            image::imageops::crop_imm(&image, 0, y, image.width(), rows.min(usable - y)).to_image()
        })
        .collect()
}
