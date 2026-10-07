use glam::Vec2;

/// A rectangle of the render target to draw into, in pixels, from the top-left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Viewport {
    /// The whole target.
    pub fn full(width: u32, height: u32) -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: width.max(1) as f32,
            height: height.max(1) as f32,
        }
    }

    /// The largest centered rectangle with the given aspect ratio (width / height).
    pub fn fit(width: u32, height: u32, aspect: f32) -> Self {
        let (w, h) = (width.max(1) as f32, height.max(1) as f32);
        let (vw, vh) = if w / h > aspect {
            (h * aspect, h) // window too wide: bars left and right
        } else {
            (w, w / aspect) // window too tall: bars top and bottom
        };
        let (vw, vh) = (vw.round().max(1.0), vh.round().max(1.0));
        Self {
            x: ((w - vw) * 0.5).floor(),
            y: ((h - vh) * 0.5).floor(),
            width: vw,
            height: vh,
        }
    }

    /// The largest whole-number multiple of a virtual resolution that fits, centered.
    /// Falls back to `fit` if even 1x doesn't fit.
    pub fn fit_integer(width: u32, height: u32, virtual_width: u32, virtual_height: u32) -> Self {
        let scale = (width / virtual_width.max(1)).min(height / virtual_height.max(1));
        if scale == 0 {
            return Self::fit(
                width,
                height,
                virtual_width as f32 / virtual_height.max(1) as f32,
            );
        }
        let (vw, vh) = (virtual_width * scale, virtual_height * scale);
        Self {
            x: ((width - vw) / 2) as f32,
            y: ((height - vh) / 2) as f32,
            width: vw as f32,
            height: vh as f32,
        }
    }

    /// Size in whole pixels, for building a `View`.
    pub fn size(&self) -> (u32, u32) {
        (self.width as u32, self.height as u32)
    }

    /// Make all following draws in this pass go into this rectangle.
    pub fn apply(&self, pass: &mut crate::Pass) {
        pass.raw()
            .set_viewport(self.x, self.y, self.width, self.height, 0.0, 1.0);
    }

    /// Convert a window position (like the mouse: y down from the top-left)
    /// into this viewport's pixel coordinates (y up from its bottom-left).
    /// Returns `None` if the position is outside the viewport.
    pub fn window_to_local(&self, window_pos: Vec2) -> Option<Vec2> {
        let local = Vec2::new(window_pos.x - self.x, self.y + self.height - window_pos.y);
        let inside =
            local.x >= 0.0 && local.y >= 0.0 && local.x <= self.width && local.y <= self.height;
        inside.then_some(local)
    }

    /// Convert a window position (like the mouse: y down from the top-left) into
    /// normalized device coordinates within this viewport: -1 to 1, y up.
    /// Returns `None` if the position is outside the viewport.
    pub fn window_to_ndc(&self, window_pos: Vec2) -> Option<Vec2> {
        let local = self.window_to_local(window_pos)?;
        Some(local / Vec2::new(self.width, self.height) * 2.0 - 1.0)
    }
}
