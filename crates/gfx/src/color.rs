use glam::Vec4;

/// A linear RGBA color, the space shaders and lighting work in.
///
/// Values copied from a color picker (paint programs, CSS) are sRGB:
/// use `from_srgb` for those, or they'll look washed out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const BLACK: Self = Self::rgb(0.0, 0.0, 0.0);
    pub const WHITE: Self = Self::rgb(1.0, 1.0, 1.0);
    pub const TRANSPARENT: Self = Self::rgba(0.0, 0.0, 0.0, 0.0);

    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// From sRGB components (0 to 1), as shown in color pickers.
    pub fn from_srgb(r: f32, g: f32, b: f32) -> Self {
        fn to_linear(c: f32) -> f32 {
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        }
        Self::rgb(to_linear(r), to_linear(g), to_linear(b))
    }

    pub fn to_vec4(self) -> Vec4 {
        Vec4::new(self.r, self.g, self.b, self.a)
    }
}

impl From<Vec4> for Color {
    fn from(v: Vec4) -> Self {
        Self::rgba(v.x, v.y, v.z, v.w)
    }
}

impl From<Color> for wgpu::Color {
    fn from(c: Color) -> Self {
        Self {
            r: c.r as f64,
            g: c.g as f64,
            b: c.b as f64,
            a: c.a as f64,
        }
    }
}
