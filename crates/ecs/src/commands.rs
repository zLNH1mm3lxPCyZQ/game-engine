use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::bundle::Bundle;
use crate::column::Component;
use crate::entity::Entity;
use crate::resource::Resource;
use crate::world::World;

type Command = Box<dyn FnOnce(&mut World) + Send>;

/// A queue of changes to apply to the world later, with `World::apply`.
/// Use it to change the world's structure while iterating a query.
pub struct Commands {
    ids: Arc<AtomicU64>,
    queue: Vec<Command>,
}

impl Commands {
    pub(crate) fn new(ids: Arc<AtomicU64>) -> Self {
        Self {
            ids,
            queue: Vec::new(),
        }
    }

    /// Queue a spawn. The entity id is reserved now and usable right away
    /// (for example in later commands); the entity exists once applied.
    pub fn spawn<B: Bundle>(&mut self, bundle: B) -> Entity {
        let entity = Entity::from_id(self.ids.fetch_add(1, Ordering::Relaxed));
        self.add(move |world| world.spawn_at(entity, bundle));
        entity
    }

    pub fn despawn(&mut self, entity: Entity) {
        self.add(move |world| {
            world.despawn(entity);
        });
    }

    pub fn insert<T: Component>(&mut self, entity: Entity, value: T) {
        self.add(move |world| {
            world.insert(entity, value);
        });
    }

    pub fn remove<T: Component>(&mut self, entity: Entity) {
        self.add(move |world| {
            world.remove::<T>(entity);
        });
    }

    pub fn insert_resource<R: Resource>(&mut self, value: R) {
        self.add(move |world| {
            world.insert_resource(value);
        });
    }

    /// Queue any change at all.
    pub fn add(&mut self, command: impl FnOnce(&mut World) + Send + 'static) {
        self.queue.push(Box::new(command));
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub(crate) fn belongs_to(&self, ids: &Arc<AtomicU64>) -> bool {
        Arc::ptr_eq(&self.ids, ids)
    }

    pub(crate) fn into_queue(self) -> Vec<Command> {
        self.queue
    }
}
