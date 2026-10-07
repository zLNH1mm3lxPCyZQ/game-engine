use std::any::TypeId;

use crate::archetype::Archetype;
use crate::column::{Column, Component};

/// A group of components spawned together. Implemented for tuples of up to 12 components.
/// A single component is a one-element tuple: `(Position(..),)`.
pub trait Bundle: 'static + Send + Sync {
    /// The component types, in tuple order.
    fn type_ids() -> Vec<TypeId>;

    /// One empty column per component, in tuple order.
    #[doc(hidden)]
    fn new_columns() -> Vec<Box<dyn Column>>;

    /// Push each component into its column of `archetype`.
    #[doc(hidden)]
    fn push_into(self, archetype: &mut Archetype);
}

macro_rules! impl_bundle {
    ($($name:ident),*) => {
        impl<$($name: Component),*> Bundle for ($($name,)*) {
            fn type_ids() -> Vec<TypeId> {
                vec![$(TypeId::of::<$name>()),*]
            }

            fn new_columns() -> Vec<Box<dyn Column>> {
                vec![$(Box::new(Vec::<$name>::new()) as Box<dyn Column>),*]
            }

            #[allow(non_snake_case, unused_variables)]
            fn push_into(self, archetype: &mut Archetype) {
                let ($($name,)*) = self;
                $(archetype.push($name);)*
            }
        }
    };
}

impl_bundle!();
impl_bundle!(A);
impl_bundle!(A, B);
impl_bundle!(A, B, C);
impl_bundle!(A, B, C, D);
impl_bundle!(A, B, C, D, E);
impl_bundle!(A, B, C, D, E, F);
impl_bundle!(A, B, C, D, E, F, G);
impl_bundle!(A, B, C, D, E, F, G, H);
impl_bundle!(A, B, C, D, E, F, G, H, I);
impl_bundle!(A, B, C, D, E, F, G, H, I, J);
impl_bundle!(A, B, C, D, E, F, G, H, I, J, K);
impl_bundle!(A, B, C, D, E, F, G, H, I, J, K, L);
