//! A small archetype-based entity component system.

mod archetype;
mod bundle;
mod column;
mod commands;
mod entity;
mod event;
mod query;
mod resource;
mod world;

pub use bundle::Bundle;
pub use column::Component;
pub use commands::Commands;
pub use entity::Entity;
pub use event::{EventCursor, Events};
pub use query::{QueryData, QueryFilter, With, Without};
pub use resource::Resource;
pub use world::World;

mod app;
mod schedule;

pub use app::{App, Plugin};
pub use schedule::Stage;

pub use schedule::{BoxError, SystemError, SystemOutput};
