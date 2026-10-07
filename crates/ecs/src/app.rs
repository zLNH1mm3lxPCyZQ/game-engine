use std::any::TypeId;
use std::collections::{HashMap, HashSet};

use crate::event::Events;
use crate::resource::Resource;
use crate::schedule::{Stage, System, SystemError, SystemOutput};
use crate::world::World;

/// Groups related setup: resources, events, systems.
/// Any `fn(&mut App)` is a plugin.
pub trait Plugin: 'static {
    fn build(&self, app: &mut App);
}

impl<F: Fn(&mut App) + 'static> Plugin for F {
    fn build(&self, app: &mut App) {
        self(app);
    }
}

/// A world plus the systems that run on it, stage by stage.
#[derive(Default)]
pub struct App {
    pub world: World,
    stages: HashMap<Stage, Vec<System>>,
    plugins: HashSet<TypeId>,
    started: bool,
}

impl App {
    pub fn new() -> Self {
        Self::default()
    }

    // ---------------------------------------------------------------------
    // Building
    // ---------------------------------------------------------------------

    /// Add a plugin. Adding the same plugin again does nothing.
    pub fn add_plugin<P: Plugin>(&mut self, plugin: P) -> &mut Self {
        if self.plugins.insert(TypeId::of::<P>()) {
            plugin.build(self);
        }
        self
    }

    /// Run `system` in `stage`, after the systems already added to it.
    /// A system returns `()` or a `Result<(), E>`.
    pub fn add_system<F, O>(&mut self, stage: Stage, system: F) -> &mut Self
    where
        F: FnMut(&mut World) -> O + Send + 'static,
        O: SystemOutput,
    {
        self.stages
            .entry(stage)
            .or_default()
            .push(System::new(system));
        self
    }

    pub fn insert_resource<R: Resource>(&mut self, value: R) -> &mut Self {
        self.world.insert_resource(value);
        self
    }

    /// Insert `R::default()`, unless the resource already exists.
    pub fn init_resource<R: Resource + Default>(&mut self) -> &mut Self {
        if !self.world.has_resource::<R>() {
            self.world.insert_resource(R::default());
        }
        self
    }

    /// Create the event queue for `E`, updated automatically at the start of each frame.
    pub fn add_event<E: Resource>(&mut self) -> &mut Self {
        if !self.world.has_resource::<Events<E>>() {
            self.world.init_events::<E>();
            self.add_system(Stage::First, |world: &mut World| {
                world.resource_mut::<Events<E>>().update();
            });
        }
        self
    }

    // ---------------------------------------------------------------------
    // Running
    // ---------------------------------------------------------------------

    /// Run every system of one stage, in the order they were added.
    /// Stops at the first system that returns an error.
    pub fn run_stage(&mut self, stage: Stage) -> Result<(), SystemError> {
        if let Some(systems) = self.stages.get_mut(&stage) {
            for system in systems {
                system.run(&mut self.world)?;
            }
        }
        Ok(())
    }

    /// Run `Startup` systems, once. Later calls do nothing.
    pub fn startup(&mut self) -> Result<(), SystemError> {
        if !self.started {
            self.started = true;
            self.run_stage(Stage::Startup)?;
        }
        Ok(())
    }

    /// One frame, with every stage once (FixedUpdate included). The runtime drives
    /// stages itself; this is for tests, tools, and headless use.
    pub fn update(&mut self) -> Result<(), SystemError> {
        self.startup()?;
        for stage in [
            Stage::First,
            Stage::FixedUpdate,
            Stage::Update,
            Stage::PostUpdate,
            Stage::Render,
            Stage::Last,
        ] {
            self.run_stage(stage)?;
        }
        Ok(())
    }

    /// Names of the systems in a stage, in run order. For debugging.
    pub fn system_names(&self, stage: Stage) -> Vec<&'static str> {
        self.stages
            .get(&stage)
            .map(|systems| systems.iter().map(|s| s.name).collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventCursor;

    #[derive(Default, Debug, PartialEq)]
    struct Log(Vec<&'static str>);

    fn log(world: &mut World, entry: &'static str) {
        world.resource_mut::<Log>().0.push(entry);
    }

    #[test]
    fn stages_run_in_order_and_systems_in_insertion_order() {
        let mut app = App::new();
        app.init_resource::<Log>()
            .add_system(Stage::Last, |w: &mut World| log(w, "last"))
            .add_system(Stage::Update, |w: &mut World| log(w, "update 1"))
            .add_system(Stage::First, |w: &mut World| log(w, "first"))
            .add_system(Stage::Update, |w: &mut World| log(w, "update 2"));

        app.update();

        assert_eq!(
            app.world.resource::<Log>().0,
            vec!["first", "update 1", "update 2", "last"]
        );
    }

    #[test]
    fn startup_runs_once() {
        let mut app = App::new();
        app.init_resource::<Log>()
            .add_system(Stage::Startup, |w: &mut World| log(w, "startup"));

        app.update();
        app.update();

        assert_eq!(app.world.resource::<Log>().0, vec!["startup"]);
    }

    #[test]
    fn plugins_are_functions_and_added_only_once() {
        fn counter_plugin(app: &mut App) {
            app.init_resource::<Log>()
                .add_system(Stage::Update, |w: &mut World| log(w, "counted"));
        }

        let mut app = App::new();
        app.add_plugin(counter_plugin).add_plugin(counter_plugin);
        app.update();

        assert_eq!(
            app.world.resource::<Log>().0,
            vec!["counted"],
            "the second add did nothing"
        );
    }

    #[test]
    fn plugins_can_be_structs_with_settings() {
        struct Greeting(&'static str);
        impl Plugin for Greeting {
            fn build(&self, app: &mut App) {
                let word = self.0;
                app.init_resource::<Log>()
                    .add_system(Stage::Update, move |w: &mut World| log(w, word));
            }
        }

        let mut app = App::new();
        app.add_plugin(Greeting("hello"));
        app.update();
        assert_eq!(app.world.resource::<Log>().0, vec!["hello"]);
    }

    #[test]
    fn systems_can_keep_state_between_runs() {
        let mut app = App::new();
        let mut runs = 0;
        app.init_resource::<Log>()
            .add_system(Stage::Update, move |w: &mut World| {
                runs += 1;
                if runs == 3 {
                    log(w, "third run");
                }
            });

        for _ in 0..3 {
            app.update();
        }
        assert_eq!(app.world.resource::<Log>().0, vec!["third run"]);
    }

    #[derive(Debug, PartialEq)]
    struct Ping(u32);

    #[test]
    fn events_flow_between_systems_and_expire() {
        let mut app = App::new();
        let mut cursor = EventCursor::<Ping>::default();

        app.init_resource::<Log>()
            .add_event::<Ping>()
            .add_system(Stage::Startup, |w: &mut World| w.send_event(Ping(1)))
            .add_system(Stage::Update, move |w: &mut World| {
                let count = cursor.read(w.resource::<Events<Ping>>()).count();
                if count > 0 {
                    log(w, "received");
                }
            });

        app.update(); // sent in Startup, received in Update
        app.update(); // already read: nothing new
        app.update();

        assert_eq!(app.world.resource::<Log>().0, vec!["received"]);
    }

    #[test]
    fn system_names_help_debugging() {
        fn advance_clock(_world: &mut World) {}

        let mut app = App::new();
        app.add_system(Stage::FixedUpdate, advance_clock);
        let names = app.system_names(Stage::FixedUpdate);
        assert!(names[0].ends_with("advance_clock"), "got {names:?}");
    }

    #[test]
    fn a_failing_system_stops_the_stage_and_names_itself() {
        fn broken(_world: &mut World) -> Result<(), std::io::Error> {
            Err(std::io::Error::other("disk on fire"))
        }

        let mut app = App::new();
        app.init_resource::<Log>()
            .add_system(Stage::Update, broken)
            .add_system(Stage::Update, |w: &mut World| log(w, "never runs"));

        let error = app.update().unwrap_err();
        assert!(error.system.ends_with("broken"));
        assert_eq!(error.source.to_string(), "disk on fire");
        assert!(app.world.resource::<Log>().0.is_empty());
    }
}
