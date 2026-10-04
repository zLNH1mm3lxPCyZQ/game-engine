use std::sync::Arc;
use std::time::Instant;

use anyhow::Context as _;
use gfx::glam::Vec2;
use tracing_subscriber::EnvFilter;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{DeviceEvent, DeviceId, ElementState, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::PhysicalKey,
    window::{Fullscreen, WindowAttributes, WindowId},
};

use crate::{Config, Context, Game, Input, Time, Window};

const STEP: f32 = 1.0 / 60.0;
const MAX_FRAME_TIME: f32 = 0.25;
/// Rough conversion from trackpad pixels to scroll "lines".
const PIXELS_PER_LINE: f32 = 20.0;

pub fn run<G: Game>() -> anyhow::Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .with_file(true)
        .with_line_number(true)
        .try_init();

    let event_loop = EventLoop::new()?;
    let mut runner = Runner::<G> {
        config: G::config(),
        window: None,
        ctx: None,
        game: None,
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

struct Runner<G: Game> {
    config: Config,
    window: Option<Arc<winit::window::Window>>,
    ctx: Option<Context>,
    game: Option<G>,
    focused: bool,
    last_time: Instant,
    accumulator: f32,
    error: Option<anyhow::Error>,
}

impl<G: Game> Runner<G> {
    fn init(&mut self, event_loop: &ActiveEventLoop) -> anyhow::Result<()> {
        let _span = tracing::info_span!("init").entered();

        let mut attributes = WindowAttributes::default()
            .with_title(&self.config.title)
            .with_inner_size(LogicalSize::new(self.config.width, self.config.height))
            .with_resizable(self.config.resizable);
        if self.config.fullscreen {
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
            self.config.vsync,
        ))
        .context("initializing GPU")?;

        let mut ctx = Context {
            gpu,
            input: Input::default(),
            window: Window::new(window.clone()),
            time: Time::default(),
            assets: asset::Assets::new(&self.config.assets_dir),
            quit_requested: false,
        };
        self.game = Some(G::init(&mut ctx).context("initializing game")?);
        self.ctx = Some(ctx);
        self.last_time = Instant::now();

        window.request_redraw();
        self.window = Some(window);
        Ok(())
    }

    fn frame(&mut self, event_loop: &ActiveEventLoop) {
        let (Some(ctx), Some(game)) = (&mut self.ctx, &mut self.game) else {
            return;
        };

        // Fixed-timestep updates
        let now = Instant::now();
        let raw_frame_time = (now - self.last_time).as_secs_f32();
        self.last_time = now;
        ctx.time.advance_frame(raw_frame_time);

        self.accumulator += raw_frame_time.min(MAX_FRAME_TIME);

        while self.accumulator >= STEP {
            game.update(ctx, STEP);
            ctx.input.end_tick();
            ctx.time.ticks += 1;
            self.accumulator -= STEP;
        }

        if ctx.quit_requested {
            event_loop.exit();
            return;
        }

        // Render
        let Some(mut frame) = ctx.gpu.begin_frame() else {
            return;
        };
        game.render(ctx, &mut frame);
        ctx.gpu.end_frame(frame);
    }
}

impl<G: Game> ApplicationHandler for Runner<G> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        if let Err(e) = self.init(event_loop) {
            self.error = Some(e);
            event_loop.exit();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let WindowEvent::RedrawRequested = event {
            self.frame(event_loop);
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            return;
        }

        let Some(ctx) = &mut self.ctx else { return };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => ctx.gpu.resize(size.width, size.height),
            WindowEvent::Focused(focused) => {
                self.focused = focused;
                if !focused {
                    ctx.input.release_all();
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(key) = event.physical_key {
                    match event.state {
                        ElementState::Pressed => ctx.input.keys.press(key),
                        ElementState::Released => ctx.input.keys.release(key),
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => match state {
                ElementState::Pressed => ctx.input.mouse_buttons.press(button),
                ElementState::Released => ctx.input.mouse_buttons.release(button),
            },
            WindowEvent::CursorMoved { position, .. } => {
                ctx.input.mouse_position = Vec2::new(position.x as f32, position.y as f32);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                ctx.input.scroll += match delta {
                    MouseScrollDelta::LineDelta(x, y) => Vec2::new(x, y),
                    MouseScrollDelta::PixelDelta(p) => {
                        Vec2::new(p.x as f32, p.y as f32) / PIXELS_PER_LINE
                    }
                };
            }
            _ => {}
        }
    }

    fn device_event(&mut self, _event_loop: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        if !self.focused {
            return;
        }
        if let (Some(ctx), DeviceEvent::MouseMotion { delta: (dx, dy) }) = (&mut self.ctx, event) {
            ctx.input.mouse_delta += Vec2::new(dx as f32, dy as f32);
        }
    }
}
