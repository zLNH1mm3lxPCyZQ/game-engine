/// Timing information, updated by the runtime every frame.
#[derive(Default)]
pub struct Time {
    pub(crate) elapsed: f64,
    pub(crate) frame_delta: f32,
    pub(crate) frame_count: u64,
    pub(crate) ticks: u64,
    pub(crate) fps: f32,
    pub(crate) fixed_delta: f32,
    fps_frames: u32,
    fps_time: f32,
}

/// How often the FPS value refreshes, in seconds.
const FPS_WINDOW: f32 = 0.5;

impl Time {
    /// Real seconds since the game started.
    pub fn elapsed(&self) -> f64 {
        self.elapsed
    }

    /// Real duration of the last frame, in seconds.
    pub fn frame_delta(&self) -> f32 {
        self.frame_delta
    }

    /// Frames rendered since the game started.
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Fixed updates run since the game started.
    pub fn ticks(&self) -> u64 {
        self.ticks
    }

    /// Frames per second, averaged over a short window.
    pub fn fps(&self) -> f32 {
        self.fps
    }

    pub(crate) fn advance_frame(&mut self, delta: f32) {
        self.frame_delta = delta;
        self.elapsed += delta as f64;
        self.frame_count += 1;

        self.fps_frames += 1;
        self.fps_time += delta;
        if self.fps_time >= FPS_WINDOW {
            self.fps = self.fps_frames as f32 / self.fps_time;
            self.fps_frames = 0;
            self.fps_time = 0.0;
        }
    }

    /// The duration of one `FixedUpdate` tick, in seconds.
    pub(crate) fn with_fixed_delta(fixed_delta: f32) -> Self {
        Self {
            fixed_delta,
            ..Default::default()
        }
    }

    /// The duration of one `FixedUpdate` tick, in seconds.
pub fn fixed_delta(&self) -> f32 {
    self.fixed_delta
}
}
