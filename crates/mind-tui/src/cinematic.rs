//! Full-bleed half-block polygon cinematics, Coherence phase canvas,
//! discovery plates, Solar Horizon pullback, and technical Probe Bay craft schematics.
//!
//! Every composition renders strictly from player-visible state using universal terminal cells
//! (TrueColor, xterm-256, ANSI-16, ASCII, Monochrome) and stays within Substrate's deep-blue,
//! indigo, pale-cyan, and gold palette.

use crate::raster::{draw_poly_norm, PixelBuffer, Rgb};
use mind_core::protocol::{ExtractorLoadout, ExtractorState, ScoutState};
use std::f32::consts::TAU;

/// Full-bleed planetary horizon Awakening sequence (boot & quick power-on).
pub fn build_awakening_buffer(
    width: u16,
    height: u16,
    progress: f32,
    quick_mode: bool,
) -> PixelBuffer {
    let pw = usize::from(width.max(20));
    let ph = usize::from(height.max(8)) * 2;
    let mut buf = PixelBuffer::new(pw, ph, Rgb(1, 10, 18));
    let p = progress.clamp(0.0, 1.0);
    let ease = 1.0 - (1.0 - p).powi(3);

    // Deep space vertical gradient.
    buf.fill_gradient_stops(
        &[
            (0.0, Rgb(1, 8, 16)),
            (0.58, Rgb(2, 18, 30)),
            (0.84, Rgb(8, 38, 52)),
            (1.0, Rgb(2, 14, 24)),
        ],
        true,
    );
    buf.draw_starfield(
        107,
        (pw * 2 / 3).max(28),
        (ph * 3) / 4,
        p * 0.4,
        Rgb(44, 84, 102),
        Rgb(168, 224, 232),
    );

    let wf = pw as f32;
    let hf = ph as f32;
    let cx = wf * 0.5;

    if !quick_mode && p < 0.22 {
        // Phase 1: Horizontal acquisition sweep expanding cleanly from center.
        let beam = (p / 0.22).clamp(0.05, 1.0);
        let half = wf * 0.46 * beam;
        let cy = hf * 0.50;
        buf.add_radial_glow(cx, cy, half.max(4.0), 4.5, Rgb(54, 198, 206), 0.72);
        buf.stroke_line(cx - half, cy, cx + half, cy, Rgb(134, 234, 236), 0.95);
        buf.add_radial_glow(cx, cy, 4.0, 3.0, Rgb(250, 210, 84), 0.85);
        return buf;
    }

    // Rising curved planetary limb with luminous cyan Fresnel atmosphere.
    let rise = if quick_mode {
        0.70 + 0.30 * ease
    } else {
        ((p - 0.16) / 0.65).clamp(0.0, 1.0)
    };
    let rise_eased = 1.0 - (1.0 - rise).powi(2);
    let r = wf.max(hf) * 1.12;
    let limb_top = hf * (1.06 - 0.42 * rise_eased);
    let cy = limb_top + r;

    buf.add_radial_glow(
        cx,
        limb_top + 2.0,
        wf * 0.60,
        hf * 0.26,
        Rgb(48, 186, 194),
        0.58 * rise_eased,
    );
    // Atmospheric crescent rim just above the planet body.
    buf.fill_circle(cx, cy, r + 2.2, Rgb(62, 204, 210), 0.75 * rise_eased);
    buf.fill_circle(cx, cy, r + 0.8, Rgb(142, 232, 234), 0.85 * rise_eased);
    buf.fill_circle(cx, cy, r, Rgb(3, 24, 36), 1.0);

    // Subtle sunlit ridge band on the rising limb.
    if rise_eased > 0.35 {
        draw_poly_norm(
            &mut buf,
            &[
                (0.0, 0.94),
                (0.22, 0.84),
                (0.45, 0.78),
                (0.72, 0.82),
                (1.0, 0.92),
                (1.0, 1.0),
                (0.0, 1.0),
            ],
            Rgb(10, 42, 56),
            0.75 * rise_eased,
            0.0,
            0.0,
            1.0,
        );
    }

    // Phase 3: Three faint harmonic waves converging toward the center datum.
    if !quick_mode && p > 0.58 {
        let wave_alpha = ((p - 0.58) / 0.30).clamp(0.0, 1.0);
        for band in 0..3 {
            let phase_off = (band as f32 - 1.0) * (1.0 - wave_alpha * 0.55) * 1.1;
            let base_y = hf * (0.34 + band as f32 * 0.08);
            let amp = hf * 0.045;
            let col = if band == 1 {
                Rgb(248, 206, 78)
            } else {
                Rgb(66, 202, 208)
            };
            let mut prev: Option<(f32, f32)> = None;
            for x in (2..pw.saturating_sub(2)).step_by(2) {
                let u = x as f32 / wf;
                let y = base_y + ((u * TAU * 2.2) + phase_off + p * TAU).sin() * amp;
                if let Some((px, py)) = prev {
                    buf.stroke_line(px, py, x as f32, y, col, 0.55 * wave_alpha);
                }
                prev = Some((x as f32, y));
            }
        }
    }

    buf
}

/// Full-bleed Coherence onboarding harmonic phase-alignment canvas.
pub fn build_coherence_buffer(
    width: u16,
    height: u16,
    positions: [u8; 3],
    selected_band: usize,
    phase: f64,
    confirming: bool,
) -> PixelBuffer {
    let pw = usize::from(width.max(24));
    let ph = usize::from(height.max(10)) * 2;
    let mut buf = PixelBuffer::new(pw, ph, Rgb(1, 12, 20));
    let wf = pw as f32;
    let hf = ph as f32;
    let t = phase as f32;
    let aligned = positions[0] == positions[1] && positions[1] == positions[2];

    buf.fill_gradient_stops(
        &[
            (0.0, Rgb(1, 10, 18)),
            (0.52, Rgb(3, 24, 36)),
            (0.80, Rgb(8, 44, 58)),
            (1.0, Rgb(2, 16, 24)),
        ],
        true,
    );
    buf.draw_starfield(
        313,
        (pw / 2).max(20),
        (ph * 2) / 3,
        t * 0.3,
        Rgb(40, 78, 96),
        Rgb(144, 206, 218),
    );

    // Distant quiet planetary silhouette along the lower quarter.
    draw_poly_norm(
        &mut buf,
        &[
            (0.0, 0.84),
            (0.18, 0.76),
            (0.38, 0.80),
            (0.62, 0.75),
            (0.84, 0.79),
            (1.0, 0.83),
            (1.0, 1.0),
            (0.0, 1.0),
        ],
        Rgb(5, 24, 34),
        0.92,
        0.0,
        0.0,
        1.0,
    );

    // Central vertical phase datum line & lock aura.
    let cx = wf * 0.5;
    let cy = hf * 0.44;
    buf.stroke_line(cx, hf * 0.08, cx, hf * 0.78, Rgb(24, 72, 88), 0.65);

    if aligned || confirming {
        let pulse = (t * TAU * 1.4).sin() * 0.15 + 0.85;
        buf.add_radial_glow(
            cx,
            cy,
            wf * 0.24,
            hf * 0.26,
            Rgb(64, 214, 216),
            0.55 * pulse,
        );
        buf.add_radial_glow(
            cx,
            cy,
            wf * 0.10,
            hf * 0.12,
            Rgb(250, 210, 84),
            0.65 * pulse,
        );
    }

    // Three full-width luminous harmonic wave curves representing the 3 alignment bands.
    let margin = (wf * 0.06).max(3.0);
    let span = (wf - margin * 2.0).max(8.0);
    let amp = (hf * 0.11).max(2.5);

    for (band, &slot) in positions.iter().enumerate() {
        let is_selected = band == selected_band && !confirming;
        let band_cy = if aligned || confirming {
            cy + (band as f32 - 1.0) * 1.2
        } else {
            hf * (0.26 + band as f32 * 0.18)
        };
        let slot_angle = (f32::from(slot % 12) / 12.0) * TAU;
        let col = if aligned || confirming {
            if band == 1 {
                Rgb(252, 216, 94)
            } else {
                Rgb(106, 232, 234)
            }
        } else if is_selected {
            Rgb(250, 206, 76)
        } else {
            Rgb(48, 168, 178)
        };
        let alpha = if is_selected || aligned || confirming {
            0.95
        } else {
            0.65
        };

        // Subtle horizontal baseline for each band.
        buf.stroke_line(
            margin,
            band_cy,
            wf - margin,
            band_cy,
            if is_selected {
                Rgb(34, 96, 114)
            } else {
                Rgb(14, 46, 60)
            },
            0.55,
        );

        let steps = (span as usize).max(24);
        let mut prev: Option<(f32, f32)> = None;
        for s in 0..=steps {
            let u = s as f32 / steps as f32;
            let x = margin + u * span;
            let env = (u * std::f32::consts::PI).sin().powf(0.55);
            let wave = (u * TAU * 2.0 + slot_angle + t * TAU * 0.7).sin();
            let y = band_cy - wave * amp * env;
            if let Some((px, py)) = prev {
                buf.stroke_line(px, py, x, y, col, alpha);
            }
            prev = Some((x, y));
        }

        // Phase crest marker sliding horizontally according to band slot (0..12).
        let marker_u = (f32::from(slot % 12) + 0.5) / 12.0;
        let mx = margin + marker_u * span;
        buf.add_radial_glow(
            mx,
            band_cy,
            4.0,
            3.0,
            col,
            if is_selected { 0.85 } else { 0.45 },
        );
        buf.fill_circle(mx, band_cy, if is_selected { 1.8 } else { 1.2 }, col, 1.0);
    }

    buf
}

/// Full-width bespoke polygon illustration for each of the 5 approved archive records.
pub fn build_discovery_buffer(
    record_id: &str,
    width: u16,
    height: u16,
    phase: f64,
    reveal: f64,
) -> PixelBuffer {
    let pw = usize::from(width.max(24));
    let ph = usize::from(height.max(6)) * 2;
    let mut buf = PixelBuffer::new(pw, ph, Rgb(2, 14, 24));
    let t = phase as f32;
    let r = reveal.clamp(0.0, 1.0) as f32;

    match record_id {
        "observation_001" => draw_discovery_001_retained_interval(&mut buf, t, r),
        "observation_002" => draw_discovery_002_unexplained_ratio(&mut buf, t, r),
        "observation_003" => draw_discovery_003_pause_structure(&mut buf, t, r),
        "observation_004" => draw_discovery_004_stable_weighting(&mut buf, t, r),
        "observation_005" => draw_discovery_005_wider_frame(&mut buf, t, r),
        _ => draw_discovery_001_retained_interval(&mut buf, t, r),
    }

    buf
}

/// Observation 001 (Retained Interval): three harmonic bands settling into a quiet basin.
fn draw_discovery_001_retained_interval(buf: &mut PixelBuffer, t: f32, r: f32) {
    let wf = buf.width as f32;
    let hf = buf.height as f32;
    buf.fill_gradient_stops(
        &[
            (0.0, Rgb(1, 10, 20)),
            (0.55, Rgb(5, 32, 48)),
            (0.82, Rgb(26, 108, 122)),
            (1.0, Rgb(4, 22, 32)),
        ],
        true,
    );
    buf.draw_starfield(
        11,
        (buf.width / 3).max(12),
        (buf.height / 4).max(2),
        t,
        Rgb(42, 82, 102),
        Rgb(168, 226, 232),
    );

    // Distant quiet basin hills.
    draw_poly_norm(
        buf,
        &[
            (0.0, 0.58),
            (0.24, 0.72),
            (0.42, 0.80),
            (0.58, 0.80),
            (0.76, 0.70),
            (1.0, 0.56),
            (1.0, 1.0),
            (0.0, 1.0),
        ],
        Rgb(12, 46, 62),
        1.0,
        0.0,
        0.0,
        1.0,
    );
    draw_poly_norm(
        buf,
        &[
            (0.0, 0.74),
            (0.32, 0.88),
            (0.68, 0.88),
            (1.0, 0.72),
            (1.0, 1.0),
            (0.0, 1.0),
        ],
        Rgb(6, 28, 40),
        1.0,
        0.0,
        0.0,
        1.0,
    );

    // Three settling harmonic arcs converging toward a narrower retained interval at center.
    let settle = 0.25 + 0.75 * r;
    buf.add_radial_glow(
        wf * 0.5,
        hf * 0.50,
        wf * 0.22,
        hf * 0.24,
        Rgb(248, 206, 78),
        0.42 * settle,
    );
    for band in 0..3 {
        let spread = (band as f32 - 1.0) * (3.2 - 1.8 * settle);
        let y0 = hf * 0.44 + spread;
        let col = if band == 1 {
            Rgb(250, 212, 86)
        } else {
            Rgb(86, 218, 222)
        };
        let mut prev: Option<(f32, f32)> = None;
        for x in (4..buf.width.saturating_sub(4)).step_by(2) {
            let u = x as f32 / wf;
            let amp = (hf * 0.12) * (1.0 - 0.35 * settle);
            let y = y0
                + ((u * TAU * 2.0 + t * TAU * 0.5).sin()) * amp * (u * std::f32::consts::PI).sin();
            if let Some((px, py)) = prev {
                buf.stroke_line(px, py, x as f32, y, col, 0.9);
            }
            prev = Some((x as f32, y));
        }
    }
    // Crisp interval bracket markers at the retained center span.
    let bx1 = wf * 0.44;
    let bx2 = wf * 0.56;
    buf.stroke_line(
        bx1,
        hf * 0.24,
        bx1,
        hf * 0.68,
        Rgb(250, 212, 86),
        0.75 * settle,
    );
    buf.stroke_line(
        bx2,
        hf * 0.24,
        bx2,
        hf * 0.68,
        Rgb(250, 212, 86),
        0.75 * settle,
    );
}

/// Observation 002 (Repeated Span): recessed enclosure cutaway framing
/// six recessed controls, two stepped surface heights, and passive cobalt-gold thermal hold.
fn draw_discovery_002_unexplained_ratio(buf: &mut PixelBuffer, t: f32, r: f32) {
    let wf = buf.width as f32;
    let hf = buf.height as f32;
    buf.fill_gradient_stops(
        &[
            (0.0, Rgb(2, 12, 24)),
            (0.45, Rgb(7, 32, 48)),
            (0.74, Rgb(24, 88, 106)),
            (1.0, Rgb(4, 18, 28)),
        ],
        true,
    );
    buf.draw_starfield(
        23,
        (buf.width / 3).max(12),
        (buf.height / 3).max(3),
        t,
        Rgb(44, 86, 106),
        Rgb(170, 224, 232),
    );

    // Outer rocky horizon silhouette framing the recessed enclosure.
    draw_poly_norm(
        buf,
        &[
            (0.0, 0.58),
            (0.14, 0.44),
            (0.24, 0.48),
            (0.76, 0.48),
            (0.86, 0.42),
            (1.0, 0.56),
            (1.0, 1.0),
            (0.0, 1.0),
        ],
        Rgb(11, 40, 56),
        1.0,
        0.0,
        0.0,
        1.0,
    );

    // Passive enclosure outer roof profile (cutaway framing).
    draw_poly_norm(
        buf,
        &[
            (0.14, 0.84),
            (0.16, 0.30),
            (0.24, 0.22),
            (0.76, 0.22),
            (0.84, 0.30),
            (0.86, 0.84),
        ],
        Rgb(18, 62, 82),
        1.0,
        0.0,
        0.0,
        1.0,
    );
    // Recessed chamber cutaway cavity holding a narrow thermal band.
    draw_poly_norm(
        buf,
        &[
            (0.19, 0.82),
            (0.20, 0.34),
            (0.26, 0.28),
            (0.74, 0.28),
            (0.80, 0.34),
            (0.81, 0.82),
        ],
        Rgb(6, 26, 38),
        1.0,
        0.0,
        0.0,
        1.0,
    );
    // Rim highlight along the enclosure roof & walls.
    buf.stroke_line(
        wf * 0.24,
        hf * 0.22,
        wf * 0.76,
        hf * 0.22,
        Rgb(94, 214, 214),
        0.85,
    );
    buf.stroke_line(
        wf * 0.16,
        hf * 0.30,
        wf * 0.14,
        hf * 0.84,
        Rgb(56, 152, 168),
        0.75,
    );
    buf.stroke_line(
        wf * 0.84,
        hf * 0.30,
        wf * 0.86,
        hf * 0.84,
        Rgb(56, 152, 168),
        0.75,
    );

    // Passive thermal-hold contour glowing gently inside the enclosure cavity.
    let pulse = ((t * TAU * 0.8).sin() * 0.2 + 0.8) * (0.4 + 0.6 * r);
    buf.add_radial_glow(
        wf * 0.48,
        hf * 0.56,
        wf * 0.26,
        hf * 0.22,
        Rgb(42, 168, 178),
        0.48 * pulse,
    );
    buf.add_radial_glow(
        wf * 0.48,
        hf * 0.58,
        wf * 0.16,
        hf * 0.12,
        Rgb(236, 190, 64),
        0.38 * pulse,
    );

    // Two adjacent stepped surface heights inside the enclosure.
    draw_poly_norm(
        buf,
        &[
            (0.19, 0.64),
            (0.56, 0.64),
            (0.58, 0.76),
            (0.81, 0.76),
            (0.81, 0.84),
            (0.19, 0.84),
        ],
        Rgb(22, 76, 94),
        1.0,
        0.0,
        0.0,
        1.0,
    );
    buf.stroke_line(
        wf * 0.19,
        hf * 0.64,
        wf * 0.56,
        hf * 0.64,
        Rgb(106, 224, 226),
        0.85,
    );
    buf.stroke_line(
        wf * 0.58,
        hf * 0.76,
        wf * 0.81,
        hf * 0.76,
        Rgb(84, 192, 202),
        0.8,
    );

    // Horizontal recessed control panel seated above the upper surface.
    draw_poly_norm(
        buf,
        &[(0.23, 0.46), (0.69, 0.46), (0.69, 0.56), (0.23, 0.56)],
        Rgb(14, 50, 68),
        1.0,
        0.0,
        0.0,
        1.0,
    );
    buf.stroke_line(
        wf * 0.23,
        hf * 0.46,
        wf * 0.69,
        hf * 0.46,
        Rgb(94, 214, 214),
        0.85,
    );

    // Exactly 6 recessed control indicators spaced at one repeated span.
    for i in 0..6 {
        let nx = 0.27 + i as f32 * 0.076;
        let px = wf * nx;
        let py = hf * 0.51;
        buf.fill_circle(px, py, 1.15, Rgb(94, 214, 214), 0.95);
        buf.set(px.round() as i32, py.round() as i32, Rgb(250, 214, 92), 0.9);
    }

    // Caliper span tick comparing the two stepped surface heights inside the enclosure.
    buf.stroke_line(
        wf * 0.74,
        hf * 0.64,
        wf * 0.74,
        hf * 0.76,
        Rgb(242, 198, 72),
        0.9,
    );
    buf.stroke_line(
        wf * 0.71,
        hf * 0.64,
        wf * 0.77,
        hf * 0.64,
        Rgb(242, 198, 72),
        0.9,
    );
    buf.stroke_line(
        wf * 0.71,
        hf * 0.76,
        wf * 0.77,
        hf * 0.76,
        Rgb(242, 198, 72),
        0.9,
    );

    // Ground plane beneath the enclosure.
    draw_poly_norm(
        buf,
        &[(0.0, 0.84), (1.0, 0.84), (1.0, 1.0), (0.0, 1.0)],
        Rgb(6, 24, 34),
        1.0,
        0.0,
        0.0,
        1.0,
    );
}

/// Observation 003 (Pause Structure): twin cadence block waveforms with matching pauses.
fn draw_discovery_003_pause_structure(buf: &mut PixelBuffer, t: f32, r: f32) {
    let wf = buf.width as f32;
    let hf = buf.height as f32;
    buf.fill_gradient_stops(
        &[
            (0.0, Rgb(1, 12, 22)),
            (0.5, Rgb(4, 26, 38)),
            (1.0, Rgb(2, 14, 24)),
        ],
        true,
    );
    // Subtle oscilloscope grid.
    for row in 1..4 {
        let y = hf * (row as f32 / 4.0);
        buf.stroke_line(2.0, y, wf - 2.0, y, Rgb(12, 42, 56), 0.7);
    }

    // Four cadence block bursts separated by aligned pause intervals.
    let blocks = [
        (0.08_f32, 0.22_f32, 0.22_f32),
        (0.28, 0.44, 0.28),
        (0.50, 0.68, 0.20),
        (0.74, 0.91, 0.25),
    ];
    let pauses = [0.25_f32, 0.47, 0.71];

    // Luminous vertical alignment rails across matching pause clusters.
    for &px_n in &pauses {
        let px = wf * px_n;
        buf.stroke_line(
            px,
            hf * 0.12,
            px,
            hf * 0.88,
            Rgb(248, 206, 78),
            0.65 * (0.3 + 0.7 * r),
        );
        buf.add_radial_glow(px, hf * 0.50, 3.0, hf * 0.30, Rgb(248, 206, 78), 0.28);
    }

    let sweep_x = (t * 0.45).rem_euclid(1.0) * wf;
    for &(x0, x1, h_amp) in &blocks {
        let bx0 = wf * x0;
        let bx1 = wf * x1;
        // Upper trace: Recovered timing block (cyan).
        let u_base = hf * 0.36;
        let u_top = u_base - hf * h_amp;
        buf.fill_polygon(
            &[
                (bx0, u_base),
                (bx0 + 2.0, u_top),
                (bx1 - 2.0, u_top),
                (bx1, u_base),
            ],
            Rgb(54, 198, 206),
            0.88,
        );
        // Lower trace: Internal cadence block (gold).
        let l_base = hf * 0.78;
        let l_top = l_base - hf * (h_amp * 0.92);
        buf.fill_polygon(
            &[
                (bx0, l_base),
                (bx0 + 2.0, l_top),
                (bx1 - 2.0, l_top),
                (bx1, l_base),
            ],
            Rgb(234, 188, 64),
            0.85,
        );
    }
    buf.stroke_line(sweep_x, 0.0, sweep_x, hf, Rgb(146, 234, 236), 0.55);
}

/// Observation 004 (Stable Weighting): three distinct devices showing one identical 4-bar weighting.
fn draw_discovery_004_stable_weighting(buf: &mut PixelBuffer, t: f32, r: f32) {
    let wf = buf.width as f32;
    let hf = buf.height as f32;
    buf.fill_gradient_stops(
        &[
            (0.0, Rgb(2, 12, 22)),
            (0.65, Rgb(6, 34, 48)),
            (1.0, Rgb(3, 18, 28)),
        ],
        true,
    );
    buf.draw_starfield(
        47,
        (buf.width / 3).max(12),
        buf.height / 2,
        t,
        Rgb(42, 78, 96),
        Rgb(150, 212, 222),
    );

    // Three separated horizon zones for three unrelated exterior devices.
    let centers = [0.18_f32, 0.50, 0.82];
    for (idx, &cx_n) in centers.iter().enumerate() {
        let cx = wf * cx_n;
        let ground_y = hf * 0.64;
        buf.stroke_line(
            cx - wf * 0.12,
            ground_y,
            cx + wf * 0.12,
            ground_y,
            Rgb(26, 86, 102),
            0.8,
        );
        // Dissimilar exterior device silhouettes:
        match idx {
            0 => {
                // Device I: Low tripod dome.
                buf.fill_polygon(
                    &[
                        (cx - 5.0, ground_y),
                        (cx - 2.5, ground_y - hf * 0.22),
                        (cx + 2.5, ground_y - hf * 0.22),
                        (cx + 5.0, ground_y),
                    ],
                    Rgb(42, 152, 166),
                    1.0,
                );
            }
            1 => {
                // Device II: Wide flat tilted collector plate.
                buf.fill_polygon(
                    &[
                        (cx - 7.0, ground_y - 1.0),
                        (cx - 5.5, ground_y - hf * 0.14),
                        (cx + 6.5, ground_y - hf * 0.24),
                        (cx + 7.5, ground_y - 1.0),
                    ],
                    Rgb(66, 178, 186),
                    1.0,
                );
            }
            _ => {
                // Device III: Tall slender angled spire prism.
                buf.fill_polygon(
                    &[
                        (cx - 3.5, ground_y),
                        (cx, ground_y - hf * 0.36),
                        (cx + 3.5, ground_y),
                    ],
                    Rgb(98, 206, 212),
                    1.0,
                );
            }
        }
        // Identical ordered 4-bar signature below each dissimilar device.
        let weights = [0.95_f32, 0.74, 0.52, 0.32];
        let bar_cols = [
            Rgb(96, 224, 226),
            Rgb(248, 206, 78),
            Rgb(64, 178, 194),
            Rgb(146, 208, 222),
        ];
        for (b, (&w, &c)) in weights.iter().zip(bar_cols.iter()).enumerate() {
            let by = hf * (0.72 + b as f32 * 0.065);
            let half_w = wf * 0.10 * w * (0.35 + 0.65 * r);
            buf.stroke_line(cx - half_w, by, cx + half_w, by, c, 0.92);
        }
    }
}

/// Observation 005 (Wider Frame): local star, current world, and faint unresolved outer returns.
fn draw_discovery_005_wider_frame(buf: &mut PixelBuffer, t: f32, r: f32) {
    draw_solar_frame_internal(buf, 0.55 + 0.45 * r, t);
}

/// Projected terminal cell `(col, row)` of the Current World marker during the Solar Horizon pullback.
pub fn solar_horizon_world_cell(
    width: u16,
    height: u16,
    elapsed_secs: f32,
    phase: f64,
) -> (u16, u16) {
    let pw = usize::from(width.max(24)) as f32;
    let ph = (usize::from(height.max(10)) * 2) as f32;
    let p = (elapsed_secs / 2.6).clamp(0.0, 1.0);
    let eased = 1.0 - (1.0 - p).powi(3);
    let t = phase as f32;
    let star_x = pw * (0.14 + 0.14 * eased);
    let star_y = ph * 0.50;
    let orbit_r = pw.min(ph) * (0.18 + 0.18 * eased);
    let angle = 0.42 + t * 0.30;
    let world_x = star_x + orbit_r * angle.cos() * 1.15;
    let world_y = star_y - orbit_r * angle.sin() * 0.72;
    let col = (world_x.round().max(0.0) as u16).min(width.saturating_sub(1));
    let row = ((world_y * 0.5).round().max(0.0) as u16).min(height.saturating_sub(1));
    (col, row)
}

/// Animated client-local Solar Horizon pullback rendered in half-block `PixelBuffer`.
pub fn build_solar_horizon_buffer(
    width: u16,
    height: u16,
    elapsed_secs: f32,
    phase: f64,
) -> PixelBuffer {
    let pw = usize::from(width.max(24));
    let ph = usize::from(height.max(10)) * 2;
    let mut buf = PixelBuffer::new(pw, ph, Rgb(1, 10, 18));
    let p = (elapsed_secs / 2.6).clamp(0.0, 1.0);
    let eased = 1.0 - (1.0 - p).powi(3);
    draw_solar_frame_internal(&mut buf, eased, phase as f32);
    buf
}

fn draw_solar_frame_internal(buf: &mut PixelBuffer, eased: f32, t: f32) {
    let wf = buf.width as f32;
    let hf = buf.height as f32;
    buf.fill_gradient_stops(
        &[
            (0.0, Rgb(1, 8, 16)),
            (0.5, Rgb(2, 16, 28)),
            (1.0, Rgb(1, 10, 18)),
        ],
        true,
    );
    buf.draw_starfield(
        505,
        (buf.width * 2 / 3).max(24),
        buf.height,
        t * 0.25,
        Rgb(38, 72, 92),
        Rgb(148, 206, 222),
    );

    // Camera pulls smoothly back from the Current World to center the wide local frame.
    let star_x = wf * (0.14 + 0.14 * eased);
    let star_y = hf * 0.50;
    let orbit_r = wf.min(hf) * (0.18 + 0.18 * eased);
    let angle = 0.42 + t * 0.30;
    let world_x = star_x + orbit_r * angle.cos() * 1.15;
    let world_y = star_y - orbit_r * angle.sin() * 0.72;

    // Subtle parallax sweep vector across the frame.
    if eased > 0.30 {
        let sweep_a = t * TAU * 0.4;
        let sx = star_x + wf * 0.62 * sweep_a.cos();
        let sy = star_y + hf * 0.42 * sweep_a.sin();
        buf.stroke_line(star_x, star_y, sx, sy, Rgb(22, 68, 86), 0.45 * eased);
    }

    // Faint dotted reference track through the current world.
    let steps = 56;
    for i in (0..steps).step_by(2) {
        let a = (i as f32 / steps as f32) * TAU;
        let rx = star_x + orbit_r * a.cos() * 1.15;
        let ry = star_y - orbit_r * a.sin() * 0.72;
        buf.set(rx.round() as i32, ry.round() as i32, Rgb(34, 96, 114), 0.65);
    }

    // Local star: soft radial corona.
    let corona = 1.0 + (t * TAU).sin() * 0.10;
    buf.add_radial_glow(
        star_x,
        star_y,
        wf * 0.14 * corona,
        hf * 0.26 * corona,
        Rgb(236, 188, 66),
        0.52,
    );
    buf.add_radial_glow(
        star_x,
        star_y,
        wf * 0.055 * corona,
        hf * 0.10 * corona,
        Rgb(252, 232, 148),
        0.88,
    );
    buf.fill_circle(
        star_x,
        star_y,
        (hf * 0.065).max(2.0),
        Rgb(255, 244, 198),
        1.0,
    );

    // Current World: luminous cyan dot with soft atmospheric aura.
    buf.add_radial_glow(world_x, world_y, 5.0, 4.0, Rgb(64, 214, 218), 0.72);
    buf.fill_circle(
        world_x,
        world_y,
        (hf * 0.036).max(1.4),
        Rgb(126, 238, 240),
        1.0,
    );

    // Intermittent, shifting unresolved outer returns — uncounted and intentionally faint.
    let bucket = (t * 8.0).floor() as usize;
    let candidates = [
        (
            0.74_f32,
            0.24_f32,
            !bucket.is_multiple_of(3),
            Rgb(84, 206, 214),
        ),
        (
            0.84,
            0.66,
            bucket % 4 == 1 || bucket % 4 == 2,
            Rgb(238, 194, 72),
        ),
        (
            0.66,
            0.80,
            bucket.is_multiple_of(5) || bucket % 5 == 3,
            Rgb(94, 196, 216),
        ),
        (
            0.90,
            0.36,
            bucket % 7 == 2 || bucket % 7 == 5,
            Rgb(118, 212, 224),
        ),
    ];
    if eased > 0.45 {
        for &(nx, ny, vis, col) in &candidates {
            if vis {
                let ox = wf * nx;
                let oy = hf * ny;
                buf.add_radial_glow(ox, oy, 3.2, 2.6, col, 0.55 * eased);
                buf.set(ox.round() as i32, oy.round() as i32, col, 0.95);
            }
        }
    }
}

/// High-resolution half-block polygon craft schematic for the Probe Bay.
/// Uses a cohesive deep-teal drafting canvas (no clashing navy) and technical silhouettes.
#[allow(clippy::too_many_arguments)]
pub fn build_probe_schematic_buffer(
    is_extractor: bool,
    loadout: ExtractorLoadout,
    scout_state: ScoutState,
    extractor_state: ExtractorState,
    rebuild_pct: u64,
    width: u16,
    height: u16,
    phase: f64,
) -> PixelBuffer {
    let pw = usize::from(width.max(28));
    let ph = usize::from(height.max(10)) * 2;
    let mut buf = PixelBuffer::new(pw, ph, Rgb(2, 20, 30));
    let wf = pw as f32;
    let hf = ph as f32;
    let t = phase as f32;

    // Cohesive Substrate deep-teal drafting canvas with delicate technical grid.
    buf.fill_gradient_stops(
        &[
            (0.0, Rgb(2, 18, 28)),
            (0.5, Rgb(4, 26, 38)),
            (1.0, Rgb(2, 16, 26)),
        ],
        false,
    );
    let step_x = (pw / 8).max(5);
    let step_y = (ph / 6).max(4);
    for gx in (step_x..pw).step_by(step_x) {
        buf.stroke_line(gx as f32, 0.0, gx as f32, hf, Rgb(10, 44, 58), 0.55);
    }
    for gy in (step_y..ph).step_by(step_y) {
        buf.stroke_line(0.0, gy as f32, wf, gy as f32, Rgb(10, 44, 58), 0.55);
    }

    if !is_extractor {
        draw_scout_polygon_craft(&mut buf, scout_state, t);
    } else {
        draw_extractor_polygon_craft(&mut buf, loadout, extractor_state, rebuild_pct, t);
    }

    buf
}

fn draw_scout_polygon_craft(buf: &mut PixelBuffer, state: ScoutState, t: f32) {
    let wf = buf.width as f32;
    let hf = buf.height as f32;
    let bob = (t * TAU * 0.9).sin() * 0.015;
    let pulse = (t * TAU * 1.6).sin() * 0.30 + 0.70;

    // Subtle backing cyan schematic glow.
    buf.add_radial_glow(
        wf * 0.50,
        hf * 0.48,
        wf * 0.34,
        hf * 0.34,
        Rgb(36, 152, 168),
        0.32,
    );

    // Swept thermal / solar wing fins (left and right).
    draw_poly_norm(
        buf,
        &[
            (0.10, 0.50 + bob),
            (0.34, 0.38 + bob),
            (0.36, 0.56 + bob),
            (0.14, 0.60 + bob),
        ],
        Rgb(26, 108, 126),
        1.0,
        0.0,
        0.0,
        1.0,
    );
    draw_poly_norm(
        buf,
        &[
            (0.90, 0.50 + bob),
            (0.66, 0.38 + bob),
            (0.64, 0.56 + bob),
            (0.86, 0.60 + bob),
        ],
        Rgb(26, 108, 126),
        1.0,
        0.0,
        0.0,
        1.0,
    );
    // Specular cyan leading wing edges.
    buf.stroke_line(
        wf * 0.10,
        hf * (0.50 + bob),
        wf * 0.34,
        hf * (0.38 + bob),
        Rgb(118, 232, 234),
        0.9,
    );
    buf.stroke_line(
        wf * 0.90,
        hf * (0.50 + bob),
        wf * 0.66,
        hf * (0.38 + bob),
        Rgb(118, 232, 234),
        0.9,
    );

    // Main faceted central dorsal fuselage.
    draw_poly_norm(
        buf,
        &[
            (0.42, 0.28 + bob),
            (0.58, 0.28 + bob),
            (0.64, 0.48 + bob),
            (0.58, 0.68 + bob),
            (0.42, 0.68 + bob),
            (0.36, 0.48 + bob),
        ],
        Rgb(46, 168, 178),
        1.0,
        0.0,
        0.0,
        1.0,
    );
    // Lit upper instrument bay facet.
    draw_poly_norm(
        buf,
        &[
            (0.45, 0.32 + bob),
            (0.55, 0.32 + bob),
            (0.58, 0.46 + bob),
            (0.42, 0.46 + bob),
        ],
        Rgb(108, 224, 226),
        0.85,
        0.0,
        0.0,
        1.0,
    );

    // Parabolic wide sensor dish & telemetry mast at top.
    draw_poly_norm(
        buf,
        &[
            (0.38, 0.22 + bob),
            (0.50, 0.27 + bob),
            (0.62, 0.22 + bob),
            (0.57, 0.28 + bob),
            (0.43, 0.28 + bob),
        ],
        Rgb(234, 192, 68),
        0.95,
        0.0,
        0.0,
        1.0,
    );
    buf.stroke_line(
        wf * 0.50,
        hf * (0.24 + bob),
        wf * 0.50,
        hf * (0.11 + bob),
        Rgb(164, 236, 238),
        0.95,
    );
    buf.add_radial_glow(
        wf * 0.50,
        hf * (0.11 + bob),
        3.6,
        3.0,
        Rgb(250, 212, 86),
        0.85 * pulse,
    );

    // Lower thruster nozzles + live state visual cue (transit plume / active survey cone).
    draw_poly_norm(
        buf,
        &[
            (0.43, 0.68 + bob),
            (0.48, 0.68 + bob),
            (0.46, 0.76 + bob),
            (0.42, 0.76 + bob),
        ],
        Rgb(18, 72, 92),
        1.0,
        0.0,
        0.0,
        1.0,
    );
    draw_poly_norm(
        buf,
        &[
            (0.52, 0.68 + bob),
            (0.57, 0.68 + bob),
            (0.58, 0.76 + bob),
            (0.54, 0.76 + bob),
        ],
        Rgb(18, 72, 92),
        1.0,
        0.0,
        0.0,
        1.0,
    );

    match state {
        ScoutState::Surveying => {
            draw_poly_norm(
                buf,
                &[(0.48, 0.72), (0.52, 0.72), (0.76, 0.98), (0.24, 0.98)],
                Rgb(64, 214, 218),
                0.32 * pulse,
                0.0,
                0.0,
                1.0,
            );
            let sweep_y = hf * (0.76 + (t * 0.9).rem_euclid(1.0) * 0.20);
            buf.stroke_line(
                wf * 0.30,
                sweep_y,
                wf * 0.70,
                sweep_y,
                Rgb(250, 210, 84),
                0.85,
            );
        }
        ScoutState::Outbound | ScoutState::Returning => {
            buf.add_radial_glow(
                wf * 0.45,
                hf * 0.82,
                3.5,
                4.5,
                Rgb(94, 224, 226),
                0.75 * pulse,
            );
            buf.add_radial_glow(
                wf * 0.55,
                hf * 0.82,
                3.5,
                4.5,
                Rgb(94, 224, 226),
                0.75 * pulse,
            );
            buf.add_radial_glow(
                wf * 0.50,
                hf * 0.80,
                4.5,
                3.5,
                Rgb(250, 206, 76),
                0.55 * pulse,
            );
        }
        _ => {}
    }
}

fn draw_extractor_polygon_craft(
    buf: &mut PixelBuffer,
    loadout: ExtractorLoadout,
    state: ExtractorState,
    rebuild_pct: u64,
    t: f32,
) {
    let wf = buf.width as f32;
    let hf = buf.height as f32;
    let pulse = (t * TAU * 1.4).sin() * 0.28 + 0.72;
    let rebuilding = state == ExtractorState::Rebuilding;

    // Render complete craft into a temporary buffer when rebuilding so we can apply a clean
    // bottom-to-top vertical scanline assembly wipe!
    let mut craft = if rebuilding {
        PixelBuffer::new(buf.width, buf.height, Rgb(2, 20, 30))
    } else {
        PixelBuffer::new(1, 1, Rgb(0, 0, 0))
    };
    {
        let target: &mut PixelBuffer = if rebuilding { &mut craft } else { &mut *buf };

        // Rig-specific frame attachments (drawn behind/around core hull).
        match loadout {
            ExtractorLoadout::Balanced => {
                // Compact twin lateral ore pods.
                draw_poly_norm(
                    target,
                    &[(0.28, 0.36), (0.37, 0.32), (0.37, 0.62), (0.28, 0.58)],
                    Rgb(32, 118, 134),
                    1.0,
                    0.0,
                    0.0,
                    1.0,
                );
                draw_poly_norm(
                    target,
                    &[(0.72, 0.36), (0.63, 0.32), (0.63, 0.62), (0.72, 0.58)],
                    Rgb(32, 118, 134),
                    1.0,
                    0.0,
                    0.0,
                    1.0,
                );
            }
            ExtractorLoadout::Cargo => {
                // Heavy oversized outboard twin hoppers + reinforced side truss frames (gold accent).
                draw_poly_norm(
                    target,
                    &[(0.12, 0.30), (0.35, 0.28), (0.35, 0.66), (0.14, 0.66)],
                    Rgb(186, 142, 42),
                    1.0,
                    0.0,
                    0.0,
                    1.0,
                );
                draw_poly_norm(
                    target,
                    &[(0.88, 0.30), (0.65, 0.28), (0.65, 0.66), (0.86, 0.66)],
                    Rgb(186, 142, 42),
                    1.0,
                    0.0,
                    0.0,
                    1.0,
                );
                // Dark recessed cargo bay bands.
                draw_poly_norm(
                    target,
                    &[(0.16, 0.38), (0.31, 0.38), (0.31, 0.58), (0.16, 0.58)],
                    Rgb(42, 82, 86),
                    0.85,
                    0.0,
                    0.0,
                    1.0,
                );
                draw_poly_norm(
                    target,
                    &[(0.69, 0.38), (0.84, 0.38), (0.84, 0.58), (0.69, 0.58)],
                    Rgb(42, 82, 86),
                    0.85,
                    0.0,
                    0.0,
                    1.0,
                );
                target.stroke_line(
                    wf * 0.12,
                    hf * 0.30,
                    wf * 0.35,
                    hf * 0.28,
                    Rgb(250, 214, 92),
                    0.95,
                );
                target.stroke_line(
                    wf * 0.88,
                    hf * 0.30,
                    wf * 0.65,
                    hf * 0.28,
                    Rgb(250, 214, 92),
                    0.95,
                );
            }
            ExtractorLoadout::Shielded => {
                // Sloped faceted ablative armor canopy wrapping the chassis (cobalt-violet rim).
                draw_poly_norm(
                    target,
                    &[
                        (0.18, 0.44),
                        (0.32, 0.16),
                        (0.68, 0.16),
                        (0.82, 0.44),
                        (0.76, 0.68),
                        (0.24, 0.68),
                    ],
                    Rgb(66, 98, 168),
                    1.0,
                    0.0,
                    0.0,
                    1.0,
                );
                target.stroke_line(
                    wf * 0.32,
                    hf * 0.16,
                    wf * 0.68,
                    hf * 0.16,
                    Rgb(176, 198, 246),
                    0.95,
                );
                target.stroke_line(
                    wf * 0.18,
                    hf * 0.44,
                    wf * 0.32,
                    hf * 0.16,
                    Rgb(146, 178, 238),
                    0.9,
                );
                target.stroke_line(
                    wf * 0.82,
                    hf * 0.44,
                    wf * 0.68,
                    hf * 0.16,
                    Rgb(146, 178, 238),
                    0.9,
                );
            }
        }

        // Core heavy drill-rig hull (asymmetric dorsal antenna + armored lower body).
        draw_poly_norm(
            target,
            &[
                (0.38, 0.24),
                (0.62, 0.24),
                (0.66, 0.42),
                (0.60, 0.68),
                (0.40, 0.68),
                (0.34, 0.42),
            ],
            Rgb(44, 154, 168),
            1.0,
            0.0,
            0.0,
            1.0,
        );
        // Asymmetric offset telemetry fin & upper core collar.
        target.stroke_line(
            wf * 0.43,
            hf * 0.24,
            wf * 0.43,
            hf * 0.12,
            Rgb(146, 232, 234),
            0.9,
        );
        target.add_radial_glow(
            wf * 0.43,
            hf * 0.12,
            2.8,
            2.2,
            Rgb(250, 210, 84),
            0.8 * pulse,
        );
        draw_poly_norm(
            target,
            &[(0.42, 0.30), (0.58, 0.30), (0.58, 0.40), (0.42, 0.40)],
            Rgb(16, 54, 72),
            1.0,
            0.0,
            0.0,
            1.0,
        );
        // Horizontal instrument slit across the core block.
        target.stroke_line(
            wf * 0.44,
            hf * 0.35,
            wf * 0.56,
            hf * 0.35,
            Rgb(118, 234, 236),
            0.95,
        );

        // Stabilizer legs & central rotary drill auger.
        target.stroke_line(
            wf * 0.40,
            hf * 0.66,
            wf * 0.30,
            hf * 0.82,
            Rgb(86, 186, 198),
            0.9,
        );
        target.stroke_line(
            wf * 0.60,
            hf * 0.66,
            wf * 0.70,
            hf * 0.82,
            Rgb(86, 186, 198),
            0.9,
        );
        draw_poly_norm(
            target,
            &[(0.45, 0.68), (0.55, 0.68), (0.50, 0.92)],
            Rgb(236, 192, 68),
            1.0,
            0.0,
            0.0,
            1.0,
        );
        if state == ExtractorState::Extracting {
            target.add_radial_glow(
                wf * 0.50,
                hf * 0.92,
                5.5,
                3.2,
                Rgb(250, 212, 86),
                0.85 * pulse,
            );
        } else if matches!(state, ExtractorState::Outbound | ExtractorState::Returning) {
            target.add_radial_glow(
                wf * 0.38,
                hf * 0.76,
                3.2,
                4.0,
                Rgb(94, 224, 226),
                0.7 * pulse,
            );
            target.add_radial_glow(
                wf * 0.62,
                hf * 0.76,
                3.2,
                4.0,
                Rgb(94, 224, 226),
                0.7 * pulse,
            );
        }
    }

    if rebuilding {
        // Vertical bottom-to-top assembly scanline wipe!
        let frac = (rebuild_pct.min(100) as f32) / 100.0;
        let scan_y = ((1.0 - frac) * hf).clamp(2.0, hf - 2.0);
        let scan_row = scan_y.round() as usize;
        for y in 0..buf.height {
            for x in 0..buf.width {
                let c = craft.get(x, y);
                let is_craft = c != Rgb(2, 20, 30);
                if !is_craft {
                    continue;
                }
                if y >= scan_row {
                    buf.set(x as i32, y as i32, c, 1.0);
                } else if (x + y) % 3 == 0 {
                    // Unbuilt upper section appears as faint cyan technical projection wireframe.
                    buf.set(x as i32, y as i32, Rgb(34, 108, 126), 0.55);
                }
            }
        }
        buf.stroke_line(
            wf * 0.10,
            scan_y,
            wf * 0.90,
            scan_y,
            Rgb(252, 216, 94),
            0.95,
        );
        buf.add_radial_glow(
            wf * 0.50,
            scan_y,
            wf * 0.36,
            2.8,
            Rgb(250, 206, 76),
            0.65 * pulse,
        );
    }
}

/// Compact half-block research topology & profile telemetry diagram for the Research workspace.
pub fn build_research_topology_buffer(
    completed_flags: [bool; 3],
    active_index: Option<usize>,
    active_profile_idx: usize,
    width: u16,
    height: u16,
    phase: f64,
) -> PixelBuffer {
    let pw = usize::from(width.max(36));
    let ph = usize::from(height.max(5)) * 2;
    let mut buf = PixelBuffer::new(pw, ph, Rgb(3, 24, 34));
    let wf = pw as f32;
    let hf = ph as f32;
    let t = phase as f32;
    let pulse = (t * TAU).sin() * 0.25 + 0.75;

    buf.fill_gradient_stops(&[(0.0, Rgb(2, 16, 26)), (1.0, Rgb(5, 32, 44))], true);
    // Faint blueprint grid.
    for x in (0..pw).step_by(8) {
        buf.stroke_line(x as f32, 0.0, x as f32, hf, Rgb(10, 44, 58), 0.45);
    }

    let root_x = wf * 0.12;
    let root_y = hf * 0.50;
    let node_x = wf * 0.48;
    let prof_x = wf * 0.82;
    let ys = [hf * 0.22, hf * 0.50, hf * 0.78];

    // Core origin node.
    buf.add_radial_glow(root_x, root_y, 5.0, 4.0, Rgb(94, 214, 214), 0.55);
    buf.fill_circle(root_x, root_y, 2.2, Rgb(248, 206, 78), 1.0);

    for (i, &ny) in ys.iter().enumerate() {
        let done = completed_flags[i];
        let active = active_index == Some(i);
        let profile_on = active_profile_idx == i + 1;
        let branch_col = if done {
            Rgb(94, 214, 214)
        } else if active {
            Rgb(248, 206, 78)
        } else {
            Rgb(28, 76, 92)
        };
        buf.stroke_line(root_x + 2.0, root_y, node_x - 3.0, ny, branch_col, 0.85);
        buf.stroke_line(
            node_x + 3.0,
            ny,
            prof_x - 3.0,
            ny,
            if profile_on {
                Rgb(248, 206, 78)
            } else if done {
                Rgb(52, 168, 178)
            } else {
                Rgb(22, 60, 74)
            },
            0.85,
        );
        if active {
            buf.add_radial_glow(node_x, ny, 5.0, 3.5, Rgb(250, 210, 82), 0.7 * pulse);
        } else if done {
            buf.add_radial_glow(node_x, ny, 4.0, 3.0, Rgb(94, 214, 214), 0.45);
        }
        buf.fill_circle(node_x, ny, 2.0, branch_col, 1.0);

        let prof_col = if profile_on {
            Rgb(250, 212, 86)
        } else if done {
            Rgb(78, 198, 204)
        } else {
            Rgb(24, 62, 76)
        };
        if profile_on {
            buf.add_radial_glow(prof_x, ny, 5.5, 3.5, Rgb(250, 212, 86), 0.65 * pulse);
        }
        buf.fill_circle(prof_x, ny, 2.1, prof_col, 1.0);
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_cinematics_and_probes_render_non_empty_surfaces() {
        let mut buffers = vec![
            ("awakening_p1", build_awakening_buffer(80, 24, 0.12, false)),
            ("awakening_p2", build_awakening_buffer(80, 24, 0.50, false)),
            ("awakening_p3", build_awakening_buffer(80, 24, 0.88, false)),
            (
                "awakening_quick",
                build_awakening_buffer(80, 24, 0.50, true),
            ),
            (
                "coherence_drift",
                build_coherence_buffer(80, 24, [1, 5, 9], 0, 0.25, false),
            ),
            (
                "coherence_lock",
                build_coherence_buffer(80, 24, [4, 4, 4], 1, 0.50, true),
            ),
            (
                "solar_horizon",
                build_solar_horizon_buffer(80, 24, 2.4, 0.35),
            ),
            (
                "research_topology",
                build_research_topology_buffer([true, false, true], None, 1, 64, 6, 0.3),
            ),
        ];
        for id in [
            "observation_001",
            "observation_002",
            "observation_003",
            "observation_004",
            "observation_005",
        ] {
            buffers.push((id, build_discovery_buffer(id, 96, 14, 0.4, 1.0)));
        }
        buffers.push((
            "probe_scout",
            build_probe_schematic_buffer(
                false,
                ExtractorLoadout::Balanced,
                ScoutState::Surveying,
                ExtractorState::Ready,
                100,
                56,
                18,
                0.3,
            ),
        ));
        for loadout in ExtractorLoadout::ALL {
            buffers.push((
                "probe_extractor",
                build_probe_schematic_buffer(
                    true,
                    loadout,
                    ScoutState::Ready,
                    ExtractorState::Extracting,
                    100,
                    56,
                    18,
                    0.3,
                ),
            ));
        }
        buffers.push((
            "probe_rebuilding",
            build_probe_schematic_buffer(
                true,
                ExtractorLoadout::Cargo,
                ScoutState::Ready,
                ExtractorState::Rebuilding,
                45,
                56,
                18,
                0.3,
            ),
        ));

        for (name, buf) in buffers {
            assert!(
                buf.mean_luminance() > 0.01,
                "scene {name} rendered empty surface"
            );
        }
    }
}
