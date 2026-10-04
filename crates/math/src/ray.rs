use glam::Vec3;

/// A half-line: a start point and a direction (always normalized).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray {
    pub origin: Vec3,
    pub direction: Vec3,
}

impl Ray {
    pub fn new(origin: Vec3, direction: Vec3) -> Self {
        Self {
            origin,
            direction: direction.normalize(),
        }
    }

    /// The point at distance `t` along the ray.
    pub fn at(&self, t: f32) -> Vec3 {
        self.origin + self.direction * t
    }

    /// Distance to an infinite plane through `point` with the given `normal`.
    pub fn intersect_plane(&self, point: Vec3, normal: Vec3) -> Option<f32> {
        let facing = normal.dot(self.direction);
        if facing.abs() < 1e-6 {
            return None; // parallel to the plane
        }
        let t = (point - self.origin).dot(normal) / facing;
        (t >= 0.0).then_some(t)
    }

    /// Distance to the first hit on a sphere.
    pub fn intersect_sphere(&self, center: Vec3, radius: f32) -> Option<f32> {
        let to_origin = self.origin - center;
        let b = to_origin.dot(self.direction);
        let c = to_origin.length_squared() - radius * radius;
        let discriminant = b * b - c;
        if discriminant < 0.0 {
            return None; // the ray's line misses the sphere
        }
        let root = discriminant.sqrt();
        let near = -b - root;
        let far = -b + root;
        if near >= 0.0 {
            Some(near)
        } else if far >= 0.0 {
            Some(far) // the ray starts inside the sphere
        } else {
            None // the sphere is behind the ray
        }
    }

    /// Distance to the first hit on an axis-aligned box (the "slab" method).
    pub fn intersect_box(&self, min: Vec3, max: Vec3) -> Option<f32> {
        let inverse = self.direction.recip();
        let t1 = (min - self.origin) * inverse;
        let t2 = (max - self.origin) * inverse;
        let t_enter = t1.min(t2).max_element();
        let t_exit = t1.max(t2).min_element();
        if t_enter > t_exit || t_exit < 0.0 {
            None
        } else {
            Some(t_enter.max(0.0))
        }
    }
}
