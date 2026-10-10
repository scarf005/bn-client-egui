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

/// One selectable row of the engine's current interaction menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuChoice {
    pub id: String,
    pub label: String,
    pub enabled: bool,
    /// Why the row cannot be chosen; empty when enabled.
    pub denial: String,
}

/// The engine's current interaction, rendered generically.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Menu {
    pub title: String,
    pub message: String,
    pub choices: Vec<MenuChoice>,
    pub can_cancel: bool,
}

/// What the UI needs from the game: `FakeModel` in tests, `EngineModel` against a real engine.
pub trait GameModel {
    /// Called once per frame: drain engine messages. No-op for synchronous models.
    fn poll(&mut self) {}
    /// True while the model keeps changing on its own and needs periodic repaints.
    fn live(&self) -> bool {
        false
    }
    /// True while a submitted command has not finished; input is ignored until then.
    fn busy(&self) -> bool {
        false
    }
    fn menu(&self) -> Option<Menu> {
        None
    }
    fn choose(&mut self, _choice_id: &str) {}
    fn cancel_menu(&mut self) {}
    fn size(&self) -> Pos;
    fn cell(&self, at: Pos) -> Cell;
    fn player(&self) -> Pos;
    /// Moves the player one step toward `target` (diagonals allowed); false if blocked.
    fn step_toward(&mut self, target: Pos) -> bool;
    fn ground(&self) -> &[String];
    fn inventory(&self) -> &[String];
    fn pick_up(&mut self, index: usize);
    fn drop_item(&mut self, index: usize);
    fn messages(&self) -> &[String];
    fn say(&mut self, text: String);
}

pub struct FakeModel {
    /// Map size; tests shrink it to emulate loading or a replaced map.
    pub size: Pos,
    player: Pos,
    ground: Vec<String>,
    inventory: Vec<String>,
    messages: Vec<String>,
}

impl Default for FakeModel {
    fn default() -> Self {
        Self {
            size: Pos { x: 40, y: 25 },
            player: Pos { x: 5, y: 5 },
            ground: vec!["Rusty knife".into(), "Tin can".into()],
            inventory: vec!["Flashlight".into()],
            messages: vec!["Welcome to the prototype.".into()],
        }
    }
}

impl GameModel for FakeModel {
    fn size(&self) -> Pos {
        self.size
    }

    fn cell(&self, at: Pos) -> Cell {
        if at.x == 20 && at.y != 12 {
            return Cell {
                glyph: '#',
                color: Color32::from_rgb(180, 180, 180),
                desc: "a wall".into(),
            };
        }
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

    fn step_toward(&mut self, target: Pos) -> bool {
        let next = Pos {
            x: self.player.x + (target.x - self.player.x).signum(),
            y: self.player.y + (target.y - self.player.y).signum(),
        };
        let blocked = self.cell(next).glyph == '#';
        if !blocked {
            self.player = next;
        }
        !blocked
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
