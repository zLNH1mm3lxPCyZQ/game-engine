use std::any::TypeId;
use std::collections::HashMap;

use crate::column::{Column, Component, typed_mut};
use crate::entity::Entity;

/// Where an entity's components live.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Location {
    pub archetype: usize,
    pub row: usize,
}

/// A table holding every entity with exactly one set of component types.
/// Row `i` of every column belongs to `entities[i]`.
///
/// `pub` because the public `Bundle` trait mentions it, but its module isn't
/// exported, so nothing outside this crate can name or use it.
#[doc(hidden)]
pub struct Archetype {
    /// Sorted, so the same set of types always has the same order.
    pub(crate) types: Vec<TypeId>,
    /// One column per type, in the same order as `types`.
    pub(crate) columns: Vec<Box<dyn Column>>,
    pub(crate) entities: Vec<Entity>,
    /// Cached transitions: component type added -> target archetype.
    pub(crate) add_edges: HashMap<TypeId, usize>,
    /// Cached transitions: component type removed -> target archetype.
    pub(crate) remove_edges: HashMap<TypeId, usize>,
}

impl Archetype {
    pub(crate) fn new(types: Vec<TypeId>, columns: Vec<Box<dyn Column>>) -> Self {
        debug_assert!(
            types.windows(2).all(|w| w[0] < w[1]),
            "types must be sorted and unique"
        );
        debug_assert_eq!(types.len(), columns.len());
        Self {
            types,
            columns,
            entities: Vec::new(),
            add_edges: HashMap::new(),
            remove_edges: HashMap::new(),
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.entities.len()
    }

    pub(crate) fn column_index(&self, type_id: TypeId) -> Option<usize> {
        self.types.binary_search(&type_id).ok()
    }

    pub(crate) fn column(&self, type_id: TypeId) -> Option<&dyn Column> {
        self.column_index(type_id).map(|i| self.columns[i].as_ref())
    }

    pub(crate) fn column_mut(&mut self, type_id: TypeId) -> Option<&mut dyn Column> {
        self.column_index(type_id).map(|i| self.columns[i].as_mut())
    }

    /// Push one component value into its column. Part of filling in a new row.
    pub(crate) fn push<T: Component>(&mut self, value: T) {
        let column = self
            .column_mut(TypeId::of::<T>())
            .expect("component type is not part of this archetype");
        typed_mut::<T>(column).push(value);
    }

    /// Remove a row from the entity list only (columns are handled by the caller).
    /// Returns the entity that moved into `row` to fill the hole, if any.
    pub(crate) fn remove_entity_row(&mut self, row: usize) -> Option<Entity> {
        self.entities.swap_remove(row);
        self.entities.get(row).copied()
    }

    /// Remove a row entirely, dropping its components.
    /// Returns the entity that moved into `row` to fill the hole, if any.
    pub(crate) fn swap_remove(&mut self, row: usize) -> Option<Entity> {
        for column in &mut self.columns {
            column.swap_remove(row);
        }
        self.remove_entity_row(row)
    }
}
