use std::error::Error;
use std::fmt;

use crate::world::World;

/// When a group of systems runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Stage {
    /// Once, before the first frame.
    Startup,
    /// Each frame, first: event queues, input.
    First,
    /// At a fixed rate (the runtime decides how many times per frame): gameplay, physics.
    FixedUpdate,
    /// Each frame: per-frame logic, animation, cameras.
    Update,
    /// Each frame, after `Update`: hierarchies, cleanup.
    PostUpdate,
    /// Each frame: drawing.
    Render,
    /// Each frame, last: end-of-frame bookkeeping.
    Last,
}

/// Any error a system can return.
pub type BoxError = Box<dyn Error + Send + Sync>;

/// What a system may return: nothing, or a `Result`.
pub trait SystemOutput {
    fn into_result(self) -> Result<(), BoxError>;
}

impl SystemOutput for () {
    fn into_result(self) -> Result<(), BoxError> {
        Ok(())
    }
}

impl<E: Into<BoxError>> SystemOutput for Result<(), E> {
    fn into_result(self) -> Result<(), BoxError> {
        self.map_err(Into::into)
    }
}

/// A system that returned an error.
#[derive(Debug)]
pub struct SystemError {
    pub system: &'static str,
    pub source: BoxError,
}

impl fmt::Display for SystemError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "system `{}` failed", self.system)
    }
}

impl Error for SystemError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.source.as_ref())
    }
}

/// A function run against the world, with a name for debugging.
pub(crate) struct System {
    pub name: &'static str,
    run: Box<dyn FnMut(&mut World) -> Result<(), BoxError> + Send>,
}

impl System {
    pub fn new<F, O>(mut function: F) -> Self
    where
        F: FnMut(&mut World) -> O + Send + 'static,
        O: SystemOutput,
    {
        Self {
            name: std::any::type_name::<F>(),
            run: Box::new(move |world| function(world).into_result()),
        }
    }

    pub fn run(&mut self, world: &mut World) -> Result<(), SystemError> {
        (self.run)(world).map_err(|source| SystemError {
            system: self.name,
            source,
        })
    }
}
