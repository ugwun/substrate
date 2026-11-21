//! Universal half-block 2D polygon, gradient, and cinematic rasterizer for standard terminals.
//!
//! Renders resolution-independent flat-shaded vector geometry into an RGB pixel buffer
//! (`width × 2*height`) and blits it across five terminal capability tiers: TrueColor,
//! xterm-256, dithered ANSI-16, ASCII luminance, and Monochrome.

pub mod pixel;
pub mod present;
pub mod scene;
pub mod shape;

pub use pixel::{PixelBuffer, Rgb};
pub use present::{blit_to_frame, buffer_to_text, ColorTier};
pub use scene::{ease_in_out, ease_out_cubic, Camera};
pub use shape::{draw_poly_norm, draw_shape_list, PolyShape};
