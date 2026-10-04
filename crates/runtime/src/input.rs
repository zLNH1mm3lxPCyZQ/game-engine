use std::collections::HashSet;
use std::hash::Hash;

use gfx::glam::Vec2;

pub use winit::event::MouseButton;
pub use winit::keyboard::KeyCode;

#[derive(Clone, Copy, Debug)]
pub(crate) enum Query {
    Held,
    Pressed,
    Released,
}

/// Held / pressed / released tracking for any kind of button.
pub struct ButtonState<T> {
    held: HashSet<T>,
    pressed: HashSet<T>,
    released: HashSet<T>,
}

impl<T> Default for ButtonState<T> {
    fn default() -> Self {
        Self {
            held: HashSet::new(),
            pressed: HashSet::new(),
            released: HashSet::new(),
        }
    }
}

impl<T: Copy + Eq + Hash> ButtonState<T> {
    pub fn held(&self, button: T) -> bool {
        self.held.contains(&button)
    }

    pub fn pressed(&self, button: T) -> bool {
        self.pressed.contains(&button)
    }

    pub fn released(&self, button: T) -> bool {
        self.released.contains(&button)
    }

    pub(crate) fn query(&self, button: T, query: Query) -> bool {
        match query {
            Query::Held => self.held(button),
            Query::Pressed => self.pressed(button),
            Query::Released => self.released(button),
        }
    }

    pub(crate) fn press(&mut self, button: T) {
        if self.held.insert(button) {
            self.pressed.insert(button);
        }
    }

    pub(crate) fn release(&mut self, button: T) {
        if self.held.remove(&button) {
            self.released.insert(button);
        }
    }

    pub(crate) fn release_all(&mut self) {
        self.released.extend(self.held.drain());
    }

    pub(crate) fn end_tick(&mut self) {
        self.pressed.clear();
        self.released.clear();
    }
}

#[derive(Default)]
pub struct Input {
    pub keys: ButtonState<KeyCode>,
    pub mouse_buttons: ButtonState<MouseButton>,
    pub(crate) mouse_position: Vec2,
    pub(crate) mouse_delta: Vec2,
    pub(crate) scroll: Vec2,
}

impl Input {
    /// Cursor position in physical pixels, from the window's top-left corner.
    pub fn mouse_position(&self) -> Vec2 {
        self.mouse_position
    }

    /// Raw mouse movement since the last update. Works while the cursor is locked.
    pub fn mouse_delta(&self) -> Vec2 {
        self.mouse_delta
    }

    /// Scroll since the last update, in lines (positive y = scrolled up).
    pub fn scroll(&self) -> Vec2 {
        self.scroll
    }

    pub(crate) fn release_all(&mut self) {
        self.keys.release_all();
        self.mouse_buttons.release_all();
    }

    pub(crate) fn end_tick(&mut self) {
        self.keys.end_tick();
        self.mouse_buttons.end_tick();
        self.mouse_delta = Vec2::ZERO;
        self.scroll = Vec2::ZERO;
    }
}
