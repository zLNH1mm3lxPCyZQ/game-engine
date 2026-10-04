use glam::{Mat4, Vec3};

#[derive(Clone, Copy, Debug)]
pub enum Projection {
    /// Far things look smaller. `fov_y` is the vertical field of view in radians.
    Perspective { fov_y: f32, near: f32, far: f32 },
    /// No foreshortening. `height` is how many world units are visible vertically;
    /// the width follows from the aspect ratio.
    Orthographic { height: f32, near: f32, far: f32 },
}

impl Projection {
    pub fn matrix(&self, aspect: f32) -> Mat4 {
        match *self {
            Self::Perspective { fov_y, near, far } => {
                glam::camera::rh::proj::directx::perspective(fov_y, aspect, near, far)
            }
            Self::Orthographic { height, near, far } => {
                let half_h = height * 0.5;
                let half_w = half_h * aspect;
                glam::camera::rh::proj::directx::orthographic(
                    -half_w, half_w, -half_h, half_h, near, far,
                )
            }
        }
    }
}

impl Default for Projection {
    fn default() -> Self {
        Self::Perspective {
            fov_y: 60f32.to_radians(),
            near: 0.1,
            far: 100.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub position: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub projection: Projection,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            position: Vec3::new(0.0, -5.0, 0.0),
            target: Vec3::ZERO,
            up: Vec3::Z,
            projection: Projection::default(),
        }
    }
}

impl Camera {
    pub fn view(&self) -> Mat4 {
        glam::camera::rh::view::look_at_mat4(self.position, self.target, self.up)
    }

    pub fn projection(&self, aspect: f32) -> Mat4 {
        self.projection.matrix(aspect)
    }

    pub fn orthographic_2d(center: glam::Vec2, height: f32) -> Self {
        Self {
            position: center.extend(10.0),
            target: center.extend(0.0),
            up: Vec3::Y,
            projection: Projection::Orthographic {
                height,
                near: 0.1,
                far: 100.0,
            },
        }
    }
}

/// Everything a renderer needs to know about how the scene is being viewed this frame.
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub view: Mat4,
    pub projection: Mat4,
    /// Camera position in world space (needed for lighting, fog, sorting).
    pub position: Vec3,
    /// Size of the area being rendered to, in pixels.
    pub width: u32,
    pub height: u32,
}

impl View {
    pub fn new(camera: &Camera, width: u32, height: u32) -> Self {
        let aspect = width as f32 / height.max(1) as f32;
        Self {
            view: camera.view(),
            projection: camera.projection(aspect),
            position: camera.position,
            width,
            height,
        }
    }

    pub fn view_proj(&self) -> Mat4 {
        self.projection * self.view
    }

    pub fn pixels(width: u32, height: u32) -> Self {
        let projection = glam::camera::rh::proj::directx::orthographic(
            0.0,
            width as f32,
            0.0,
            height as f32,
            -1.0,
            1.0,
        );
        Self {
            view: Mat4::IDENTITY,
            projection,
            position: Vec3::ZERO,
            width,
            height,
        }
    }

    pub fn window_to_pixels(&self, window_pos: glam::Vec2) -> glam::Vec2 {
        glam::Vec2::new(window_pos.x, self.height as f32 - window_pos.y)
    }

    /// The world-space ray under a point in normalized device coordinates (-1 to 1, y up).
    /// Works for perspective and orthographic projections alike.
    pub fn ray(&self, ndc: glam::Vec2) -> math::Ray {
        let inverse = self.view_proj().inverse();
        let near = inverse.project_point3(ndc.extend(0.0));
        let far = inverse.project_point3(ndc.extend(1.0));
        math::Ray::new(near, far - near)
    }
}
