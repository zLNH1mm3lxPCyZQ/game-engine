use gfx::glam::Vec3;

/// Parallel light from far away, like the sun.
#[derive(Clone, Copy, Debug)]
pub struct DirectionalLight {
    /// The direction the light travels (from the sun toward the ground).
    pub direction: Vec3,
    pub color: Vec3,
    pub intensity: f32,
}

/// Light from a point, fading with distance.
#[derive(Clone, Copy, Debug)]
pub struct PointLight {
    pub position: Vec3,
    pub color: Vec3,
    pub intensity: f32,
    /// Beyond this distance the light has no effect.
    pub range: f32,
}

/// Scene-wide lighting: one sun and a sky/ground ambient.
#[derive(Clone, Copy, Debug)]
pub struct Lighting {
    pub sun: DirectionalLight,
    /// Ambient light from above.
    pub sky_color: Vec3,
    /// Ambient light from below.
    pub ground_color: Vec3,
}

impl Default for Lighting {
    fn default() -> Self {
        Self {
            sun: DirectionalLight {
                direction: Vec3::new(-0.4, -0.6, -1.0).normalize(),
                color: Vec3::new(1.0, 0.96, 0.9),
                intensity: 3.0,
            },
            sky_color: Vec3::new(0.3, 0.38, 0.5),
            ground_color: Vec3::new(0.12, 0.1, 0.08),
        }
    }
}

/// Where the sun's shadows are computed.
#[derive(Clone, Copy, Debug)]
pub struct ShadowSettings {
    pub enabled: bool,
    /// Center of the shadowed region (usually near the player or camera target).
    pub center: Vec3,
    /// Half-size of the shadowed region, in world units. Smaller = sharper shadows.
    pub radius: f32,
}

impl Default for ShadowSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            center: Vec3::ZERO,
            radius: 10.0,
        }
    }
}
