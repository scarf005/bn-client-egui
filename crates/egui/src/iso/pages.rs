//! The tileset's sheets as the 3D scene's textures: decoded in the background, a strip of rows per page.

use super::geometry::PageId;
use crate::display::{STRIP, load};
use crate::tileset::Sheet;
use eframe::egui;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// The decoded pages, filling as the sheets finish.
pub struct Pages {
    images: Mutex<HashMap<PageId, Arc<image::RgbaImage>>>,
    /// Sheets not decoded yet.
    waiting: AtomicUsize,
}

impl Pages {
    /// Starts decoding `sheets`; each finished sheet wakes the UI.
    pub fn start(sheets: Vec<Sheet>, ctx: egui::Context) -> Arc<Self> {
        let pages = Arc::new(Self {
            images: Mutex::default(),
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
    let rows = (STRIP / sheet.sprite.1).max(1) * sheet.sprite.1;
    let usable = image.height() / sheet.sprite.1 * sheet.sprite.1;
    (0..usable)
        .step_by(rows as usize)
        .map(|y| {
            image::imageops::crop_imm(&image, 0, y, image.width(), rows.min(usable - y)).to_image()
        })
        .collect()
}
