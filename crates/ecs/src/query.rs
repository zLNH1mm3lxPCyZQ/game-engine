use std::any::TypeId;
use std::marker::PhantomData;

use crate::archetype::Archetype;
use crate::column::{Column, Component, typed, typed_mut};
use crate::entity::Entity;

/// One archetype, borrowed for a query. Each column can be taken exactly once,
/// which is how a query proves it never holds two references to the same column.
#[doc(hidden)]
pub struct ArchetypeBorrow<'w> {
    types: &'w [TypeId],
    entities: &'w [Entity],
    columns: Vec<Option<&'w mut Box<dyn Column>>>,
}

impl<'w> ArchetypeBorrow<'w> {
    pub(crate) fn new(archetype: &'w mut Archetype) -> Self {
        let Archetype {
            types,
            columns,
            entities,
            ..
        } = archetype;
        Self {
            types,
            entities,
            columns: columns.iter_mut().map(Some).collect(),
        }
    }

    fn take(&mut self, type_id: TypeId) -> &'w mut Box<dyn Column> {
        let index = self
            .types
            .binary_search(&type_id)
            .expect("query requested a component this archetype doesn't have");
        self.columns[index]
            .take()
            .expect("a component type appears twice in one query, like (&A, &mut A)")
    }
}

/// What a query fetches for each matching entity.
/// Implemented for `&T`, `&mut T`, `Entity`, and tuples of those.
pub trait QueryData {
    type Item<'w>;
    #[doc(hidden)]
    type Iter<'w>: Iterator<Item = Self::Item<'w>>;

    /// Add the component types this query needs to `types`.
    fn required(types: &mut Vec<TypeId>);

    #[doc(hidden)]
    fn fetch<'w>(archetype: &mut ArchetypeBorrow<'w>) -> Self::Iter<'w>;
}

impl<T: Component> QueryData for &T {
    type Item<'w> = &'w T;
    type Iter<'w> = std::slice::Iter<'w, T>;

    fn required(types: &mut Vec<TypeId>) {
        types.push(TypeId::of::<T>());
    }

    fn fetch<'w>(archetype: &mut ArchetypeBorrow<'w>) -> Self::Iter<'w> {
        let column: &'w Box<dyn Column> = archetype.take(TypeId::of::<T>());
        typed::<T>(&**column).iter()
    }
}

impl<T: Component> QueryData for &mut T {
    type Item<'w> = &'w mut T;
    type Iter<'w> = std::slice::IterMut<'w, T>;

    fn required(types: &mut Vec<TypeId>) {
        types.push(TypeId::of::<T>());
    }

    fn fetch<'w>(archetype: &mut ArchetypeBorrow<'w>) -> Self::Iter<'w> {
        let column = archetype.take(TypeId::of::<T>());
        typed_mut::<T>(&mut **column).iter_mut()
    }
}

impl QueryData for Entity {
    type Item<'w> = Entity;
    type Iter<'w> = std::iter::Copied<std::slice::Iter<'w, Entity>>;

    fn required(_types: &mut Vec<TypeId>) {}

    fn fetch<'w>(archetype: &mut ArchetypeBorrow<'w>) -> Self::Iter<'w> {
        archetype.entities.iter().copied()
    }
}

/// Advances several iterators together, yielding a tuple of their items.
#[doc(hidden)]
pub struct TupleIter<T>(T);

macro_rules! impl_query_tuple {
    ($($name:ident),+) => {
        impl<$($name: QueryData),+> QueryData for ($($name,)+) {
            type Item<'w> = ($(<$name as QueryData>::Item<'w>,)+);
            type Iter<'w> = TupleIter<($(<$name as QueryData>::Iter<'w>,)+)>;

            fn required(types: &mut Vec<TypeId>) {
                $(<$name as QueryData>::required(types);)+
            }

            fn fetch<'w>(archetype: &mut ArchetypeBorrow<'w>) -> Self::Iter<'w> {
                TupleIter(($(<$name as QueryData>::fetch(archetype),)+))
            }
        }

        impl<$($name: Iterator),+> Iterator for TupleIter<($($name,)+)> {
            type Item = ($($name::Item,)+);

            #[allow(non_snake_case)]
            fn next(&mut self) -> Option<Self::Item> {
                let ($($name,)+) = &mut self.0;
                Some(($($name.next()?,)+))
            }
        }

        impl<$($name: QueryFilter),+> QueryFilter for ($($name,)+) {
            fn matches(types: &[TypeId]) -> bool {
                $(<$name as QueryFilter>::matches(types))&&+
            }
        }
    };
}

impl_query_tuple!(A);
impl_query_tuple!(A, B);
impl_query_tuple!(A, B, C);
impl_query_tuple!(A, B, C, D);
impl_query_tuple!(A, B, C, D, E);
impl_query_tuple!(A, B, C, D, E, F);
impl_query_tuple!(A, B, C, D, E, F, G);
impl_query_tuple!(A, B, C, D, E, F, G, H);

/// Which archetypes a query includes, beyond the components it fetches.
pub trait QueryFilter {
    fn matches(types: &[TypeId]) -> bool;
}

/// No filter.
impl QueryFilter for () {
    fn matches(_types: &[TypeId]) -> bool {
        true
    }
}

/// Only entities that have `T` (without fetching it).
pub struct With<T>(PhantomData<T>);

/// Only entities that don't have `T`.
pub struct Without<T>(PhantomData<T>);

impl<T: Component> QueryFilter for With<T> {
    fn matches(types: &[TypeId]) -> bool {
        types.binary_search(&TypeId::of::<T>()).is_ok()
    }
}

impl<T: Component> QueryFilter for Without<T> {
    fn matches(types: &[TypeId]) -> bool {
        types.binary_search(&TypeId::of::<T>()).is_err()
    }
}
