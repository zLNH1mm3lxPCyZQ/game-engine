use std::any::Any;

/// Anything storable in the ECS. Implemented automatically for every suitable type.
pub trait Component: 'static + Send + Sync {}

impl<T: 'static + Send + Sync> Component for T {}

/// A column of components of one type, with that type hidden.
/// It's always a `Vec<T>` underneath; typed code gets it back with `typed` / `typed_mut`.
///
/// `pub` because the public `Bundle` trait mentions it, but its module isn't
/// exported, so nothing outside this crate can name or use it.
#[doc(hidden)]
pub trait Column: Any + Send + Sync {
    fn len(&self) -> usize;

    /// Remove and drop the value at `row`. The last value moves into its place.
    fn swap_remove(&mut self, row: usize);

    /// Remove the value at `row` and push it onto `target`, which must hold the same type.
    /// The last value moves into `row`'s place here.
    fn move_row_to(&mut self, row: usize, target: &mut dyn Column);

    /// A new, empty column of the same component type.
    fn new_empty(&self) -> Box<dyn Column>;
}

impl<T: Component> Column for Vec<T> {
    fn len(&self) -> usize {
        Vec::len(self)
    }

    fn swap_remove(&mut self, row: usize) {
        Vec::swap_remove(self, row);
    }

    fn move_row_to(&mut self, row: usize, target: &mut dyn Column) {
        let value = Vec::swap_remove(self, row);
        typed_mut::<T>(target).push(value);
    }

    fn new_empty(&self) -> Box<dyn Column> {
        Box::new(Vec::<T>::new())
    }
}

/// View a type-erased column as `Vec<T>`. Panics if the column holds another type.
pub(crate) fn typed<T: Component>(column: &dyn Column) -> &Vec<T> {
    (column as &dyn Any)
        .downcast_ref::<Vec<T>>()
        .expect("column holds a different component type")
}

/// Mutable version of `typed`.
pub(crate) fn typed_mut<T: Component>(column: &mut dyn Column) -> &mut Vec<T> {
    (column as &mut dyn Any)
        .downcast_mut::<Vec<T>>()
        .expect("column holds a different component type")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct Position(f32);

    fn column_of(values: Vec<Position>) -> Box<dyn Column> {
        Box::new(values)
    }

    #[test]
    fn swap_remove_moves_the_last_value_into_the_hole() {
        let mut column = column_of(vec![Position(1.0), Position(2.0), Position(3.0)]);
        column.swap_remove(0);
        assert_eq!(
            typed::<Position>(column.as_ref()),
            &vec![Position(3.0), Position(2.0)]
        );
    }

    #[test]
    fn rows_move_between_columns_of_the_same_type() {
        let mut from = column_of(vec![Position(1.0), Position(2.0)]);
        let mut to = from.new_empty();

        from.move_row_to(0, to.as_mut());

        assert_eq!(typed::<Position>(from.as_ref()), &vec![Position(2.0)]);
        assert_eq!(typed::<Position>(to.as_ref()), &vec![Position(1.0)]);
    }

    #[test]
    fn new_empty_keeps_the_type() {
        let column = column_of(vec![Position(1.0)]);
        let empty = column.new_empty();
        assert_eq!(empty.len(), 0);
        assert!(typed::<Position>(empty.as_ref()).is_empty());
    }

    #[test]
    #[should_panic(expected = "different component type")]
    fn wrong_type_panics() {
        let column = column_of(vec![Position(1.0)]);
        let _ = typed::<u32>(column.as_ref());
    }
}
