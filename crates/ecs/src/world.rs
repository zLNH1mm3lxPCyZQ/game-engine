use std::any::TypeId;
use std::collections::HashMap;

use crate::archetype::{Archetype, Location};
use crate::bundle::Bundle;
use crate::column::{Column, Component, typed, typed_mut};
use crate::commands::Commands;
use crate::entity::{Entities, Entity};
use crate::event::Events;
use crate::query::{ArchetypeBorrow, QueryData, QueryFilter};
use crate::resource::{Resource, Resources};

/// Owns all entities and their components.
#[derive(Default)]
pub struct World {
    entities: Entities,
    archetypes: Vec<Archetype>,
    /// Sorted component types -> index into `archetypes`.
    archetype_index: HashMap<Vec<TypeId>, usize>,
    resources: Resources,
}

impl World {
    pub fn new() -> Self {
        Self::default()
    }

    // ---------------------------------------------------------------------
    // Entities
    // ---------------------------------------------------------------------

    /// Create an entity with the given components.
    pub fn spawn<B: Bundle>(&mut self, bundle: B) -> Entity {
        let entity = self.entities.alloc();
        self.spawn_at(entity, bundle);
        entity
    }

    /// Spawn with an id reserved earlier (by `Commands`).
    pub(crate) fn spawn_at<B: Bundle>(&mut self, entity: Entity, bundle: B) {
        debug_assert!(!self.is_alive(entity), "entity {entity:?} is already alive");
        let archetype_index = self.archetype_for::<B>();
        let archetype = &mut self.archetypes[archetype_index];

        let row = archetype.len();
        archetype.entities.push(entity);
        bundle.push_into(archetype);

        self.entities.set_location(
            entity,
            Location {
                archetype: archetype_index,
                row,
            },
        );
    }

    /// Remove an entity and all its components. Returns false if it wasn't alive.
    pub fn despawn(&mut self, entity: Entity) -> bool {
        let Some(location) = self.entities.free(entity) else {
            return false;
        };
        let moved = self.archetypes[location.archetype].swap_remove(location.row);
        if let Some(moved) = moved {
            // The last entity of the table filled the hole: it lives at this row now.
            self.entities.set_location(moved, location);
        }
        true
    }

    pub fn is_alive(&self, entity: Entity) -> bool {
        self.entities.is_alive(entity)
    }

    /// Number of living entities.
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    // ---------------------------------------------------------------------
    // Components
    // ---------------------------------------------------------------------

    pub fn get<T: Component>(&self, entity: Entity) -> Option<&T> {
        let location = self.entities.location(entity)?;
        let column = self.archetypes[location.archetype].column(TypeId::of::<T>())?;
        typed::<T>(column).get(location.row)
    }

    pub fn get_mut<T: Component>(&mut self, entity: Entity) -> Option<&mut T> {
        let location = self.entities.location(entity)?;
        let column = self.archetypes[location.archetype].column_mut(TypeId::of::<T>())?;
        typed_mut::<T>(column).get_mut(location.row)
    }

    pub fn has<T: Component>(&self, entity: Entity) -> bool {
        self.get::<T>(entity).is_some()
    }

    /// Add a component, or replace it if the entity already has one.
    /// Returns false if the entity isn't alive.
    pub fn insert<T: Component>(&mut self, entity: Entity, value: T) -> bool {
        let Some(location) = self.entities.location(entity) else {
            return false;
        };

        // Already there: replace in place, no move.
        if let Some(existing) = self.get_mut::<T>(entity) {
            *existing = value;
            return true;
        }

        let target = self.archetype_with_added::<T>(location.archetype);
        let [source, destination] = self
            .archetypes
            .get_disjoint_mut([location.archetype, target])
            .expect("adding a component always changes the archetype");

        // Move every existing component to the destination table.
        for (i, type_id) in source.types.iter().enumerate() {
            let to = destination
                .column_mut(*type_id)
                .expect("destination has every source type");
            source.columns[i].move_row_to(location.row, to);
        }
        // And add the new one.
        destination.push(value);

        self.finish_move(entity, location, target);
        true
    }

    /// Remove a component and return it. `None` if the entity isn't alive or doesn't have it.
    pub fn remove<T: Component>(&mut self, entity: Entity) -> Option<T> {
        let location = self.entities.location(entity)?;
        if self.archetypes[location.archetype]
            .column_index(TypeId::of::<T>())
            .is_none()
        {
            return None;
        }

        let target = self.archetype_with_removed::<T>(location.archetype);
        let [source, destination] = self
            .archetypes
            .get_disjoint_mut([location.archetype, target])
            .expect("removing a component always changes the archetype");

        let mut removed = None;
        for (i, type_id) in source.types.iter().enumerate() {
            if *type_id == TypeId::of::<T>() {
                // Take this one out instead of moving it.
                removed =
                    Some(typed_mut::<T>(source.columns[i].as_mut()).swap_remove(location.row));
            } else {
                let to = destination
                    .column_mut(*type_id)
                    .expect("destination has every kept type");
                source.columns[i].move_row_to(location.row, to);
            }
        }

        self.finish_move(entity, location, target);
        removed
    }

    /// After an entity's components moved from `from` to the end of archetype `to`:
    /// fix the entity lists and both affected locations.
    fn finish_move(&mut self, entity: Entity, from: Location, to: usize) {
        let moved = self.archetypes[from.archetype].remove_entity_row(from.row);
        if let Some(moved) = moved {
            self.entities.set_location(moved, from);
        }

        let destination = &mut self.archetypes[to];
        destination.entities.push(entity);
        let row = destination.len() - 1;
        self.entities
            .set_location(entity, Location { archetype: to, row });

        debug_assert!(
            destination
                .columns
                .iter()
                .all(|column| column.len() == destination.len()),
            "every column must have one value per entity"
        );
    }

    // ---------------------------------------------------------------------
    // Queries
    // ---------------------------------------------------------------------

    /// Iterate over every entity matching `Q`, like `(&mut Position, &Velocity)`.
    pub fn query<Q: QueryData>(&mut self) -> impl Iterator<Item = Q::Item<'_>> {
        self.query_filtered::<Q, ()>()
    }

    /// Like `query`, restricted by a filter, like `With<Player>` or `(With<Npc>, Without<Asleep>)`.
    pub fn query_filtered<Q: QueryData, F: QueryFilter>(
        &mut self,
    ) -> impl Iterator<Item = Q::Item<'_>> {
        let mut required = Vec::new();
        Q::required(&mut required);

        self.archetypes
            .iter_mut()
            .filter(move |archetype| {
                required
                    .iter()
                    .all(|type_id| archetype.column_index(*type_id).is_some())
                    && F::matches(&archetype.types)
            })
            .flat_map(|archetype| Q::fetch(&mut ArchetypeBorrow::new(archetype)))
    }

    // ---------------------------------------------------------------------
    // Archetypes
    // ---------------------------------------------------------------------

    /// Number of archetypes, for debugging and tests.
    pub fn archetype_count(&self) -> usize {
        self.archetypes.len()
    }

    /// Find or create the archetype for a bundle's component types.
    fn archetype_for<B: Bundle>(&mut self) -> usize {
        let mut types = B::type_ids();
        types.sort();
        if let Some(&index) = self.archetype_index.get(&types) {
            return index;
        }
        assert!(
            types.windows(2).all(|w| w[0] != w[1]),
            "a bundle contains the same component type twice"
        );

        // New archetype: create its columns, ordered like the sorted types.
        let mut pairs: Vec<_> = B::type_ids().into_iter().zip(B::new_columns()).collect();
        pairs.sort_by_key(|(type_id, _)| *type_id);
        let columns = pairs.into_iter().map(|(_, column)| column).collect();

        self.add_archetype(types, columns)
    }

    /// The archetype reached from `source` by adding `T`. Cached as an edge.
    fn archetype_with_added<T: Component>(&mut self, source: usize) -> usize {
        let added = TypeId::of::<T>();
        if let Some(&target) = self.archetypes[source].add_edges.get(&added) {
            return target;
        }

        let mut types = self.archetypes[source].types.clone();
        let position = types.binary_search(&added).unwrap_err();
        types.insert(position, added);

        let target = match self.archetype_index.get(&types) {
            Some(&index) => index,
            None => {
                let source_archetype = &self.archetypes[source];
                let columns: Vec<Box<dyn Column>> = types
                    .iter()
                    .map(|type_id| match source_archetype.column(*type_id) {
                        Some(column) => column.new_empty(),
                        None => Box::new(Vec::<T>::new()),
                    })
                    .collect();
                self.add_archetype(types, columns)
            }
        };

        self.archetypes[source].add_edges.insert(added, target);
        self.archetypes[target].remove_edges.insert(added, source);
        target
    }

    /// The archetype reached from `source` by removing `T`. Cached as an edge.
    fn archetype_with_removed<T: Component>(&mut self, source: usize) -> usize {
        let removed = TypeId::of::<T>();
        if let Some(&target) = self.archetypes[source].remove_edges.get(&removed) {
            return target;
        }

        let types: Vec<TypeId> = self.archetypes[source]
            .types
            .iter()
            .copied()
            .filter(|type_id| *type_id != removed)
            .collect();

        let target = match self.archetype_index.get(&types) {
            Some(&index) => index,
            None => {
                let source_archetype = &self.archetypes[source];
                let columns = types
                    .iter()
                    .map(|type_id| {
                        source_archetype
                            .column(*type_id)
                            .expect("kept type exists")
                            .new_empty()
                    })
                    .collect();
                self.add_archetype(types, columns)
            }
        };

        self.archetypes[source].remove_edges.insert(removed, target);
        self.archetypes[target].add_edges.insert(removed, source);
        target
    }

    fn add_archetype(&mut self, types: Vec<TypeId>, columns: Vec<Box<dyn Column>>) -> usize {
        let index = self.archetypes.len();
        self.archetypes.push(Archetype::new(types.clone(), columns));
        self.archetype_index.insert(types, index);
        index
    }

    // ---------------------------------------------------------------------
    // Resources
    // ---------------------------------------------------------------------

    /// Store a resource, returning the previous one of the same type, if any.
    pub fn insert_resource<R: Resource>(&mut self, value: R) -> Option<R> {
        self.resources.insert(value)
    }

    pub fn get_resource<R: Resource>(&self) -> Option<&R> {
        self.resources.get::<R>()
    }

    pub fn get_resource_mut<R: Resource>(&mut self) -> Option<&mut R> {
        self.resources.get_mut::<R>()
    }

    /// A resource that must exist. Panics with its type name if it doesn't.
    pub fn resource<R: Resource>(&self) -> &R {
        self.get_resource::<R>()
            .unwrap_or_else(|| panic!("resource `{}` doesn't exist", std::any::type_name::<R>()))
    }

    /// Mutable version of `resource`.
    pub fn resource_mut<R: Resource>(&mut self) -> &mut R {
        self.get_resource_mut::<R>()
            .unwrap_or_else(|| panic!("resource `{}` doesn't exist", std::any::type_name::<R>()))
    }

    pub fn remove_resource<R: Resource>(&mut self) -> Option<R> {
        self.resources.remove::<R>()
    }

    pub fn has_resource<R: Resource>(&self) -> bool {
        self.resources.contains::<R>()
    }

    /// Temporarily take a resource out, so it can be used together with the rest of the
    /// world (queries included). It's put back afterwards.
    ///
    /// While inside, the world doesn't have this resource; inserting another one of the
    /// same type is overwritten when the scope ends.
    pub fn resource_scope<R: Resource, T>(&mut self, f: impl FnOnce(&mut World, &mut R) -> T) -> T {
        let mut resource = self
            .remove_resource::<R>()
            .unwrap_or_else(|| panic!("resource `{}` doesn't exist", std::any::type_name::<R>()));
        let result = f(self, &mut resource);
        self.insert_resource(resource);
        result
    }
    // ---------------------------------------------------------------------
    // Commands
    // ---------------------------------------------------------------------

    /// A new, empty command buffer for this world.
    pub fn commands(&self) -> Commands {
        Commands::new(self.entities.id_counter())
    }

    /// Run every queued command, in the order they were queued.
    pub fn apply(&mut self, commands: Commands) {
        assert!(
            commands.belongs_to(&self.entities.id_counter()),
            "commands were created by a different world"
        );
        for command in commands.into_queue() {
            command(self);
        }
    }

    // ---------------------------------------------------------------------
    // Events
    // ---------------------------------------------------------------------

    /// Create the event queue for `E`, if it doesn't exist yet.
    pub fn init_events<E: Resource>(&mut self) {
        if !self.has_resource::<Events<E>>() {
            self.insert_resource(Events::<E>::default());
        }
    }

    /// Send an event. The queue must have been created with `init_events`.
    pub fn send_event<E: Resource>(&mut self, event: E) {
        self.resource_mut::<Events<E>>().send(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        EventCursor,
        query::{With, Without},
    };

    #[derive(Debug, PartialEq)]
    struct Position(f32, f32);
    #[derive(Debug, PartialEq)]
    struct Velocity(f32, f32);
    #[derive(Debug, PartialEq)]
    struct Health(u32);
    #[derive(Debug, PartialEq)]
    struct Name(&'static str);

    // --- spawn, get, despawn ---

    #[test]
    fn spawn_and_get() {
        let mut world = World::new();
        let e = world.spawn((Position(1.0, 2.0), Velocity(3.0, 4.0)));
        assert_eq!(world.get::<Position>(e), Some(&Position(1.0, 2.0)));
        assert_eq!(world.get::<Velocity>(e), Some(&Velocity(3.0, 4.0)));
        assert_eq!(world.get::<Name>(e), None);
        assert_eq!(world.len(), 1);
    }

    #[test]
    fn get_mut_changes_the_component() {
        let mut world = World::new();
        let e = world.spawn((Position(0.0, 0.0),));
        world.get_mut::<Position>(e).unwrap().0 = 5.0;
        assert_eq!(world.get::<Position>(e), Some(&Position(5.0, 0.0)));
    }

    #[test]
    fn same_types_share_an_archetype_in_any_order() {
        let mut world = World::new();
        world.spawn((Position(0.0, 0.0), Velocity(0.0, 0.0)));
        world.spawn((Velocity(1.0, 1.0), Position(1.0, 1.0)));
        world.spawn((Position(2.0, 2.0),));
        assert_eq!(world.archetype_count(), 2);
    }

    #[test]
    fn despawn_keeps_the_moved_entity_correct() {
        let mut world = World::new();
        let a = world.spawn((Name("a"),));
        let b = world.spawn((Name("b"),));
        let c = world.spawn((Name("c"),));

        // Removing `a` moves `c` (the last row) into `a`'s row.
        assert!(world.despawn(a));

        assert!(!world.is_alive(a));
        assert_eq!(world.get::<Name>(a), None);
        assert_eq!(world.get::<Name>(b), Some(&Name("b")));
        assert_eq!(
            world.get::<Name>(c),
            Some(&Name("c")),
            "c's location was updated"
        );
        assert_eq!(world.len(), 2);
    }

    #[test]
    fn despawning_twice_fails() {
        let mut world = World::new();
        let e = world.spawn(());
        assert!(world.despawn(e));
        assert!(!world.despawn(e));
    }

    #[test]
    #[should_panic(expected = "same component type twice")]
    fn duplicate_components_panic() {
        let mut world = World::new();
        world.spawn((Position(0.0, 0.0), Position(1.0, 1.0)));
    }

    #[test]
    fn components_are_dropped_on_despawn() {
        use std::sync::Arc;

        let shared = Arc::new(());
        let mut world = World::new();
        let e = world.spawn((Arc::clone(&shared),));
        assert_eq!(Arc::strong_count(&shared), 2);
        world.despawn(e);
        assert_eq!(
            Arc::strong_count(&shared),
            1,
            "the component's destructor ran"
        );
    }

    // --- insert and remove ---

    #[test]
    fn insert_adds_a_component_and_keeps_the_others() {
        let mut world = World::new();
        let e = world.spawn((Position(1.0, 2.0), Name("e")));
        assert!(world.insert(e, Health(10)));

        assert_eq!(world.get::<Health>(e), Some(&Health(10)));
        assert_eq!(world.get::<Position>(e), Some(&Position(1.0, 2.0)));
        assert_eq!(world.get::<Name>(e), Some(&Name("e")));
    }

    #[test]
    fn insert_replaces_an_existing_component() {
        let mut world = World::new();
        let e = world.spawn((Health(10),));
        let archetypes = world.archetype_count();
        world.insert(e, Health(3));
        assert_eq!(world.get::<Health>(e), Some(&Health(3)));
        assert_eq!(
            world.archetype_count(),
            archetypes,
            "no move, no new archetype"
        );
    }

    #[test]
    fn remove_returns_the_component_and_keeps_the_others() {
        let mut world = World::new();
        let e = world.spawn((Position(1.0, 2.0), Health(10)));
        assert_eq!(world.remove::<Health>(e), Some(Health(10)));
        assert_eq!(world.get::<Health>(e), None);
        assert_eq!(world.get::<Position>(e), Some(&Position(1.0, 2.0)));
        assert_eq!(world.remove::<Health>(e), None, "already removed");
    }

    #[test]
    fn moving_an_entity_keeps_its_old_neighbours_correct() {
        let mut world = World::new();
        let a = world.spawn((Name("a"),));
        let b = world.spawn((Name("b"),));

        // `a` leaves the table; `b` moves into its row.
        world.insert(a, Health(1));

        assert_eq!(world.get::<Name>(a), Some(&Name("a")));
        assert_eq!(world.get::<Name>(b), Some(&Name("b")));
        assert_eq!(world.get::<Health>(b), None);
    }

    #[test]
    fn transitions_reuse_archetypes() {
        let mut world = World::new();
        let e = world.spawn((Position(0.0, 0.0),));
        for _ in 0..10 {
            world.insert(e, Health(1));
            world.remove::<Health>(e);
        }
        assert_eq!(
            world.archetype_count(),
            2,
            "{{Position}} and {{Health, Position}}"
        );
        assert_eq!(world.get::<Position>(e), Some(&Position(0.0, 0.0)));
    }

    #[test]
    fn insert_and_remove_on_dead_entities_fail() {
        let mut world = World::new();
        let e = world.spawn(());
        world.despawn(e);
        assert!(!world.insert(e, Health(1)));
        assert_eq!(world.remove::<Health>(e), None);
    }

    #[test]
    fn components_can_be_built_up_one_by_one() {
        let mut world = World::new();
        let e = world.spawn(());
        world.insert(e, Position(1.0, 1.0));
        world.insert(e, Velocity(2.0, 2.0));
        world.insert(e, Name("built"));
        assert_eq!(world.get::<Velocity>(e), Some(&Velocity(2.0, 2.0)));
        assert_eq!(world.get::<Name>(e), Some(&Name("built")));
    }

    // --- queries ---

    #[test]
    fn query_reads_every_matching_entity() {
        let mut world = World::new();
        world.spawn((Position(1.0, 0.0), Velocity(0.0, 0.0)));
        world.spawn((Position(2.0, 0.0),));
        world.spawn((Velocity(5.0, 0.0),));

        let total: f32 = world.query::<&Position>().map(|p| p.0).sum();
        assert_eq!(
            total, 3.0,
            "both entities with a Position, across archetypes"
        );
    }

    #[test]
    fn query_writes_components() {
        let mut world = World::new();
        let a = world.spawn((Position(0.0, 0.0), Velocity(1.0, 2.0)));
        let b = world.spawn((Position(10.0, 10.0), Velocity(-1.0, 0.0), Health(5)));

        for (position, velocity) in world.query::<(&mut Position, &Velocity)>() {
            position.0 += velocity.0;
            position.1 += velocity.1;
        }

        assert_eq!(world.get::<Position>(a), Some(&Position(1.0, 2.0)));
        assert_eq!(world.get::<Position>(b), Some(&Position(9.0, 10.0)));
    }

    #[test]
    fn query_can_include_the_entity() {
        let mut world = World::new();
        let a = world.spawn((Name("a"),));
        let b = world.spawn((Name("b"), Health(1)));

        let mut found: Vec<(Entity, &str)> = world
            .query::<(Entity, &Name)>()
            .map(|(e, n)| (e, n.0))
            .collect();
        found.sort();
        assert_eq!(found, vec![(a, "a"), (b, "b")]);
    }

    #[test]
    fn filters_include_and_exclude() {
        let mut world = World::new();
        world.spawn((Name("healthy"), Health(10)));
        world.spawn((Name("plain"),));

        let with: Vec<&str> = world
            .query_filtered::<&Name, With<Health>>()
            .map(|n| n.0)
            .collect();
        assert_eq!(with, vec!["healthy"]);

        let without: Vec<&str> = world
            .query_filtered::<&Name, Without<Health>>()
            .map(|n| n.0)
            .collect();
        assert_eq!(without, vec!["plain"]);
    }

    #[test]
    fn queries_skip_despawned_entities() {
        let mut world = World::new();
        let a = world.spawn((Health(1),));
        world.spawn((Health(2),));
        world.despawn(a);
        assert_eq!(world.query::<&Health>().count(), 1);
    }

    #[test]
    #[should_panic(expected = "appears twice")]
    fn aliasing_the_same_component_panics() {
        let mut world = World::new();
        world.spawn((Position(0.0, 0.0),));
        world.query::<(&Position, &mut Position)>().count();
    }

    // --- resources ---

    #[derive(Debug, PartialEq)]
    struct Clock(u32);
    #[derive(Debug, PartialEq)]
    struct Score(u32);

    #[test]
    fn resources_are_stored_by_type() {
        let mut world = World::new();
        world.insert_resource(Clock(8));
        world.insert_resource(Score(0));

        assert_eq!(world.resource::<Clock>(), &Clock(8));
        assert_eq!(world.resource::<Score>(), &Score(0));
        assert!(world.get_resource::<Health>().is_none());
    }

    #[test]
    fn inserting_again_replaces_and_returns_the_old_value() {
        let mut world = World::new();
        assert_eq!(world.insert_resource(Clock(8)), None);
        assert_eq!(world.insert_resource(Clock(9)), Some(Clock(8)));
        assert_eq!(world.resource::<Clock>(), &Clock(9));
    }

    #[test]
    fn resources_can_be_changed_and_removed() {
        let mut world = World::new();
        world.insert_resource(Score(0));
        world.resource_mut::<Score>().0 += 10;
        assert_eq!(world.remove_resource::<Score>(), Some(Score(10)));
        assert!(!world.has_resource::<Score>());
    }

    #[test]
    #[should_panic(expected = "doesn't exist")]
    fn a_missing_required_resource_panics_with_its_name() {
        let world = World::new();
        world.resource::<Clock>();
    }

    #[test]
    fn resource_scope_allows_queries_alongside_a_resource() {
        let mut world = World::new();
        world.insert_resource(Score(0));
        world.spawn((Health(3),));
        world.spawn((Health(4),));

        // Sum every Health into the Score resource, during a query.
        world.resource_scope::<Score, _>(|world, score| {
            for health in world.query::<&Health>() {
                score.0 += health.0;
            }
        });

        assert_eq!(
            world.resource::<Score>(),
            &Score(7),
            "the resource was put back"
        );
    }

    // --- commands ---

    #[test]
    fn commands_change_the_world_during_a_query() {
        let mut world = World::new();
        world.spawn((Health(0),));
        world.spawn((Health(5),));

        // Despawn the dead, and spawn a replacement for each.
        let mut commands = world.commands();
        for (entity, health) in world.query::<(Entity, &Health)>() {
            if health.0 == 0 {
                commands.despawn(entity);
                commands.spawn((Name("replacement"),));
            }
        }
        world.apply(commands);

        assert_eq!(world.query::<&Health>().count(), 1);
        assert_eq!(world.query::<&Name>().count(), 1);
    }

    #[test]
    fn a_queued_spawn_can_be_used_before_it_exists() {
        let mut world = World::new();
        let mut commands = world.commands();
        let e = commands.spawn((Name("later"),));
        commands.insert(e, Health(3));

        assert!(!world.is_alive(e), "not applied yet");
        world.apply(commands);

        assert_eq!(world.get::<Name>(e), Some(&Name("later")));
        assert_eq!(world.get::<Health>(e), Some(&Health(3)));
    }

    #[test]
    fn reserved_ids_never_clash_with_direct_spawns() {
        let mut world = World::new();
        let mut commands = world.commands();
        let queued = commands.spawn(());
        let direct = world.spawn(());
        assert_ne!(queued, direct);
        world.apply(commands);
        assert!(world.is_alive(queued) && world.is_alive(direct));
    }

    #[test]
    #[should_panic(expected = "different world")]
    fn commands_from_another_world_are_rejected() {
        let other = World::new();
        let mut world = World::new();
        world.apply(other.commands());
    }

    // --- events ---

    #[derive(Debug, PartialEq)]
    struct DayStarted(u32);

    #[test]
    fn readers_see_each_event_once() {
        let mut world = World::new();
        world.init_events::<DayStarted>();
        let mut reader = EventCursor::<DayStarted>::default();

        world.send_event(DayStarted(1));
        world.send_event(DayStarted(2));
        let read: Vec<_> = reader
            .read(world.resource::<Events<DayStarted>>())
            .collect();
        assert_eq!(read, vec![&DayStarted(1), &DayStarted(2)]);

        let again: Vec<_> = reader
            .read(world.resource::<Events<DayStarted>>())
            .collect();
        assert!(again.is_empty(), "already read");
    }

    #[test]
    fn every_reader_sees_every_event() {
        let mut world = World::new();
        world.init_events::<DayStarted>();
        let mut a = EventCursor::<DayStarted>::default();
        let mut b = EventCursor::<DayStarted>::default();

        world.send_event(DayStarted(1));
        let events = world.resource::<Events<DayStarted>>();
        assert_eq!(a.read(events).count(), 1);
        assert_eq!(b.read(events).count(), 1);
    }

    #[test]
    fn events_live_for_two_updates() {
        let mut world = World::new();
        world.init_events::<DayStarted>();
        world.send_event(DayStarted(1));

        world.resource_mut::<Events<DayStarted>>().update();
        let mut late_reader = EventCursor::<DayStarted>::default();
        assert_eq!(
            late_reader
                .read(world.resource::<Events<DayStarted>>())
                .count(),
            1,
            "still there after one update"
        );

        world.resource_mut::<Events<DayStarted>>().update();
        let mut very_late_reader = EventCursor::<DayStarted>::default();
        assert_eq!(
            very_late_reader
                .read(world.resource::<Events<DayStarted>>())
                .count(),
            0,
            "gone after two"
        );
    }
}
