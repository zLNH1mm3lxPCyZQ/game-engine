//! Shared helpers for the examples.

use gfx::glam::Vec3;
use runtime::{ActionMap, AxisBinding, Binding, Context, KeyCode, MouseButton};

/// Default config: the given title, and this crate's assets folder.
pub fn config(title: &str) -> runtime::Config {
    runtime::Config {
        title: title.into(),
        assets_dir: concat!(env!("CARGO_MANIFEST_DIR"), "/assets").into(),
        ..Default::default()
    }
}

/// Keys every example shares: Q quits, F11 toggles fullscreen.
pub fn standard_keys(ctx: &mut Context) {
    if ctx.input.keys.pressed(KeyCode::KeyQ) {
        ctx.quit();
    }
    if ctx.input.keys.pressed(KeyCode::F11) {
        ctx.window.set_fullscreen(!ctx.window.is_fullscreen());
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum OrbitAction {
    Orbit,
    Capture,
    Release,
    ToggleProjection,
}

/// A camera orbiting a target: mouse (after a click captures it), WASD/arrows, scroll to zoom,
/// P for orthographic, Escape to release the mouse.
pub struct OrbitCamera {
    pub target: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub orthographic: bool,
    actions: ActionMap<OrbitAction>,
}

impl OrbitCamera {
    pub fn new(target: Vec3, distance: f32) -> Self {
        let actions = ActionMap::new()
            .bind_axis(OrbitAction::Orbit, AxisBinding::WASD)
            .bind_axis(OrbitAction::Orbit, AxisBinding::ARROWS)
            .bind(OrbitAction::Capture, Binding::Mouse(MouseButton::Left))
            .bind(OrbitAction::Release, Binding::Key(KeyCode::Escape))
            .bind(OrbitAction::ToggleProjection, Binding::Key(KeyCode::KeyP));
        Self {
            target,
            yaw: 0.0,
            pitch: 0.4,
            distance,
            orthographic: false,
            actions,
        }
    }

    pub fn update(&mut self, ctx: &mut Context, dt: f32) {
        let input = &ctx.input;
        let orbit = self.actions.axis(input, OrbitAction::Orbit);
        let capture = self.actions.pressed(input, OrbitAction::Capture);
        let release = self.actions.pressed(input, OrbitAction::Release);
        let toggle = self.actions.pressed(input, OrbitAction::ToggleProjection);
        let mouse = input.mouse_delta();
        let scroll = input.scroll().y;

        if capture {
            ctx.window.set_cursor_locked(true);
        }
        if release {
            ctx.window.set_cursor_locked(false);
        }
        if toggle {
            self.orthographic = !self.orthographic;
        }

        self.yaw -= mouse.x * 0.005 + orbit.x * 2.0 * dt;
        self.pitch = (self.pitch + mouse.y * 0.005 - orbit.y * 2.0 * dt).clamp(-1.5, 1.5);
        self.distance = (self.distance - scroll * 0.3).clamp(1.0, 50.0);
    }

    pub fn camera(&self) -> gfx::Camera {
        let offset = Vec3::new(
            self.pitch.cos() * self.yaw.sin(),
            -self.pitch.cos() * self.yaw.cos(),
            self.pitch.sin(),
        ) * self.distance;

        let projection = if self.orthographic {
            gfx::Projection::Orthographic {
                height: self.distance * 0.8,
                near: 0.1,
                far: 200.0,
            }
        } else {
            gfx::Projection::default()
        };

        gfx::Camera {
            position: self.target + offset,
            target: self.target,
            up: Vec3::Z,
            projection,
        }
    }
}
