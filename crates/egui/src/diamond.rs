//! The flat map of an isometric tileset, laid out as the game's tile view does (`cata_tiles.cpp`
//! `player_to_screen`): a map square is a diamond twice as wide as tall, one square east is half a tile right and a
//! quarter tile up, one south half a tile right and a quarter down, and the avatar's square is in the middle.

use bn_client_core::Pos;
use eframe::egui::{Pos2, Rect, Vec2, vec2};

/// Where the squares of an isometric flat map lie on the screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Diamond {
    /// The middle of the centre square's diamond.
    pub centre: Pos2,
    /// The centre square.
    pub at: Pos,
    /// The width of a square's diamond, which is also the width of its tile.
    pub width: f32,
    /// The height of a tile for a width of one, as the tileset says.
    pub ratio: f32,
}

impl Diamond {
    /// The tile box of a square: where the game draws the sprites of the square from, the diamond in its middle.
    pub fn tile(&self, p: Pos) -> Rect {
        let (dx, dy) = ((p.x - self.at.x) as f32, (p.y - self.at.y) as f32);
        let middle = self.centre + vec2(dx + dy, (dy - dx) / 2.0) * self.width / 2.0;
        Rect::from_center_size(middle, vec2(self.width, self.width * self.ratio))
    }

    /// The screen point of the ground at (`x`, `y`) in view squares.
    pub fn ground(&self, x: f32, y: f32) -> Pos2 {
        let (dx, dy) = (x - self.at.x as f32 - 0.5, y - self.at.y as f32 - 0.5);
        self.centre + vec2(dx + dy, (dy - dx) / 2.0) * self.width / 2.0
    }

    /// The corners of a square's diamond: left, top, right, bottom.
    pub fn corners(&self, p: Pos) -> [Pos2; 4] {
        let (x, y) = (p.x as f32, p.y as f32);
        [(x, y), (x + 1.0, y), (x + 1.0, y + 1.0), (x, y + 1.0)].map(|(x, y)| self.ground(x, y))
    }

    /// The square whose diamond holds the screen point `at`; the point may lie outside the view.
    pub fn square_at(&self, at: Pos2) -> Pos {
        let Vec2 { x: u, y: v } = (at - self.centre) / (self.width / 2.0);
        // `u` is east plus south of the centre square's middle, `v` twice south minus east.
        let (east, south) = ((u - 2.0 * v) / 2.0, u / 2.0 + v);
        Pos {
            x: self.at.x + (east + 0.5).floor() as i32,
            y: self.at.y + (south + 0.5).floor() as i32,
        }
    }

    /// The squares in the order the game draws them: row by row from the top of the screen, left to right.
    pub fn order(&self, size: Pos) -> Vec<Pos> {
        let mut squares = (0..size.y)
            .flat_map(|y| (0..size.x).map(move |x| Pos { x, y }))
            .collect::<Vec<_>>();
        squares.sort_by_key(|p| {
            let (dx, dy) = (p.x - self.at.x, p.y - self.at.y);
            (dy - dx, dx + dy)
        });
        squares
    }
}
