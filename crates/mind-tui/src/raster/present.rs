//! Five-tier terminal presenter for `PixelBuffer`: TrueColor half-block (`▀`), xterm-256
//! dithered half-block, ANSI-16 EGA dithered half-block, ASCII luminance, and Monochrome.

use super::pixel::{bayer_threshold, PixelBuffer, Rgb};
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span, Text},
    Frame,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorTier {
    TrueColor,
    Ansi256,
    Ansi16,
    Ascii,
    Monochrome,
}

const ANSI16_TABLE: [(Color, Rgb); 16] = [
    (Color::Black, Rgb(0, 0, 0)),
    (Color::Red, Rgb(170, 0, 0)),
    (Color::Green, Rgb(0, 170, 0)),
    (Color::Yellow, Rgb(170, 110, 0)),
    (Color::Blue, Rgb(0, 0, 170)),
    (Color::Magenta, Rgb(170, 0, 170)),
    (Color::Cyan, Rgb(0, 160, 170)),
    (Color::Gray, Rgb(170, 170, 170)),
    (Color::DarkGray, Rgb(76, 86, 94)),
    (Color::LightRed, Rgb(255, 85, 85)),
    (Color::LightGreen, Rgb(85, 255, 85)),
    (Color::LightYellow, Rgb(255, 230, 85)),
    (Color::LightBlue, Rgb(85, 115, 255)),
    (Color::LightMagenta, Rgb(255, 85, 255)),
    (Color::LightCyan, Rgb(85, 235, 235)),
    (Color::White, Rgb(245, 245, 240)),
];

#[inline]
fn quantize_rgb(c: Rgb) -> Color {
    // 4-step quantization keeps smooth visual fidelity while dramatically reducing Ratatui
    // diff chatter over SSH and remote links.
    let q = |v: u8| v.saturating_add(2) / 4 * 4;
    Color::Rgb(q(c.0), q(c.1), q(c.2))
}

#[inline]
fn perceptual_dist_sq(r1: f32, g1: f32, b1: f32, r2: f32, g2: f32, b2: f32) -> f32 {
    let dr = r1 - r2;
    let dg = g1 - g2;
    let db = b1 - b2;
    dr * dr * 0.30 + dg * dg * 0.59 + db * db * 0.11
}

#[inline]
pub(crate) fn to_xterm256_solid(c: Rgb) -> Color {
    to_xterm256_inner(f32::from(c.0), f32::from(c.1), f32::from(c.2))
}

#[inline]
fn to_xterm256_inner(r: f32, g: f32, b: f32) -> Color {
    const CUBE_LEVELS: [f32; 6] = [0.0, 95.0, 135.0, 175.0, 215.0, 255.0];
    let cube_idx = |v: f32| -> usize {
        if v < 48.0 {
            0
        } else if v < 115.0 {
            1
        } else {
            ((v - 35.0) / 40.0).round().clamp(0.0, 5.0) as usize
        }
    };
    let ri = cube_idx(r);
    let gi = cube_idx(g);
    let bi = cube_idx(b);
    let cube_color = 16 + 36 * (ri as u8) + 6 * (gi as u8) + (bi as u8);
    let cr = CUBE_LEVELS[ri];
    let cg = CUBE_LEVELS[gi];
    let cb = CUBE_LEVELS[bi];
    let cube_dist = perceptual_dist_sq(r, g, b, cr, cg, cb);

    let lum = r * 0.30 + g * 0.59 + b * 0.11;
    let (gray_color, gray_val) = if lum < 4.0 {
        (16u8, 0.0)
    } else if lum > 246.0 {
        (231u8, 255.0)
    } else {
        let idx = ((lum - 8.0) / 10.0).round().clamp(0.0, 23.0) as u8;
        (232 + idx, 8.0 + 10.0 * f32::from(idx))
    };
    let gray_dist = perceptual_dist_sq(r, g, b, gray_val, gray_val, gray_val);

    // Preserve teal/cyan hues in the 6x6x6 cube (indices 23, 24, 30, 31) rather than
    // flattening dark-teal planetary surfaces onto neutral gray.
    let src_chroma = r.max(g).max(b) - r.min(g).min(b);
    let cube_has_teal = (cg >= 95.0 && cb >= 95.0 && cr == 0.0) || (cb >= 95.0 && cg >= cr);
    let gray_penalty = if src_chroma >= 22.0 && cube_has_teal && lum >= 18.0 {
        1.65
    } else {
        1.0
    };

    if gray_dist * gray_penalty < cube_dist {
        Color::Indexed(gray_color)
    } else {
        Color::Indexed(cube_color)
    }
}

#[inline]
pub(crate) fn to_xterm256(c: Rgb, x: usize, y: usize) -> Color {
    let d = (bayer_threshold(x, y) - 0.5) * 10.0;
    let r = (f32::from(c.0) + d).clamp(0.0, 255.0);
    let g = (f32::from(c.1) + d).clamp(0.0, 255.0);
    let b = (f32::from(c.2) + d).clamp(0.0, 255.0);
    to_xterm256_inner(r, g, b)
}

#[inline]
fn to_ansi16(c: Rgb, x: usize, y: usize) -> Color {
    let d = (bayer_threshold(x, y) - 0.5) * 18.0;
    let tr = (f32::from(c.0) + d).clamp(0.0, 255.0);
    let tg = (f32::from(c.1) + d).clamp(0.0, 255.0);
    let tb = (f32::from(c.2) + d).clamp(0.0, 255.0);
    let mut best_color = Color::Black;
    let mut best_dist = f32::INFINITY;
    for &(color, Rgb(r, g, b)) in &ANSI16_TABLE {
        let dist = perceptual_dist_sq(tr, tg, tb, f32::from(r), f32::from(g), f32::from(b));
        if dist < best_dist {
            best_dist = dist;
            best_color = color;
        }
    }
    best_color
}

#[inline]
fn pixel_color(c: Rgb, x: usize, y: usize, tier: ColorTier) -> Color {
    match tier {
        ColorTier::TrueColor => quantize_rgb(c),
        ColorTier::Ansi256 => to_xterm256(c, x, y),
        ColorTier::Ansi16 => to_ansi16(c, x, y),
        ColorTier::Ascii | ColorTier::Monochrome => Color::Reset,
    }
}

/// Blit `PixelBuffer` directly into a Ratatui `Frame` region (`area.height` rows × `area.width` cols).
/// Expects `buf.width == area.width` and `buf.height == area.height * 2` (clips cleanly otherwise).
pub fn blit_to_frame(buf: &PixelBuffer, frame: &mut Frame<'_>, area: Rect, tier: ColorTier) {
    let out = frame.buffer_mut();
    let cols = usize::from(area.width).min(buf.width);
    let rows = usize::from(area.height).min(buf.height / 2);

    match tier {
        ColorTier::TrueColor | ColorTier::Ansi256 | ColorTier::Ansi16 => {
            for row in 0..rows {
                let y_top = row * 2;
                let y_bot = y_top + 1;
                let ty = area.y + row as u16;
                for col in 0..cols {
                    let top = pixel_color(buf.get(col, y_top), col, y_top, tier);
                    let bot = pixel_color(buf.get(col, y_bot), col, y_bot, tier);
                    let cell = &mut out[(area.x + col as u16, ty)];
                    if top == bot {
                        cell.set_symbol(" ");
                        cell.set_style(Style::default().bg(top));
                    } else {
                        cell.set_symbol("▀");
                        cell.set_style(Style::default().fg(top).bg(bot));
                    }
                }
            }
        }
        ColorTier::Ascii | ColorTier::Monochrome => {
            const ASCII_RAMP: &[&str] = &[" ", ".", ":", "-", "=", "+", "*", "#", "%", "@"];
            const MONO_RAMP: &[&str] = &[" ", "·", "░", "▒", "▓", "█"];
            let ramp = if tier == ColorTier::Ascii {
                ASCII_RAMP
            } else {
                MONO_RAMP
            };
            let max_idx = ramp.len() - 1;
            for row in 0..rows {
                let y_top = row * 2;
                let y_bot = y_top + 1;
                let ty = area.y + row as u16;
                for col in 0..cols {
                    let lum =
                        (buf.get(col, y_top).luminance() + buf.get(col, y_bot).luminance()) * 0.5;
                    let idx = ((lum * max_idx as f32).round() as usize).min(max_idx);
                    let cell = &mut out[(area.x + col as u16, ty)];
                    cell.set_symbol(ramp[idx]);
                    cell.set_style(Style::default());
                }
            }
        }
    }
}

/// Convert `PixelBuffer` into run-length merged Ratatui `Text<'static>` lines for Paragraph widgets.
pub fn buffer_to_text(buf: &PixelBuffer, tier: ColorTier) -> Text<'static> {
    let rows = buf.height / 2;
    let mut lines = Vec::with_capacity(rows);

    match tier {
        ColorTier::TrueColor | ColorTier::Ansi256 | ColorTier::Ansi16 => {
            for row in 0..rows {
                let y_top = row * 2;
                let y_bot = y_top + 1;
                let mut spans: Vec<Span<'static>> = Vec::new();
                let mut run_style: Option<Style> = None;
                let mut run_text = String::with_capacity(buf.width);
                for col in 0..buf.width {
                    let top = pixel_color(buf.get(col, y_top), col, y_top, tier);
                    let bot = pixel_color(buf.get(col, y_bot), col, y_bot, tier);
                    let (ch, style) = if top == bot {
                        (' ', Style::default().bg(top))
                    } else {
                        ('▀', Style::default().fg(top).bg(bot))
                    };
                    if run_style == Some(style) {
                        run_text.push(ch);
                    } else {
                        if let Some(prev) = run_style {
                            spans.push(Span::styled(std::mem::take(&mut run_text), prev));
                        }
                        run_style = Some(style);
                        run_text.push(ch);
                    }
                }
                if let Some(prev) = run_style {
                    spans.push(Span::styled(run_text, prev));
                }
                lines.push(Line::from(spans));
            }
        }
        ColorTier::Ascii | ColorTier::Monochrome => {
            const ASCII_RAMP: &[char] = &[' ', '.', ':', '-', '=', '+', '*', '#', '%', '@'];
            const MONO_RAMP: &[char] = &[' ', '·', '░', '▒', '▓', '█'];
            let ramp = if tier == ColorTier::Ascii {
                ASCII_RAMP
            } else {
                MONO_RAMP
            };
            let max_idx = ramp.len() - 1;
            for row in 0..rows {
                let y_top = row * 2;
                let y_bot = y_top + 1;
                let mut s = String::with_capacity(buf.width);
                for col in 0..buf.width {
                    let lum =
                        (buf.get(col, y_top).luminance() + buf.get(col, y_bot).luminance()) * 0.5;
                    let idx = ((lum * max_idx as f32).round() as usize).min(max_idx);
                    s.push(ramp[idx]);
                }
                lines.push(Line::raw(s));
            }
        }
    }

    Text::from(lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    #[test]
    fn polygon_fill_and_all_five_color_tiers_render_deterministically() {
        let mut buf = PixelBuffer::new(40, 24, Rgb(2, 16, 26));
        buf.fill_vertical_gradient(Rgb(2, 14, 28), Rgb(10, 48, 66), true);
        buf.fill_polygon(
            &[(4.0, 22.0), (18.0, 6.0), (34.0, 22.0)],
            Rgb(42, 161, 152),
            1.0,
        );
        buf.add_radial_glow(18.0, 8.0, 8.0, 6.0, Rgb(94, 214, 214), 0.6);
        assert!(buf.mean_luminance() > 0.05);

        for tier in [
            ColorTier::TrueColor,
            ColorTier::Ansi256,
            ColorTier::Ansi16,
            ColorTier::Ascii,
            ColorTier::Monochrome,
        ] {
            let backend = TestBackend::new(40, 12);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal
                .draw(|frame| blit_to_frame(&buf, frame, frame.area(), tier))
                .unwrap();
            let rendered = terminal.backend().buffer();
            for y in 0..12 {
                for x in 0..40 {
                    let cell = &rendered[(x, y)];
                    match tier {
                        ColorTier::Ansi16 => {
                            assert!(!matches!(cell.fg, Color::Rgb(_, _, _) | Color::Indexed(_)));
                            assert!(!matches!(cell.bg, Color::Rgb(_, _, _) | Color::Indexed(_)));
                        }
                        ColorTier::Ansi256 => {
                            assert!(!matches!(cell.fg, Color::Rgb(_, _, _)));
                            assert!(!matches!(cell.bg, Color::Rgb(_, _, _)));
                        }
                        ColorTier::Ascii => {
                            assert!(cell.symbol().is_ascii());
                            assert_eq!(cell.fg, Color::Reset);
                            assert_eq!(cell.bg, Color::Reset);
                        }
                        ColorTier::Monochrome => {
                            assert_eq!(cell.fg, Color::Reset);
                            assert_eq!(cell.bg, Color::Reset);
                        }
                        ColorTier::TrueColor => {}
                    }
                }
            }
        }
    }
}
