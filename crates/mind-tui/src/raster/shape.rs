//! Resolution-independent normalized vector shapes for the half-block polygon engine.
//!
//! Coordinates live in `0.0..=1.0` so a single vector asset scales cleanly from 80×24
//! (`80×48` subpixels) to 200×60 (`200×120` subpixels).

use super::pixel::{PixelBuffer, Rgb};

#[derive(Debug, Clone, Copy)]
pub struct PolyShape {
    pub points: &'static [(f32, f32)],
    pub color: Rgb,
    pub alpha: f32,
}

/// Draw a normalized polygon (`0.0..=1.0`) mapped into pixel buffer coordinates with optional
/// pan and zoom around the canvas center.
pub fn draw_poly_norm(
    buf: &mut PixelBuffer,
    points: &[(f32, f32)],
    color: Rgb,
    alpha: f32,
    pan_x: f32,
    pan_y: f32,
    zoom: f32,
) {
    if points.len() < 3 {
        return;
    }
    let w = buf.width as f32;
    let h = buf.height as f32;
    let mut scaled = Vec::with_capacity(points.len());
    for &(nx, ny) in points {
        let sx = ((nx - 0.5) * zoom + 0.5 + pan_x) * w;
        let sy = ((ny - 0.5) * zoom + 0.5 + pan_y) * h;
        scaled.push((sx, sy));
    }
    buf.fill_polygon(&scaled, color, alpha);
}

pub fn draw_shape_list(
    buf: &mut PixelBuffer,
    shapes: &[PolyShape],
    pan_x: f32,
    pan_y: f32,
    zoom: f32,
) {
    for shape in shapes {
        draw_poly_norm(
            buf,
            shape.points,
            shape.color,
            shape.alpha,
            pan_x,
            pan_y,
            zoom,
        );
    }
}
