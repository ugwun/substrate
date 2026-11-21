//! Deterministic camera easing and keyframe utilities for multi-shot vector cinematics.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub pan_x: f32,
    pub pan_y: f32,
    pub zoom: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            pan_x: 0.0,
            pan_y: 0.0,
            zoom: 1.0,
        }
    }
}

impl Camera {
    pub fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            pan_x: self.pan_x + (other.pan_x - self.pan_x) * t,
            pan_y: self.pan_y + (other.pan_y - self.pan_y) * t,
            zoom: self.zoom + (other.zoom - self.zoom) * t,
        }
    }
}

#[inline]
pub fn ease_in_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[inline]
pub fn ease_out_cubic(t: f32) -> f32 {
    let inv = 1.0 - t.clamp(0.0, 1.0);
    1.0 - inv * inv * inv
}
