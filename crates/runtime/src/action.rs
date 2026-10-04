use std::collections::HashMap;
use std::hash::Hash;

use gfx::glam::Vec2;

use crate::input::{Input, KeyCode, MouseButton, Query};

/// A digital input that can trigger a button action.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Binding {
    Key(KeyCode),
    Mouse(MouseButton),
}

/// An input that produces a 2D direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AxisBinding {
    Keys {
        up: KeyCode,
        down: KeyCode,
        left: KeyCode,
        right: KeyCode,
    },
}

impl AxisBinding {
    pub const WASD: Self = Self::Keys {
        up: KeyCode::KeyW,
        down: KeyCode::KeyS,
        left: KeyCode::KeyA,
        right: KeyCode::KeyD,
    };

    pub const ARROWS: Self = Self::Keys {
        up: KeyCode::ArrowUp,
        down: KeyCode::ArrowDown,
        left: KeyCode::ArrowLeft,
        right: KeyCode::ArrowRight,
    };

    fn read(&self, input: &Input) -> Vec2 {
        match *self {
            Self::Keys {
                up,
                down,
                left,
                right,
            } => {
                let key = |k| if input.keys.held(k) { 1.0 } else { 0.0 };
                Vec2::new(key(right) - key(left), key(up) - key(down))
            }
        }
    }
}

/// Maps a game's own action enum to inputs.
pub struct ActionMap<A> {
    buttons: HashMap<A, Vec<Binding>>,
    axes: HashMap<A, Vec<AxisBinding>>,
    axes_1d: HashMap<A, Vec<Axis1DBinding>>,
}

impl<A> Default for ActionMap<A> {
    fn default() -> Self {
        Self {
            buttons: HashMap::new(),
            axes: HashMap::new(),
            axes_1d: HashMap::new(),
        }
    }
}

impl<A: Copy + Eq + Hash> ActionMap<A> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a button binding. An action can have any number of them.
    pub fn bind(mut self, action: A, binding: Binding) -> Self {
        self.buttons.entry(action).or_default().push(binding);
        self
    }

    /// Add an axis binding. Multiple bindings are summed.
    pub fn bind_axis(mut self, action: A, binding: AxisBinding) -> Self {
        self.axes.entry(action).or_default().push(binding);
        self
    }

    /// Add a 1D axis binding. Multiple bindings are summed.
    pub fn bind_axis_1d(mut self, action: A, binding: Axis1DBinding) -> Self {
        self.axes_1d.entry(action).or_default().push(binding);
        self
    }

    pub fn held(&self, input: &Input, action: A) -> bool {
        self.query(input, action, Query::Held)
    }

    pub fn pressed(&self, input: &Input, action: A) -> bool {
        self.query(input, action, Query::Pressed)
    }

    pub fn released(&self, input: &Input, action: A) -> bool {
        self.query(input, action, Query::Released)
    }

    /// A direction from all axis bindings combined, clamped to length 1.
    pub fn axis(&self, input: &Input, action: A) -> Vec2 {
        self.axes
            .get(&action)
            .into_iter()
            .flatten()
            .map(|b| b.read(input))
            .sum::<Vec2>()
            .clamp_length_max(1.0)
    }

    /// A value from -1 to 1 from all 1D bindings combined.
    pub fn axis_1d(&self, input: &Input, action: A) -> f32 {
        self.axes_1d
            .get(&action)
            .into_iter()
            .flatten()
            .map(|b| b.read(input))
            .sum::<f32>()
            .clamp(-1.0, 1.0)
    }

    fn query(&self, input: &Input, action: A, query: Query) -> bool {
        self.buttons
            .get(&action)
            .into_iter()
            .flatten()
            .any(|b| match *b {
                Binding::Key(k) => input.keys.query(k, query),
                Binding::Mouse(m) => input.mouse_buttons.query(m, query),
            })
    }
}

/// An input that produces a single value from -1 to 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Axis1DBinding {
    Keys {
        negative: KeyCode,
        positive: KeyCode,
    },
}

impl Axis1DBinding {
    fn read(&self, input: &Input) -> f32 {
        match *self {
            Self::Keys { negative, positive } => {
                let key = |k| if input.keys.held(k) { 1.0 } else { 0.0 };
                key(positive) - key(negative)
            }
        }
    }
}
