//! The rows of long lists past their first page.

use super::*;

/// A page of a list to ask `bn.interaction.choices` for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageRequest {
    pub component_id: String,
    pub offset: u64,
    pub limit: u64,
}

#[derive(Debug, Default)]
struct Fetched {
    rows: Vec<Row>,
    waiting: bool,
    /// Nothing more is asked for until the boundary changes: a page was refused or came back empty.
    closed: bool,
}

/// The rows of long lists past their first page, fetched by component id for one boundary.
#[derive(Debug, Default)]
pub struct Pages {
    boundary_id: String,
    lists: HashMap<String, Fetched>,
}

impl Pages {
    /// The next page of `list` to ask for, once: none when every row is held or the page is on its way.
    pub fn next(&mut self, boundary_id: &str, list: &List) -> Option<PageRequest> {
        if self.boundary_id != boundary_id {
            *self = Self {
                boundary_id: boundary_id.into(),
                ..Self::default()
            };
        }
        let held = self.lists.entry(list.head.id.clone()).or_default();
        let loaded = list.offset.unwrap_or(0) + (list.rows.len() + held.rows.len()) as u64;
        if held.waiting || held.closed || loaded >= list.total {
            return None;
        }
        held.waiting = true;
        Some(PageRequest {
            component_id: list.head.id.clone(),
            offset: loaded,
            limit: PAGE_ROWS,
        })
    }

    /// Adds an answer to the rows held; an answer for another boundary or a list not asked for is dropped.
    pub fn accept(&mut self, page: RowsResult) {
        let held = self.lists.get_mut(&page.component_id);
        if let (true, Some(held)) = (page.boundary_id == self.boundary_id, held) {
            held.waiting = false;
            held.closed = page.rows.is_empty();
            held.rows.extend(page.rows);
        }
    }

    /// A page was refused; the list is not asked again for this boundary.
    pub fn failed(&mut self, component_id: &str) {
        if let Some(held) = self.lists.get_mut(component_id) {
            held.waiting = false;
            held.closed = true;
        }
    }

    /// The rows of the list that are known: its first page, then the pages fetched.
    pub fn rows<'a>(&'a self, boundary_id: &str, list: &'a List) -> Vec<&'a Row> {
        let held = self
            .lists
            .get(&list.head.id)
            .filter(|_| self.boundary_id == boundary_id);
        list.rows
            .iter()
            .chain(held.into_iter().flat_map(|held| held.rows.iter()))
            .collect()
    }
}
