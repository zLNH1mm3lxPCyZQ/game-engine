use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::archetype::Location;

/// A handle to an entity: a unique number, never reused within a world.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Entity(u64);

impl Entity {
    pub fn id(self) -> u64 {
        self.0
    }

    /// Rebuild a handle from a stored id (scene files, save games).
    pub fn from_id(id: u64) -> Self {
        Self(id)
    }
}

impl fmt::Debug for Entity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Entity({})", self.0)
    }
}

/// Allocates entities and records where each one's components live.
#[derive(Default, Debug)]
pub(crate) struct Entities {
    /// Shared with command buffers, so they can reserve ids without borrowing the world.
    next: Arc<AtomicU64>,
    locations: HashMap<Entity, Location>,
}

impl Entities {
    /// A fresh entity. It isn't alive until `set_location` is called.
    pub fn alloc(&mut self) -> Entity {
        Entity(self.next.fetch_add(1, Ordering::Relaxed))
    }

    /// The id counter, for command buffers.
    pub fn id_counter(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.next)
    }

    pub fn set_location(&mut self, entity: Entity, location: Location) {
        self.locations.insert(entity, location);
    }

    pub fn location(&self, entity: Entity) -> Option<Location> {
        self.locations.get(&entity).copied()
    }

    /// Forget an entity. Returns where it was, or `None` if it wasn't alive.
    pub fn free(&mut self, entity: Entity) -> Option<Location> {
        self.locations.remove(&entity)
    }

    pub fn is_alive(&self, entity: Entity) -> bool {
        self.locations.contains_key(&entity)
    }

    pub fn len(&self) -> usize {
        self.locations.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HERE: Location = Location {
        archetype: 0,
        row: 0,
    };

    #[test]
    fn allocates_distinct_entities() {
        let mut entities = Entities::default();
        let a = entities.alloc();
        let b = entities.alloc();
        assert_ne!(a, b);
    }

    #[test]
    fn freed_ids_are_never_reused() {
        let mut entities = Entities::default();
        let a = entities.alloc();
        entities.set_location(a, HERE);
        assert!(entities.free(a).is_some());
        let b = entities.alloc();
        assert_ne!(a, b);
        assert!(!entities.is_alive(a));
    }

    #[test]
    fn double_free_is_rejected() {
        let mut entities = Entities::default();
        let a = entities.alloc();
        entities.set_location(a, HERE);
        assert!(entities.free(a).is_some());
        assert!(entities.free(a).is_none());
        assert_eq!(entities.len(), 0);
    }
}
