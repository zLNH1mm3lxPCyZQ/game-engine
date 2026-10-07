use std::any::{Any, TypeId};
use std::collections::HashMap;

/// One-of-a-kind data stored in the world. Implemented automatically for every suitable type.
pub trait Resource: 'static + Send + Sync {}

impl<T: 'static + Send + Sync> Resource for T {}

/// One value per type.
#[derive(Default)]
pub(crate) struct Resources {
    values: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl Resources {
    /// Store a value, returning the previous one of the same type, if any.
    pub fn insert<R: Resource>(&mut self, value: R) -> Option<R> {
        self.values
            .insert(TypeId::of::<R>(), Box::new(value))
            .map(|old| {
                *old.downcast::<R>()
                    .expect("resource stored under its own type")
            })
    }

    pub fn get<R: Resource>(&self) -> Option<&R> {
        self.values
            .get(&TypeId::of::<R>())
            .and_then(|value| (**value).downcast_ref::<R>())
    }

    pub fn get_mut<R: Resource>(&mut self) -> Option<&mut R> {
        self.values
            .get_mut(&TypeId::of::<R>())
            .and_then(|value| (**value).downcast_mut::<R>())
    }

    pub fn remove<R: Resource>(&mut self) -> Option<R> {
        self.values.remove(&TypeId::of::<R>()).map(|old| {
            *old.downcast::<R>()
                .expect("resource stored under its own type")
        })
    }

    pub fn contains<R: Resource>(&self) -> bool {
        self.values.contains_key(&TypeId::of::<R>())
    }
}
