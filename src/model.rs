use eframe::egui::Color32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pos {
    pub x: i32,
    pub y: i32,
}

pub struct Cell {
    pub glyph: char,
    pub color: Color32,
    pub desc: String,
}

/// What the UI needs from the game. The engine protocol will replace `FakeModel` behind this.
pub trait GameModel {
    fn size(&self) -> Pos;
    fn cell(&self, at: Pos) -> Cell;
    fn player(&self) -> Pos;
    /// Moves the player one step toward `target` (diagonals allowed).
    fn step_toward(&mut self, target: Pos);
    fn ground(&self) -> &[String];
    fn inventory(&self) -> &[String];
    fn pick_up(&mut self, index: usize);
    fn drop_item(&mut self, index: usize);
    fn messages(&self) -> &[String];
    fn say(&mut self, text: String);
}

pub struct FakeModel {
    player: Pos,
    ground: Vec<String>,
    inventory: Vec<String>,
    messages: Vec<String>,
}

impl Default for FakeModel {
    fn default() -> Self {
        Self {
            player: Pos { x: 5, y: 5 },
            ground: vec!["Rusty knife".into(), "Tin can".into()],
            inventory: vec!["Flashlight".into()],
            messages: vec!["Welcome to the prototype.".into()],
        }
    }
}

impl GameModel for FakeModel {
    fn size(&self) -> Pos {
        Pos { x: 40, y: 25 }
    }

    fn cell(&self, at: Pos) -> Cell {
        let (glyph, color, desc) = match (at.x * 7 + at.y * 13) % 11 {
            0 => ('T', Color32::from_rgb(40, 160, 60), "a tree"),
            1 => ('~', Color32::from_rgb(70, 120, 220), "shallow water"),
            _ => ('.', Color32::from_rgb(130, 130, 110), "open grass"),
        };
        Cell {
            glyph,
            color,
            desc: desc.into(),
        }
    }

    fn player(&self) -> Pos {
        self.player
    }

    fn step_toward(&mut self, target: Pos) {
        self.player.x += (target.x - self.player.x).signum();
        self.player.y += (target.y - self.player.y).signum();
    }

    fn ground(&self) -> &[String] {
        &self.ground
    }

    fn inventory(&self) -> &[String] {
        &self.inventory
    }

    fn pick_up(&mut self, index: usize) {
        if index < self.ground.len() {
            self.inventory.push(self.ground.remove(index));
        }
    }

    fn drop_item(&mut self, index: usize) {
        if index < self.inventory.len() {
            self.ground.push(self.inventory.remove(index));
        }
    }

    fn messages(&self) -> &[String] {
        &self.messages
    }

    fn say(&mut self, text: String) {
        self.messages.push(text);
    }
}
