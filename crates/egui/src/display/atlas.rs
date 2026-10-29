//! A tileset's sheets as textures: decoded in the background under each filter the map asks for, and uploaded
//! a sheet per frame.

use super::*;

/// Sprite rows are uploaded in strips at most this tall, within every GPU's texture limit.
pub(crate) const STRIP: u32 = 4096;
/// Remembered squares draw darkened, squares in low light slightly less.
const MEMORY: Color32 = Color32::from_gray(100);
const SHADOW: Color32 = Color32::from_gray(170);

/// The game's filters for what is not plainly in sight: grayscale for low light, sepia for memory, and
/// the cyan haze laid over levels seen through open air.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Fx {
    Plain,
    Shadow,
    Memory,
    ZOverlay,
}

impl Fx {
    const ALL: [Fx; 4] = [Fx::Plain, Fx::Shadow, Fx::Memory, Fx::ZOverlay];

    pub(super) fn index(self) -> usize {
        self as usize
    }

    /// The colour a plain sprite is multiplied by until the filtered sheets have arrived.
    pub(crate) fn stand_in(self) -> Color32 {
        match self {
            Fx::Plain => Color32::WHITE,
            Fx::Shadow => SHADOW,
            Fx::Memory => MEMORY,
            Fx::ZOverlay => Color32::TRANSPARENT,
        }
    }

    /// The game's `color_pixel_*` function of this filter on one pixel.
    pub fn pixel(self, [r, g, b]: [u8; 3]) -> [u8; 3] {
        let average = ((85 * (u32::from(r) + u32::from(g) + u32::from(b))) >> 8) as u8;
        match self {
            Fx::Plain => [r, g, b],
            Fx::ZOverlay => [128, 255, 255],
            _ if r == 0 && g == 0 && b == 0 => [r, g, b],
            Fx::Shadow => [((u32::from(average) * 5) >> 3).max(1) as u8; 3],
            Fx::Memory => {
                let (dark, light) = ([39u32, 23, 19], [241u32, 220, 163]);
                let level = f32::from(average) / 255.0;
                let shade = ((level.powf(1.6) * 150.0).round().min(100.0)) as u32;
                [0, 1, 2].map(|i| ((dark[i] * (100 - shade) + light[i] * shade) / 100) as u8)
            }
        }
    }
}

/// A decoded sprite sheet, cut into strips ready to become textures.
struct Strips {
    sheet: usize,
    strips: Vec<egui::ColorImage>,
}

/// Wakes the UI when a background thread has finished a sheet; known once the first frame has run.
type Wake = Arc<OnceLock<egui::Context>>;

/// The sheets of a tileset under one filter, decoded in the background and uploaded as they arrive.
pub(super) struct Sheets {
    incoming: Receiver<Strips>,
    /// Decoded sheets waiting for their turn: one is uploaded per frame, so no frame stalls on a copy.
    decoded: VecDeque<Strips>,
    textures: HashMap<usize, Vec<TextureHandle>>,
    pub(super) loading: usize,
}

impl Sheets {
    fn start(sheets: Vec<tileset::Sheet>, fx: Fx, wake: Wake) -> Self {
        let (send, incoming) = channel();
        let loading = sheets.len();
        std::thread::spawn(move || {
            for (n, sheet) in sheets.iter().enumerate() {
                let strips = decode(sheet, fx).unwrap_or_default();
                if send.send(Strips { sheet: n, strips }).is_err() {
                    return;
                }
                wake.get().inspect(|ctx| ctx.request_repaint());
            }
        });
        Self {
            incoming,
            decoded: VecDeque::new(),
            textures: HashMap::new(),
            loading,
        }
    }

    /// Uploads the next decoded sheet; true while more are waiting for the next frame.
    fn upload(&mut self, ctx: &egui::Context, fx: Fx) -> bool {
        self.decoded.extend(self.incoming.try_iter());
        if let Some(done) = self.decoded.pop_front() {
            self.loading -= 1;
            let textures = done
                .strips
                .into_iter()
                .enumerate()
                .map(|(i, image)| {
                    let name = format!("sheet{}.{i}.{fx:?}", done.sheet);
                    ctx.load_texture(name, image, TextureOptions::NEAREST)
                })
                .collect();
            self.textures.insert(done.sheet, textures);
        }
        !self.decoded.is_empty()
    }
}

/// Where a sprite is: its texture, its rectangle in it (0..1), the sheet's offset and the filter the
/// texture already has.
#[derive(Clone, Copy)]
pub(super) struct Source {
    pub(super) texture: TextureId,
    pub(super) uv: Rect,
    pub(super) offset: (f32, f32),
    pub(super) fx: Fx,
}

/// A tileset, with its plain sheets from the start and the filtered ones once a square needs them.
pub(super) struct Atlas {
    pub(super) tileset: Tileset,
    pub(super) sheets: [Option<Sheets>; 4],
    /// The sprites to paint, for the one thread that paints them.
    pub(super) jobs: Sender<Job>,
    pub(super) painted: RefCell<HashMap<PaintKey, Painted>>,
    wake: Wake,
}

impl Atlas {
    pub(super) fn start(tileset: Tileset) -> Self {
        let wake = Wake::default();
        let plain = Sheets::start(tileset.sheets.clone(), Fx::Plain, wake.clone());
        let (jobs, queue) = channel();
        std::thread::spawn(move || painter(queue));
        Self {
            tileset,
            sheets: [Some(plain), None, None, None],
            jobs,
            painted: RefCell::default(),
            wake,
        }
    }

    /// Starts decoding the sheets of a filter the first time something is drawn with it.
    pub(super) fn request(&mut self, fx: Fx) {
        if self.sheets[fx.index()].is_none() {
            self.sheets[fx.index()] = Some(Sheets::start(
                self.tileset.sheets.clone(),
                fx,
                self.wake.clone(),
            ));
        }
    }

    pub(super) fn upload(&mut self, ctx: &egui::Context) -> bool {
        // Set before looking for sheets, so one finished meanwhile either is seen now or wakes the UI.
        let _ = self.wake.set(ctx.clone());
        let mut more = false;
        for fx in Fx::ALL {
            if let Some(sheets) = &mut self.sheets[fx.index()] {
                more |= sheets.upload(ctx, fx);
            }
        }
        more
    }

    /// The sprite under a filter; the plain one when the filtered sheet has not arrived.
    pub(super) fn source(&self, sprite: u32, fx: Fx) -> Option<Source> {
        let (n, local) = self.tileset.locate(sprite)?;
        let sheet = &self.tileset.sheets[n];
        let rows = (STRIP / sheet.sprite.1).max(1);
        let (col, row) = (local % sheet.columns, local / sheet.columns);
        let strips = |fx: Fx| self.sheets[fx.index()].as_ref()?.textures.get(&n);
        let (strips, fx) = match strips(fx) {
            Some(found) => (found, fx),
            None => (strips(Fx::Plain)?, Fx::Plain),
        };
        let texture = strips.get((row / rows) as usize)?;
        let size = texture.size_vec2();
        let min = egui::vec2(
            (col * sheet.sprite.0) as f32,
            ((row % rows) * sheet.sprite.1) as f32,
        );
        Some(Source {
            texture: texture.id(),
            uv: Rect::from_min_size(
                (min / size).to_pos2(),
                egui::vec2(sheet.sprite.0 as f32, sheet.sprite.1 as f32) / size,
            ),
            offset: (sheet.offset.0 as f32, sheet.offset.1 as f32),
            fx,
        })
    }
}

/// A sheet's pixels with its transparency key made transparent.
pub(crate) fn load(sheet: &tileset::Sheet) -> Option<image::RgbaImage> {
    let mut image = image::open(&sheet.file).ok()?.into_rgba8();
    if let Some(key) = sheet.key {
        for px in image.pixels_mut().filter(|px| px.0[..3] == key) {
            px.0[3] = 0;
        }
    }
    Some(image)
}

/// Decodes a sheet to RGBA under a filter, makes its transparency key transparent and cuts it into
/// strips of whole sprite rows.
fn decode(sheet: &tileset::Sheet, fx: Fx) -> Option<Vec<egui::ColorImage>> {
    let mut image = load(sheet)?;
    for px in image.pixels_mut() {
        let [r, g, b] = fx.pixel([px.0[0], px.0[1], px.0[2]]);
        px.0[..3].copy_from_slice(&[r, g, b]);
    }
    let rows = (STRIP / sheet.sprite.1).max(1) * sheet.sprite.1;
    let usable = image.height() / sheet.sprite.1 * sheet.sprite.1;
    Some(
        (0..usable)
            .step_by(rows as usize)
            .map(|y| {
                let h = rows.min(usable - y);
                let strip = image::imageops::crop_imm(&image, 0, y, image.width(), h).to_image();
                egui::ColorImage::from_rgba_unmultiplied(
                    [strip.width() as usize, strip.height() as usize],
                    strip.as_raw(),
                )
            })
            .collect(),
    )
}
