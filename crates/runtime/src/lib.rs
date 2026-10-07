mod action;
mod input;
mod runner;
mod time;
mod window;

pub use action::{ActionMap, Axis1DBinding, AxisBinding, Binding};
pub use asset;
pub use input::{ButtonState, Input, KeyCode, MouseButton};
pub use runner::run;
pub use time::Time;
pub use window::Window;

mod app_runner;
pub use app_runner::run_app;
pub use ecs;

/// How the game's window and display should be set up.
#[derive(Clone)]
pub struct Config {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub resizable: bool,
    pub fullscreen: bool,
    pub vsync: bool,
    pub assets_dir: std::path::PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            title: "game".into(),
            width: 1280,
            height: 720,
            resizable: true,
            fullscreen: false,
            vsync: true,
            assets_dir: std::path::PathBuf::from("assets"),
        }
    }
}

pub struct Context {
    pub gpu: gfx::GpuContext,
    pub input: Input,
    pub window: Window,
    pub time: Time,
    pub assets: asset::Assets,
    pub(crate) quit_requested: bool,
}

impl Context {
    /// Exit after the current frame.
    pub fn quit(&mut self) {
        self.quit_requested = true;
    }
}

pub trait Game: Sized + 'static {
    /// Window and display settings. Override to customize.
    fn config() -> Config {
        Config::default()
    }

    fn init(ctx: &mut Context) -> anyhow::Result<Self>;
    fn update(&mut self, ctx: &mut Context, dt: f32);
    fn render(&mut self, ctx: &mut Context, frame: &mut gfx::Frame);
}

/// Insert-free way for systems to end the game: `world.resource_mut::<Exit>().request()`.
#[derive(Default)]
pub struct Exit {
    requested: bool,
}

impl Exit {
    pub fn request(&mut self) {
        self.requested = true;
    }

    pub fn is_requested(&self) -> bool {
        self.requested
    }
}
