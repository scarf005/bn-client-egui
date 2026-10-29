//! Sprites painted with a vehicle part's colour where their mask lets it through, made one after another on a
//! thread of their own.

use super::*;

/// A sprite, the paint and the filter it is made under.
pub(super) type PaintKey = (u32, Option<u32>, Paint, Fx);

/// A sprite painted with a vehicle part's color, made in the background.
pub(super) enum Painted {
    Pending(Receiver<Option<egui::ColorImage>>),
    /// The handle keeps the texture alive.
    Ready {
        _texture: TextureHandle,
        source: Source,
    },
    Failed,
}

impl Atlas {
    /// The sprite painted with `paint` under a filter; none while it is being made or when it cannot be.
    pub(super) fn painted(
        &self,
        ctx: &egui::Context,
        (placed, paint): (Placed, Paint),
        fx: Fx,
    ) -> Option<Source> {
        let sprite = placed.sprite;
        let (n, local) = self.tileset.locate(sprite)?;
        let mut cache = self.painted.borrow_mut();
        let key = (sprite, placed.mask, paint, fx);
        let Some(entry) = cache.get_mut(&key) else {
            let mask = placed.mask.and_then(|m| {
                let (n, local) = self.tileset.locate(m)?;
                Some((n, self.tileset.sheets[n].clone(), local))
            });
            let (done, receive) = channel();
            let _ = self.jobs.send(Job {
                sheet: (n, self.tileset.sheets[n].clone(), local),
                mask,
                paint,
                fx,
                done,
            });
            cache.insert(key, Painted::Pending(receive));
            ctx.request_repaint_after(std::time::Duration::from_millis(30));
            return None;
        };
        if let Painted::Pending(receive) = entry {
            let sheet = &self.tileset.sheets[n];
            *entry = match receive.try_recv() {
                Ok(Some(image)) => {
                    let name = format!("paint{sprite}.{paint:?}.{fx:?}");
                    let texture = ctx.load_texture(name, image, TextureOptions::NEAREST);
                    let source = Source {
                        texture: texture.id(),
                        uv: Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        offset: (sheet.offset.0 as f32, sheet.offset.1 as f32),
                        fx,
                    };
                    Painted::Ready {
                        _texture: texture,
                        source,
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(30));
                    return None;
                }
                _ => Painted::Failed,
            };
        }
        match entry {
            Painted::Ready { source, .. } => Some(*source),
            _ => None,
        }
    }
}

/// A sprite to paint: its sheet as `(sheet number, sheet, index in the sheet)` and that of the sprite masking its
/// paint, the paint and the filter, and where the painted image goes.
pub(super) struct Job {
    sheet: (usize, tileset::Sheet, u32),
    mask: Option<(usize, tileset::Sheet, u32)>,
    paint: Paint,
    fx: Fx,
    done: Sender<Option<egui::ColorImage>>,
}

/// Paints the sprites asked for, one after another, until the atlas that asks is gone.
pub(super) fn painter(jobs: Receiver<Job>) {
    let mut decoded = HashMap::new();
    for job in jobs {
        let _ = job.done.send(paint_sprite(&mut decoded, &job));
    }
}

/// The decoded pixels of sheet `n`, kept for the other sprites painted from it.
fn decoded_sheet(
    decoded: &mut HashMap<usize, Arc<image::RgbaImage>>,
    n: usize,
    sheet: &tileset::Sheet,
) -> Option<Arc<image::RgbaImage>> {
    match decoded.get(&n) {
        Some(image) => Some(image.clone()),
        None => Some(decoded.entry(n).or_insert(Arc::new(load(sheet)?)).clone()),
    }
}

/// One sprite of a sheet painted with `paint`, where its mask lets the paint through, and then filtered, like
/// the game's tinted sprite lookup.
fn paint_sprite(
    decoded: &mut HashMap<usize, Arc<image::RgbaImage>>,
    Job {
        sheet: (n, sheet, local),
        mask,
        paint,
        fx,
        ..
    }: &Job,
) -> Option<egui::ColorImage> {
    let (n, local, fx, paint) = (*n, *local, *fx, *paint);
    let image = decoded_sheet(decoded, n, sheet)?;
    let mask = mask.as_ref().and_then(|&(n, ref sheet, local)| {
        let image = decoded_sheet(decoded, n, sheet)?;
        Some(cut(&image, sheet, local))
    });
    let mut sprite = cut(&image, sheet, local);
    for (x, y, px) in sprite
        .enumerate_pixels_mut()
        .filter(|(_, _, px)| px.0[3] > 0)
    {
        let pixel = |p: &image::Rgba<u8>| [p.0[0], p.0[1], p.0[2]];
        let mask = mask
            .as_ref()
            .and_then(|m| m.get_pixel_checked(x, y))
            .map(pixel);
        let tinted = tint_pixel(pixel(px), paint, mask);
        px.0[..3].copy_from_slice(&fx.pixel(tinted));
    }
    let (w, h) = sheet.sprite;
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [w as usize, h as usize],
        sprite.as_raw(),
    ))
}

/// Sprite `local` of `sheet`, cut from the sheet's decoded pixels.
fn cut(image: &image::RgbaImage, sheet: &tileset::Sheet, local: u32) -> image::RgbaImage {
    let (w, h) = sheet.sprite;
    let (col, row) = (local % sheet.columns, local / sheet.columns);
    image::imageops::crop_imm(image, col * w, row * h, w, h).to_image()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sheet of two 2x1 sprites in `dir`: a sprite of two grey pixels and its mask, whose first pixel lets
    /// the paint through and whose second does not.
    fn sheet(dir: &Path) -> tileset::Sheet {
        let file = dir.join("masked.png");
        let mut image = image::RgbaImage::new(4, 1);
        let pixels = [[128, 128, 128], [200, 40, 40], [255, 255, 127], [0, 0, 0]];
        for (x, rgb) in pixels.into_iter().enumerate() {
            image.put_pixel(x as u32, 0, image::Rgba([rgb[0], rgb[1], rgb[2], 255]));
        }
        image.save(&file).unwrap();
        tileset::Sheet {
            file,
            sprite: (2, 1),
            offset: (0, 0),
            pixelscale: 1.0,
            key: None,
            columns: 2,
            first: 0,
            count: 2,
        }
    }

    #[test]
    fn a_sprite_is_painted_only_where_its_mask_lets_the_paint_through() {
        let dir = std::env::temp_dir().join(format!("bn-client-mask-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sheet = sheet(&dir);
        let mut decoded = HashMap::new();
        let paint = Paint::of([247, 251, 245]);
        let mut painted = |mask: Option<(usize, tileset::Sheet, u32)>| {
            let job = Job {
                sheet: (0, sheet.clone(), 0),
                mask,
                paint,
                fx: Fx::Plain,
                done: channel().0,
            };
            paint_sprite(&mut decoded, &job).unwrap().pixels
        };
        let color = |p: [u8; 3]| Color32::from_rgb(p[0], p[1], p[2]);
        let masked = painted(Some((0, sheet.clone(), 1)));
        assert_eq!(
            masked[0],
            color(tint_pixel([128; 3], paint, Some([255, 255, 127])))
        );
        assert_ne!(masked[0], color([128; 3]));
        assert_eq!(masked[1], color([200, 40, 40]));
        // Without a mask the whole sprite is painted.
        let whole = painted(None);
        assert_eq!(whole[1], color(tint_pixel([200, 40, 40], paint, None)));
        assert_ne!(whole[1], color([200, 40, 40]));
    }
}
