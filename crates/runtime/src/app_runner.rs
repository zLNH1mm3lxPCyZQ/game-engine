use std::sync::Arc;
use std::time::Instant;

use anyhow::Context as _;
use ecs::{App, Stage};
use tracing_subscriber::EnvFilter;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{DeviceEvent, DeviceId, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Fullscreen, WindowAttributes, WindowId},
};

use crate::{Config, Exit, Input, Time, Window};

const STEP: f32 = 1.0 / 60.0;
const MAX_FRAME_TIME: f32 = 0.25;

/// Open a window and run `app`: Startup once, then every frame:
/// First, FixedUpdate (at 60 Hz, 0 or more times), Update, PostUpdate, Render, Last.
///
/// Window settings come from a `Config` resource, if the app has one.
pub fn run_app(app: App) -> anyhow::Result<()> {
    let debug = cfg!(debug_assertions);
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .with_file(debug)
        .with_line_number(debug)
        .try_init();

    let event_loop = EventLoop::new()?;
    let mut runner = AppRunner {
        app,
        window: None,
        tick_input: Input::default(),
        focused: true,
        last_time: Instant::now(),
        accumulator: 0.0,
        error: None,
    };
    event_loop.run_app(&mut runner)?;

    match runner.error.take() {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

struct AppRunner {
    app: App,
    window: Option<Arc<winit::window::Window>>,
    /// The `Input` seen by FixedUpdate: cleared after every tick.
    /// The world's own `Input` resource is the per-frame one.
    tick_input: Input,
    focused: bool,
    last_time: Instant,
    accumulator: f32,
    error: Option<anyhow::Error>,
}

impl AppRunner {
    fn init(&mut self, event_loop: &ActiveEventLoop) -> anyhow::Result<()> {
        let _span = tracing::info_span!("init").entered();
        let config = self
            .app
            .world
            .get_resource::<Config>()
            .cloned()
            .unwrap_or_default();

        let mut attributes = WindowAttributes::default()
            .with_title(&config.title)
            .with_inner_size(LogicalSize::new(config.width, config.height))
            .with_resizable(config.resizable);
        if config.fullscreen {
            attributes = attributes.with_fullscreen(Some(Fullscreen::Borderless(None)));
        }
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .context("creating window")?,
        );

        let size = window.inner_size();
        let gpu = pollster::block_on(gfx::GpuContext::new(
            window.clone(),
            size.width,
            size.height,
            config.vsync,
        ))
        .context("initializing GPU")?;

        let world = &mut self.app.world;
        world.insert_resource(gpu);
        world.insert_resource(Window::new(window.clone()));
        world.insert_resource(Input::default());
        world.insert_resource(Time::with_fixed_delta(STEP));
        world.insert_resource(asset::Assets::new(config.assets_dir.clone()));
        world.insert_resource(Exit::default());
        world.insert_resource(config);

        self.app.startup().context("starting the game")?;
        self.last_time = Instant::now();

        window.request_redraw();
        self.window = Some(window);
        Ok(())
    }

    fn frame(&mut self, event_loop: &ActiveEventLoop) -> anyhow::Result<()> {
        // Time
        let now = Instant::now();
        let raw_frame_time = (now - self.last_time).as_secs_f32();
        self.last_time = now;
        self.app
            .world
            .resource_mut::<Time>()
            .advance_frame(raw_frame_time);
        self.accumulator += raw_frame_time.min(MAX_FRAME_TIME);

        self.app.run_stage(Stage::First)?;

        // Fixed ticks, each seeing the tick version of Input.
        while self.accumulator >= STEP {
            let frame_input = self
                .app
                .world
                .insert_resource(std::mem::take(&mut self.tick_input))
                .expect("Input resource exists");
            self.app.run_stage(Stage::FixedUpdate)?;
            self.tick_input = self
                .app
                .world
                .insert_resource(frame_input)
                .expect("Input resource exists");
            self.tick_input.end_tick();

            self.app.world.resource_mut::<Time>().ticks += 1;
            self.accumulator -= STEP;
        }

        self.app.run_stage(Stage::Update)?;
        self.app.run_stage(Stage::PostUpdate)?;

        if self.app.world.resource::<Exit>().is_requested() {
            event_loop.exit();
            return Ok(());
        }

        // Render, with the frame available as a resource.
        let frame = self
            .app
            .world
            .resource_mut::<gfx::GpuContext>()
            .begin_frame();
        if let Some(frame) = frame {
            self.app.world.insert_resource(frame);
            self.app.run_stage(Stage::Render)?;
            let frame = self
                .app
                .world
                .remove_resource::<gfx::Frame>()
                .expect("Render systems must not remove the Frame");
            self.app
                .world
                .resource::<gfx::GpuContext>()
                .end_frame(frame);
        }

        self.app.run_stage(Stage::Last)?;

        // The frame version of Input is cleared once per frame.
        self.app.world.resource_mut::<Input>().end_tick();
        Ok(())
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, error: anyhow::Error) {
        self.error = Some(error);
        event_loop.exit();
    }
}

impl ApplicationHandler for AppRunner {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        if let Err(e) = self.init(event_loop) {
            self.fail(event_loop, e);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match &event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
                return;
            }
            WindowEvent::RedrawRequested => {
                if let Err(e) = self.frame(event_loop) {
                    self.fail(event_loop, e);
                    return;
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
                return;
            }
            WindowEvent::Resized(size) => {
                if let Some(gpu) = self.app.world.get_resource_mut::<gfx::GpuContext>() {
                    gpu.resize(size.width, size.height);
                }
            }
            WindowEvent::Focused(focused) => self.focused = *focused,
            _ => {}
        }

        // Feed input events to both versions of Input.
        self.tick_input.handle_window_event(&event);
        if let Some(input) = self.app.world.get_resource_mut::<Input>() {
            input.handle_window_event(&event);
        }
    }

    fn device_event(&mut self, _event_loop: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        if !self.focused {
            return;
        }
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            self.tick_input.handle_mouse_motion(dx, dy);
            if let Some(input) = self.app.world.get_resource_mut::<Input>() {
                input.handle_mouse_motion(dx, dy);
            }
        }
    }
}
