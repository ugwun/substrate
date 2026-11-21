//! Core 2D RGB pixel buffer with scanline filled polygons, Bayer-dithered gradients,
//! atmospheric glows, vector lines, and procedural starfields.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub const BLACK: Self = Self(0, 0, 0);

    #[inline]
    pub fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let inv = 1.0 - t;
        Self(
            (f32::from(self.0) * inv + f32::from(other.0) * t).round() as u8,
            (f32::from(self.1) * inv + f32::from(other.1) * t).round() as u8,
            (f32::from(self.2) * inv + f32::from(other.2) * t).round() as u8,
        )
    }

    #[inline]
    pub fn blend(dst: Self, src: Self, alpha: f32) -> Self {
        if alpha >= 0.999 {
            return src;
        }
        if alpha <= 0.001 {
            return dst;
        }
        dst.lerp(src, alpha)
    }

    #[inline]
    pub fn add_clamped(self, other: Self, strength: f32) -> Self {
        let s = strength.clamp(0.0, 1.0);
        Self(
            (f32::from(self.0) + f32::from(other.0) * s).clamp(0.0, 255.0) as u8,
            (f32::from(self.1) + f32::from(other.1) * s).clamp(0.0, 255.0) as u8,
            (f32::from(self.2) + f32::from(other.2) * s).clamp(0.0, 255.0) as u8,
        )
    }

    #[inline]
    pub fn scale(self, factor: f32) -> Self {
        Self(
            (f32::from(self.0) * factor).clamp(0.0, 255.0) as u8,
            (f32::from(self.1) * factor).clamp(0.0, 255.0) as u8,
            (f32::from(self.2) * factor).clamp(0.0, 255.0) as u8,
        )
    }

    #[inline]
    pub fn luminance(self) -> f32 {
        (0.2126 * f32::from(self.0) + 0.7152 * f32::from(self.1) + 0.0722 * f32::from(self.2))
            / 255.0
    }
}

const BAYER4: [[f32; 4]; 4] = [
    [0.0 / 16.0, 8.0 / 16.0, 2.0 / 16.0, 10.0 / 16.0],
    [12.0 / 16.0, 4.0 / 16.0, 14.0 / 16.0, 6.0 / 16.0],
    [3.0 / 16.0, 11.0 / 16.0, 1.0 / 16.0, 9.0 / 16.0],
    [15.0 / 16.0, 7.0 / 16.0, 13.0 / 16.0, 5.0 / 16.0],
];

#[inline]
pub fn bayer_threshold(x: usize, y: usize) -> f32 {
    BAYER4[y & 3][x & 3]
}

#[derive(Debug, Clone)]
pub struct PixelBuffer {
    pub width: usize,
    pub height: usize,
    pixels: Vec<Rgb>,
}

impl PixelBuffer {
    pub fn new(width: usize, height: usize, bg: Rgb) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        Self {
            width,
            height,
            pixels: vec![bg; width * height],
        }
    }

    pub fn clear(&mut self, color: Rgb) {
        self.pixels.fill(color);
    }

    #[inline]
    pub fn get(&self, x: usize, y: usize) -> Rgb {
        if x < self.width && y < self.height {
            self.pixels[y * self.width + x]
        } else {
            Rgb::BLACK
        }
    }

    #[inline]
    pub fn set(&mut self, x: i32, y: i32, color: Rgb, alpha: f32) {
        if x < 0 || y < 0 {
            return;
        }
        let (ux, uy) = (x as usize, y as usize);
        if ux >= self.width || uy >= self.height {
            return;
        }
        let idx = uy * self.width + ux;
        self.pixels[idx] = Rgb::blend(self.pixels[idx], color, alpha);
    }

    #[inline]
    pub fn add_glow(&mut self, x: i32, y: i32, color: Rgb, strength: f32) {
        if x < 0 || y < 0 {
            return;
        }
        let (ux, uy) = (x as usize, y as usize);
        if ux >= self.width || uy >= self.height {
            return;
        }
        let idx = uy * self.width + ux;
        self.pixels[idx] = self.pixels[idx].add_clamped(color, strength);
    }

    /// Vertical gradient with optional 4×4 Bayer ordered dithering to eliminate banding.
    pub fn fill_vertical_gradient(&mut self, top: Rgb, bottom: Rgb, dither: bool) {
        let denom = self.height.saturating_sub(1).max(1) as f32;
        for y in 0..self.height {
            let base_t = y as f32 / denom;
            for x in 0..self.width {
                let t = if dither {
                    (base_t + (bayer_threshold(x, y) - 0.5) * 0.045).clamp(0.0, 1.0)
                } else {
                    base_t
                };
                self.pixels[y * self.width + x] = top.lerp(bottom, t);
            }
        }
    }

    /// Multi-stop vertical gradient for atmospheric sky and horizon transitions.
    pub fn fill_gradient_stops(&mut self, stops: &[(f32, Rgb)], dither: bool) {
        if stops.is_empty() {
            return;
        }
        if stops.len() == 1 {
            self.clear(stops[0].1);
            return;
        }
        let denom = self.height.saturating_sub(1).max(1) as f32;
        for y in 0..self.height {
            let base_t = y as f32 / denom;
            for x in 0..self.width {
                let t = if dither {
                    (base_t + (bayer_threshold(x, y) - 0.5) * 0.035).clamp(0.0, 1.0)
                } else {
                    base_t
                };
                let mut color = stops[0].1;
                for window in stops.windows(2) {
                    let (t0, c0) = window[0];
                    let (t1, c1) = window[1];
                    if t >= t0 && t <= t1 {
                        let span = (t1 - t0).max(0.0001);
                        color = c0.lerp(c1, (t - t0) / span);
                        break;
                    } else if t > t1 {
                        color = c1;
                    }
                }
                self.pixels[y * self.width + x] = color;
            }
        }
    }

    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: Rgb, alpha: f32) {
        if w <= 0 || h <= 0 {
            return;
        }
        let x0 = x.max(0) as usize;
        let y0 = y.max(0) as usize;
        let x1 = (x + w).min(self.width as i32).max(0) as usize;
        let y1 = (y + h).min(self.height as i32).max(0) as usize;
        for py in y0..y1 {
            let row = py * self.width;
            for px in x0..x1 {
                self.pixels[row + px] = Rgb::blend(self.pixels[row + px], color, alpha);
            }
        }
    }

    /// Clipped scanline polygon fill using the even-odd rule.
    pub fn fill_polygon(&mut self, points: &[(f32, f32)], color: Rgb, alpha: f32) {
        if points.len() < 3 || alpha <= 0.001 {
            return;
        }
        let mut min_y = f32::INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for &(_, y) in points {
            min_y = min_y.min(y);
            max_y = max_y.max(y);
        }
        let y_start = (min_y.floor() as i32).max(0);
        let y_end = (max_y.ceil() as i32).min(self.height as i32 - 1);
        if y_start > y_end {
            return;
        }

        let mut intersections = Vec::with_capacity(points.len());
        let n = points.len();
        for py in y_start..=y_end {
            intersections.clear();
            let scan_y = py as f32 + 0.5;
            for i in 0..n {
                let (x0, y0) = points[i];
                let (x1, y1) = points[(i + 1) % n];
                let crosses = (y0 <= scan_y && y1 > scan_y) || (y1 <= scan_y && y0 > scan_y);
                if crosses {
                    let t = (scan_y - y0) / (y1 - y0);
                    intersections.push(x0 + t * (x1 - x0));
                }
            }
            intersections.sort_by(|a, b| a.total_cmp(b));
            let row = (py as usize) * self.width;
            for pair in intersections.chunks_exact(2) {
                let x_start = (pair[0].ceil() as i32).max(0);
                let x_end = (pair[1].floor() as i32).min(self.width as i32 - 1);
                if x_start <= x_end {
                    for px in x_start as usize..=x_end as usize {
                        self.pixels[row + px] = Rgb::blend(self.pixels[row + px], color, alpha);
                    }
                }
            }
        }
    }

    pub fn fill_ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, color: Rgb, alpha: f32) {
        if rx <= 0.0 || ry <= 0.0 || alpha <= 0.001 {
            return;
        }
        let x0 = ((cx - rx).floor() as i32).max(0);
        let x1 = ((cx + rx).ceil() as i32).min(self.width as i32 - 1);
        let y0 = ((cy - ry).floor() as i32).max(0);
        let y1 = ((cy + ry).ceil() as i32).min(self.height as i32 - 1);
        let inv_rx = 1.0 / rx;
        let inv_ry = 1.0 / ry;
        for py in y0..=y1 {
            let dy = (py as f32 + 0.5 - cy) * inv_ry;
            let dy2 = dy * dy;
            if dy2 > 1.0 {
                continue;
            }
            let row = (py as usize) * self.width;
            for px in x0..=x1 {
                let dx = (px as f32 + 0.5 - cx) * inv_rx;
                if dx * dx + dy2 <= 1.0 {
                    self.pixels[row + px as usize] =
                        Rgb::blend(self.pixels[row + px as usize], color, alpha);
                }
            }
        }
    }

    pub fn fill_circle(&mut self, cx: f32, cy: f32, r: f32, color: Rgb, alpha: f32) {
        self.fill_ellipse(cx, cy, r, r, color, alpha);
    }

    pub fn add_radial_glow(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, color: Rgb, peak: f32) {
        if rx <= 0.0 || ry <= 0.0 || peak <= 0.001 {
            return;
        }
        let x0 = ((cx - rx).floor() as i32).max(0);
        let x1 = ((cx + rx).ceil() as i32).min(self.width as i32 - 1);
        let y0 = ((cy - ry).floor() as i32).max(0);
        let y1 = ((cy + ry).ceil() as i32).min(self.height as i32 - 1);
        let inv_rx = 1.0 / rx;
        let inv_ry = 1.0 / ry;
        for py in y0..=y1 {
            let dy = (py as f32 + 0.5 - cy) * inv_ry;
            let dy2 = dy * dy;
            if dy2 >= 1.0 {
                continue;
            }
            let row = (py as usize) * self.width;
            for px in x0..=x1 {
                let dx = (px as f32 + 0.5 - cx) * inv_rx;
                let d2 = dx * dx + dy2;
                if d2 < 1.0 {
                    let falloff = (1.0 - d2.sqrt()).powi(2);
                    let idx = row + px as usize;
                    self.pixels[idx] = self.pixels[idx].add_clamped(color, peak * falloff);
                }
            }
        }
    }

    pub fn stroke_line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, color: Rgb, alpha: f32) {
        let dx = x1 - x0;
        let dy = y1 - y0;
        let steps = dx.abs().max(dy.abs()).ceil() as i32;
        if steps <= 0 {
            self.set(x0.round() as i32, y0.round() as i32, color, alpha);
            return;
        }
        let inv = 1.0 / steps as f32;
        for i in 0..=steps {
            let t = i as f32 * inv;
            let x = (x0 + dx * t).round() as i32;
            let y = (y0 + dy * t).round() as i32;
            self.set(x, y, color, alpha);
        }
    }

    pub fn stroke_circle(&mut self, cx: f32, cy: f32, r: f32, color: Rgb, alpha: f32) {
        if r <= 0.5 {
            return;
        }
        let steps = (r * std::f32::consts::TAU * 1.5).ceil().max(16.0) as usize;
        for i in 0..steps {
            let a = (i as f32 / steps as f32) * std::f32::consts::TAU;
            let x = (cx + a.cos() * r).round() as i32;
            let y = (cy + a.sin() * r).round() as i32;
            self.set(x, y, color, alpha);
        }
    }

    pub fn draw_starfield(
        &mut self,
        seed: u64,
        count: usize,
        max_y: usize,
        phase: f32,
        dim: Rgb,
        bright: Rgb,
    ) {
        let y_limit = max_y.min(self.height).max(1);
        let mut s = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
        for i in 0..count {
            s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let x = ((s >> 32) as usize) % self.width;
            s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let y = ((s >> 32) as usize) % y_limit;
            let twinkle = ((phase * std::f32::consts::TAU + i as f32 * 1.37).sin() * 0.5 + 0.5)
                .clamp(0.0, 1.0);
            let color = if i % 5 == 0 {
                dim.lerp(bright, twinkle)
            } else {
                dim.scale(0.55 + 0.45 * twinkle)
            };
            self.add_glow(x as i32, y as i32, color, 0.85);
        }
    }

    /// Clear procedural stars inside a normalized rectangle (e.g. behind overlaid title text).
    pub fn mask_rect(&mut self, x0: usize, y0: usize, x1: usize, y1: usize, fill: Rgb) {
        let rx0 = x0.min(self.width);
        let rx1 = x1.min(self.width);
        let ry0 = y0.min(self.height);
        let ry1 = y1.min(self.height);
        for y in ry0..ry1 {
            for x in rx0..rx1 {
                self.pixels[y * self.width + x] = fill;
            }
        }
    }

    /// Return the average luminance across the buffer (0.0..=1.0).
    pub fn mean_luminance(&self) -> f32 {
        let sum: f32 = self.pixels.iter().map(|c| c.luminance()).sum();
        sum / self.pixels.len().max(1) as f32
    }
}
