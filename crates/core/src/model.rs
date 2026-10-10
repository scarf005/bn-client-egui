#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pos {
    pub x: i32,
    pub y: i32,
}

/// 8-bit sRGB colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

pub struct Cell {
    pub glyph: char,
    pub color: Rgb,
    /// Background behind the glyph, when the game colour has one.
    pub bg: Option<Rgb>,
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

/// A registered action the engine currently accepts, with the keys the game binds to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionEntry {
    pub id: String,
    pub name: String,
    pub keys: Vec<String>,
}

/// Engine loading screen: a title, the steps seen so far and the one in progress.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Loading {
    pub title: String,
    pub entries: Vec<String>,
    pub index: usize,
    /// Splash image, relative to the game root.
    pub image: Option<String>,
}

/// What the UI needs from the game: `FakeModel` in tests, `EngineModel` against a real engine.
pub trait GameModel {
    /// Called once per frame: drain engine messages. No-op for synchronous models.
    fn poll(&mut self) {}
    /// Registers a callback the model runs (from any thread) when new engine data has arrived.
    fn set_waker(&mut self, _waker: crate::transport::Waker) {}
    /// Absolute map position of view cell (0, 0); lets a walk target survive the view scrolling.
    fn origin(&self) -> Pos {
        Pos { x: 0, y: 0 }
    }
    /// True while the model keeps changing on its own and needs periodic repaints.
    fn live(&self) -> bool {
        false
    }
    /// True while a submitted command has not finished; input is ignored until then.
    fn busy(&self) -> bool {
        false
    }
    /// Actions the engine accepts right now.
    fn actions(&self) -> Vec<ActionEntry> {
        vec![]
    }
    /// Runs the action with this id; false if it is not offered or the model is busy.
    fn invoke_action(&mut self, _id: &str) -> bool {
        false
    }
    /// A game key token (`g`, `SPACE`, `UP`); true if it ran an action.
    fn press(&mut self, _key: &str) -> bool {
        false
    }
    /// The engine's loading screen, while it is up.
    fn loading(&self) -> Option<Loading> {
        None
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
                color: Rgb(180, 180, 180),
                bg: None,
                desc: "a wall".into(),
            };
        }
        let (glyph, color, desc) = match (at.x * 7 + at.y * 13) % 11 {
            0 => ('T', Rgb(40, 160, 60), "a tree"),
            1 => ('~', Rgb(70, 120, 220), "shallow water"),
            _ => ('.', Rgb(130, 130, 110), "open grass"),
        };
        Cell {
            glyph,
            color,
            bg: None,
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
