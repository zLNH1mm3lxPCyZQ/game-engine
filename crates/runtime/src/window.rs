use std::sync::Arc;

use winit::window::{CursorGrabMode, Fullscreen};

/// The game's view of its window.
pub struct Window {
    raw: Arc<winit::window::Window>,
}

impl Window {
    pub(crate) fn new(raw: Arc<winit::window::Window>) -> Self {
        Self { raw }
    }

    /// Size in physical pixels.
    pub fn size(&self) -> (u32, u32) {
        let size = self.raw.inner_size();
        (size.width, size.height)
    }

    pub fn set_title(&self, title: &str) {
        self.raw.set_title(title);
    }

    pub fn is_fullscreen(&self) -> bool {
        self.raw.fullscreen().is_some()
    }

    pub fn set_fullscreen(&self, fullscreen: bool) {
        self.raw
            .set_fullscreen(fullscreen.then_some(Fullscreen::Borderless(None)));
    }

    /// Hide the cursor and keep it in the window. Use `Input::mouse_delta` for movement.
    pub fn set_cursor_locked(&self, locked: bool) {
        if locked {
            let result = self
                .raw
                .set_cursor_grab(CursorGrabMode::Locked)
                .or_else(|_| self.raw.set_cursor_grab(CursorGrabMode::Confined));
            if let Err(e) = result {
                tracing::warn!("could not lock cursor: {e}");
            }
            self.raw.set_cursor_visible(false);
        } else {
            let _ = self.raw.set_cursor_grab(CursorGrabMode::None);
            self.raw.set_cursor_visible(true);
        }
    }
}
