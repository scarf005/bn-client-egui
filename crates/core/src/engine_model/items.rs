//! The rows of the items on the avatar's square and in its inventory, and how each draws.

use super::*;

/// The engine's id of an item row and how many items it stands for.
pub(super) type ItemRow = (String, u64);

/// The engine's ids of `rows`.
pub(super) fn ids(rows: &[ItemRow]) -> Vec<String> {
    rows.iter().map(|(id, _)| id.clone()).collect()
}

/// The row the engine names `item_id`; none for an empty id or a row that is gone.
pub(super) fn row_of(rows: &[ItemRow], item_id: &str) -> Option<ItemRow> {
    rows.iter()
        .find(|(id, _)| !id.is_empty() && id == item_id)
        .cloned()
}

impl EngineModel {
    pub(super) fn refresh_items(&mut self) {
        let avatar = self.replica.avatar();
        let row = |look: &Look, line: String, item: ItemRow| (line, item, self.item_cell(look));
        let stack = |i: &protocol::InventoryItem| {
            row(
                &i.look,
                i.label.clone(),
                (i.id.clone(), i.count.unwrap_or(1)),
            )
        };
        let ground = match avatar.and_then(|a| a.ground.as_ref()) {
            Some(stacks) => stacks.iter().map(stack).collect(),
            None => self
                .avatar_pos()
                .and_then(|p| self.replica.cell(&p.dim, p.x, p.y, p.z))
                .map_or(vec![], |c| {
                    c.items
                        .iter()
                        .map(|l| row(l, look_name(l), (String::new(), 1)))
                        .collect()
                }),
        };
        let carried = avatar.map_or(vec![], |a| {
            a.inventory.iter().flatten().map(stack).collect()
        });
        (self.ground, self.ground_items, self.ground_cells) = unzip3(ground);
        (self.inventory, self.inventory_items, self.inventory_cells) = unzip3(carried);
    }

    /// How a list row draws its item: the same layers a map square gets for it.
    pub(super) fn item_cell(&self, look: &Look) -> Cell {
        Cell {
            layers: vec![self.layer(look, 0)],
            ..self.styled(look, look_name(look))
        }
    }
}

fn unzip3<A, B, C>(rows: Vec<(A, B, C)>) -> (Vec<A>, Vec<B>, Vec<C>) {
    let (mut a, mut b, mut c) = (vec![], vec![], vec![]);
    for (x, y, z) in rows {
        a.push(x);
        b.push(y);
        c.push(z);
    }
    (a, b, c)
}
