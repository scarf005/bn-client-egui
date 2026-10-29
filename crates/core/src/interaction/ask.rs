//! What the player is asked for: the arguments of one affordance, gathered from the focus, the marked rows and
//! what the player types.

use super::*;

/// A text field's input: what it takes, from the form field it edits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Filter {
    #[default]
    Any,
    /// A whole number within the field's bounds.
    Integer {
        min: Option<i64>,
        max: Option<i64>,
    },
    Float,
}

/// The options of a `select` field and the one highlighted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Select {
    pub options: Vec<FieldOption>,
    pub at: usize,
}

/// What the player is being asked for: the arguments of one affordance being collected. A renderer only
/// shows it; every change goes through `Interaction`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ask {
    pub affordance_id: String,
    /// What the count would be sent as: the typed one, else the shape's default, held to its bounds.
    pub count: Option<i64>,
    /// The text typed, or a field's current value to start from.
    pub text: Option<String>,
    /// The panels that can answer a `pane` argument.
    pub panes: Vec<String>,
    /// The panel highlighted among `panes`.
    pub pane_at: usize,
    /// The options when the text is a `select` field's choice.
    pub select: Option<Select>,
    /// The next key pressed is the answer, as for a `key` field.
    pub capture: bool,
    /// The square or tile chosen, for a `pos` argument.
    pub pos: Option<WorldPos>,
    pub pane: Option<String>,
    /// The member being asked for now; none when every one is answered.
    pub asking: Option<ArgKind>,
    /// The count digits typed; none until typed.
    digits: Option<String>,
    /// The count is the shape's default, which the first digit typed replaces.
    fresh: bool,
    /// The text is the engine's suggestion, which the first character typed or a backspace replaces.
    pub(super) suggested: bool,
    bounds: Option<CountShape>,
    /// The members still to answer, in the order they are asked.
    pub(super) todo: Vec<ArgKind>,
    pub(super) filter: Filter,
    pub(super) max_len: Option<usize>,
}

impl Ask {
    fn new(affordance: &Affordance) -> Self {
        let bounds = affordance
            .args
            .as_ref()
            .and_then(|shape| shape.count.clone());
        let default = bounds.as_ref().and_then(|bounds| bounds.default);
        let mut ask = Self {
            affordance_id: affordance.id.clone(),
            count: None,
            text: None,
            panes: vec![],
            pane_at: 0,
            select: None,
            capture: false,
            pos: None,
            pane: None,
            asking: None,
            digits: default.map(|n| n.to_string()),
            fresh: default.is_some(),
            suggested: false,
            bounds,
            todo: vec![],
            filter: Filter::Any,
            max_len: None,
        };
        ask.recount();
        ask
    }

    /// The count as it would be sent: typed digits, else the default, held to the bounds.
    fn recount(&mut self) {
        let Some(bounds) = &self.bounds else {
            return;
        };
        let typed = self
            .digits
            .as_deref()
            .and_then(|digits| digits.parse::<i64>().ok());
        self.count = typed.or(bounds.default).map(|n| {
            let (min, max) = (
                bounds.min.unwrap_or(i64::MIN),
                bounds.max.unwrap_or(i64::MAX),
            );
            n.clamp(min, max.max(min))
        });
    }

    fn set_todo(&mut self, todo: Vec<ArgKind>) {
        self.asking = todo.first().copied();
        self.todo = todo;
    }
}

impl Interaction {
    /// Starts collecting the arguments of `affordance` and returns what the client must still ask the player
    /// for: the focus and the marked rows already answer `row` and `rows`. A count or a text is always asked,
    /// starting from the shape's default; a position or a pane until it is answered.
    pub fn begin(&mut self, affordance: &Affordance) -> Vec<ArgKind> {
        self.open(affordance, None)
    }

    /// `begin`, which also reads from `screen` what the arguments are chosen among: the field a text edits and
    /// the panels a pane is one of.
    pub(super) fn open(
        &mut self,
        affordance: &Affordance,
        screen: Option<&Screen>,
    ) -> Vec<ArgKind> {
        let shape = affordance.args.clone().unwrap_or_default();
        let mut ask = Ask::new(affordance);
        let todo = [
            (shape.count.is_some(), ArgKind::Count),
            (shape.text.is_some(), ArgKind::Text),
            (shape.pos.is_some(), ArgKind::Pos),
            (shape.pane.is_some(), ArgKind::Pane),
        ]
        .into_iter()
        .filter_map(|(asked, kind)| asked.then_some(kind))
        .collect::<Vec<_>>();
        ask.text = shape
            .text
            .as_ref()
            .map(|t| t.default.clone().unwrap_or_default());
        ask.suggested = shape.text.as_ref().is_some_and(|t| t.default.is_some());
        if let Some(screen) = screen {
            self.seed(&mut ask, screen, &shape);
        }
        let rows = self
            .collect(affordance)
            .1
            .into_iter()
            .filter(|kind| matches!(kind, ArgKind::Row | ArgKind::Rows));
        let asked = rows.chain(todo.iter().copied()).collect();
        ask.set_todo(todo);
        self.ask = Some(ask);
        asked
    }

    /// Types a digit (or a leading minus) of the count.
    pub fn push_count(&mut self, c: char) -> bool {
        let Some(ask) = self.ask.as_mut() else {
            return false;
        };
        if std::mem::take(&mut ask.fresh) {
            ask.digits = None;
        }
        let typed = ask.digits.get_or_insert_default();
        let fits = c.is_ascii_digit() || (c == '-' && typed.is_empty());
        if fits {
            typed.push(c);
        }
        ask.recount();
        fits
    }

    pub fn pop_count(&mut self) {
        if let Some(ask) = self.ask.as_mut() {
            ask.fresh = false;
            ask.digits.as_mut().map(String::pop);
            ask.recount();
        }
    }

    /// Types text, keeping the characters the field takes and as many as it holds.
    pub fn push_text(&mut self, text: &str) {
        let Some(ask) = self.ask.as_mut() else {
            return;
        };
        let (filter, max_len) = (ask.filter, ask.max_len);
        if std::mem::take(&mut ask.suggested) {
            ask.text = None;
        }
        let typed = ask.text.get_or_insert_default();
        for c in text.chars().filter(|&c| filter.takes(c)) {
            if max_len.is_none_or(|max| typed.chars().count() < max) {
                typed.push(c);
            }
        }
    }

    pub fn pop_text(&mut self) {
        let Some(ask) = self.ask.as_mut() else {
            return;
        };
        match (std::mem::take(&mut ask.suggested), ask.text.as_mut()) {
            (true, Some(typed)) => typed.clear(),
            (false, Some(typed)) => {
                typed.pop();
            }
            (_, None) => {}
        }
    }

    /// Replaces the text typed.
    pub fn answer_text(&mut self, text: &str) {
        if let Some(ask) = self.ask.as_mut() {
            ask.text = Some(text.into());
            ask.suggested = false;
        }
    }

    pub fn set_pos(&mut self, pos: WorldPos) {
        if let Some(ask) = self.ask.as_mut() {
            ask.pos = Some(pos);
        }
    }

    pub fn set_pane(&mut self, pane_id: &str) {
        if let Some(ask) = self.ask.as_mut() {
            ask.pane = Some(pane_id.into());
        }
    }

    /// What the player is being asked for, if an affordance is collecting its arguments.
    pub fn ask(&self) -> Option<&Ask> {
        self.ask.as_ref()
    }

    /// Ends the collection without invoking.
    pub fn cancel(&mut self) {
        self.ask = None;
    }

    /// The arguments `affordance` asks for, from the focus, the marked rows and the ask; else the members
    /// still missing. A count is held to the shape's bounds, and defaults to the shape's default.
    pub fn args(&self, affordance: &Affordance) -> Result<InvokeArgs, Vec<ArgKind>> {
        match self.collect(affordance) {
            (args, missing) if missing.is_empty() => Ok(args),
            (_, missing) => Err(missing),
        }
    }

    /// The arguments gathered so far and the members still missing.
    pub(super) fn collect(&self, affordance: &Affordance) -> (InvokeArgs, Vec<ArgKind>) {
        let none = ArgShape::default();
        let shape = affordance.args.as_ref().unwrap_or(&none);
        let unasked;
        let ask = match self
            .ask
            .as_ref()
            .filter(|ask| ask.affordance_id == affordance.id)
        {
            Some(ask) => ask,
            None => {
                unasked = Ask::new(affordance);
                &unasked
            }
        };
        let mut missing = vec![];
        let mut args = InvokeArgs::default();
        if let Some(of) = &shape.row {
            args.row = self.focused_in(&of.component_id).map(String::from);
            if args.row.is_none() {
                missing.push(ArgKind::Row);
            }
        }
        if let Some(of) = &shape.rows {
            let counted = of.counts == Some(true);
            let picks = self.picks(&of.component_id);
            let rows = match (picks.is_empty(), self.focused_in(&of.component_id)) {
                (false, _) => picks
                    .iter()
                    .map(|pick| RowPick {
                        id: pick.id.clone(),
                        count: pick.count.filter(|_| counted),
                    })
                    .collect(),
                (true, Some(id)) => vec![RowPick {
                    id: id.into(),
                    count: None,
                }],
                (true, None) => vec![],
            };
            match rows.is_empty() {
                true => missing.push(ArgKind::Rows),
                false => args.rows = Some(rows),
            }
        }
        if shape.count.is_some() {
            args.count = ask.count;
            if args.count.is_none() {
                missing.push(ArgKind::Count);
            }
        }
        if shape.text.is_some() {
            args.text = ask.text.clone();
            if args.text.is_none() {
                missing.push(ArgKind::Text);
            }
        }
        if shape.pos.is_some() {
            args.pos = ask.pos.clone();
            if args.pos.is_none() {
                missing.push(ArgKind::Pos);
            }
        }
        if shape.pane.is_some() {
            args.pane = ask.pane.clone();
            if args.pane.is_none() {
                missing.push(ArgKind::Pane);
            }
        }
        (args, missing)
    }

    /// The arguments to invoke `affordance` with, ending the collection and clearing the rows it used.
    pub fn finish(&mut self, affordance: &Affordance) -> Result<InvokeArgs, Vec<ArgKind>> {
        let args = self.args(affordance)?;
        if let Some(of) = affordance
            .args
            .as_ref()
            .and_then(|shape| shape.rows.as_ref())
        {
            self.clear_picks(&of.component_id);
        }
        self.ask = None;
        Ok(args)
    }
}
