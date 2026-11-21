//! Flat-shaded half-block polygon terrain vistas and local outpost scenes.
//!
//! Renders resolution-independent multi-plane exterior landscapes strictly from player-visible
//! knowledge: unexplored regions display shrouded atmospheric horizon bands and never leak terrain,
//! deposits, or structures.

use crate::raster::{buffer_to_text, draw_poly_norm, ColorTier, PixelBuffer, Rgb};
use mind_core::protocol::{
    AtmosphericSeverity, BaseModuleKind, BaseSnapshot, PlanetSnapshot, RegionKnowledge,
    RegionSnapshot, SubregionClass, SubregionKnowledge, SubregionSnapshot, TemperatureBand,
    TerrainClass,
};
use ratatui::text::Text;

/// Render a high-resolution flat-shaded polygon horizon vista (`width` cols × `height` rows =
/// `width × 2*height` subpixels) for the selected planetary region.
pub fn render_region_vista(
    region: &RegionSnapshot,
    planet: &PlanetSnapshot,
    width: u16,
    height: u16,
    phase: f64,
    tier: ColorTier,
) -> Text<'static> {
    let buf = build_region_vista_buffer(region, planet, width, height, phase);
    buffer_to_text(&buf, tier)
}

pub fn build_region_vista_buffer(
    region: &RegionSnapshot,
    planet: &PlanetSnapshot,
    width: u16,
    height: u16,
    phase: f64,
) -> PixelBuffer {
    let pw = usize::from(width.max(16));
    let ph = usize::from(height.max(4)) * 2;
    let mut buf = PixelBuffer::new(pw, ph, Rgb(2, 16, 26));
    let t = phase as f32;

    if region.knowledge == RegionKnowledge::Unknown || region.terrain.is_none() {
        draw_unknown_horizon(&mut buf, t);
        return buf;
    }

    let terrain = region.terrain.unwrap_or(TerrainClass::Plain);
    let temp = region.temperature.unwrap_or(TemperatureBand::Temperate);
    let atmo = region
        .atmospheric_severity
        .unwrap_or(AtmosphericSeverity::Stable);

    // 1. Atmospheric sky gradient (deep blue / indigo / pale cyan / gold).
    let (sky_top, sky_mid, horizon_glow) = match temp {
        TemperatureBand::Frigid => (Rgb(2, 12, 26), Rgb(10, 34, 58), Rgb(62, 128, 168)),
        TemperatureBand::Temperate => (Rgb(1, 16, 26), Rgb(6, 44, 58), Rgb(42, 148, 152)),
        TemperatureBand::Hot => (Rgb(4, 18, 30), Rgb(14, 48, 62), Rgb(118, 148, 126)),
        _ => (Rgb(2, 14, 28), Rgb(8, 36, 54), Rgb(42, 118, 142)),
    };
    buf.fill_gradient_stops(
        &[
            (0.0, sky_top),
            (0.48, sky_mid),
            (0.76, horizon_glow),
            (1.0, Rgb(4, 20, 28)),
        ],
        true,
    );

    // Subtle stars in the upper sky (clearer in Stable atmosphere).
    let star_count = match atmo {
        AtmosphericSeverity::Stable => (pw / 2).max(14),
        AtmosphericSeverity::Variable => (pw / 4).max(7),
        AtmosphericSeverity::Severe => 4,
    };
    buf.draw_starfield(
        region.position.latitude_band as u64 * 31 + region.position.longitude_sector as u64 * 17,
        star_count,
        ph / 2,
        t,
        Rgb(54, 102, 120),
        Rgb(178, 226, 232),
    );

    // Soft distant horizon glow behind the ridge silhouette.
    buf.add_radial_glow(
        pw as f32 * 0.52,
        ph as f32 * 0.66,
        pw as f32 * 0.55,
        ph as f32 * 0.28,
        Rgb(58, 178, 186),
        0.38,
    );

    // 2. Three flat-shaded parallax polygon terrain planes (flat silhouette grammar).
    let (far_col, mid_col, near_col, accent_col) = match terrain {
        TerrainClass::Polar => (
            Rgb(24, 62, 86),
            Rgb(42, 96, 122),
            Rgb(18, 48, 68),
            Rgb(146, 206, 224),
        ),
        TerrainClass::Basin => (
            Rgb(14, 48, 64),
            Rgb(20, 74, 88),
            Rgb(8, 36, 48),
            Rgb(68, 184, 192),
        ),
        TerrainClass::Plain => (
            Rgb(14, 52, 64),
            Rgb(24, 82, 92),
            Rgb(10, 40, 50),
            Rgb(78, 188, 184),
        ),
        TerrainClass::Highland => (
            Rgb(18, 52, 70),
            Rgb(32, 88, 104),
            Rgb(12, 42, 54),
            Rgb(104, 196, 194),
        ),
        TerrainClass::Ridge => (
            Rgb(22, 54, 74),
            Rgb(40, 92, 108),
            Rgb(14, 38, 52),
            Rgb(142, 198, 190),
        ),
        TerrainClass::Fractured => (
            Rgb(20, 48, 68),
            Rgb(38, 78, 96),
            Rgb(12, 34, 48),
            Rgb(166, 188, 168),
        ),
    };

    draw_terrain_layers(&mut buf, terrain, far_col, mid_col, near_col, accent_col);

    // 3. Atmospheric motion (drifting wisps or diagonal weather streaks).
    match atmo {
        AtmosphericSeverity::Variable => {
            for i in 0..5 {
                let speed = 0.08 + (i as f32) * 0.025;
                let x = (((t * speed + i as f32 * 0.23) % 1.2) - 0.1) * pw as f32;
                let y = (0.18 + (i as f32) * 0.09) * ph as f32;
                buf.stroke_line(x, y, x + 5.0, y, Rgb(94, 168, 186), 0.35);
            }
        }
        AtmosphericSeverity::Severe => {
            for i in 0..10 {
                let x = (((t * 0.35 + i as f32 * 0.13) % 1.2) - 0.1) * pw as f32;
                let y = (0.12 + ((i * 7) % 5) as f32 * 0.11) * ph as f32;
                buf.stroke_line(x, y, x - 3.5, y + 3.0, Rgb(146, 206, 224), 0.48);
            }
        }
        AtmosphericSeverity::Stable => {}
    }

    // 4. Diegetic surface presence silhouettes (Core, Relay, Outpost, Active Probes).
    let has_base = planet
        .network
        .bases
        .iter()
        .any(|base| base.region_id == region.id);
    let scout_here = planet.scout.region_id == region.id
        || planet
            .active_mission
            .as_ref()
            .is_some_and(|m| m.target_region_id == region.id);
    let extractor_here = planet
        .active_extraction
        .as_ref()
        .is_some_and(|m| m.target_region_id == region.id);

    if region.core {
        draw_core_monolith(&mut buf, t);
    } else if has_base {
        draw_horizon_outpost(&mut buf, t);
    } else if region.relay {
        draw_relay_spire(&mut buf, t);
    }

    if let Some(deposit) = &region.deposit {
        if deposit.remaining > 0 {
            let glint = ((t * std::f32::consts::TAU * 1.4).sin() * 0.5 + 0.5) * 0.65 + 0.35;
            buf.add_radial_glow(
                pw as f32 * 0.78,
                ph as f32 * 0.84,
                3.5,
                2.5,
                Rgb(242, 198, 72),
                glint * 0.75,
            );
        }
    }

    if scout_here {
        draw_horizon_scout(&mut buf, t, 0.24, 0.30);
    }
    if extractor_here {
        draw_horizon_extractor(&mut buf, t, 0.72, 0.34);
    }

    buf
}

fn draw_unknown_horizon(buf: &mut PixelBuffer, t: f32) {
    let pw = buf.width as f32;
    let ph = buf.height as f32;
    buf.fill_gradient_stops(
        &[
            (0.0, Rgb(1, 10, 18)),
            (0.55, Rgb(4, 24, 36)),
            (0.76, Rgb(10, 44, 58)),
            (1.0, Rgb(2, 16, 24)),
        ],
        true,
    );
    buf.draw_starfield(
        991,
        (buf.width / 3).max(8),
        buf.height / 2,
        t,
        Rgb(34, 68, 84),
        Rgb(96, 152, 172),
    );
    // Shrouded flat horizon band (reveals zero underlying terrain truth).
    draw_poly_norm(
        buf,
        &[(0.0, 0.70), (1.0, 0.70), (1.0, 1.0), (0.0, 1.0)],
        Rgb(5, 24, 34),
        1.0,
        0.0,
        0.0,
        1.0,
    );
    // Gentle horizontal acquisition sweep line across the fogged horizon.
    let sweep_x = (t * 0.55).rem_euclid(1.0) * pw;
    buf.add_radial_glow(sweep_x, ph * 0.68, pw * 0.14, 2.2, Rgb(62, 188, 196), 0.55);
    buf.stroke_line(0.0, ph * 0.70, pw, ph * 0.70, Rgb(34, 108, 122), 0.7);
}

fn draw_terrain_layers(
    buf: &mut PixelBuffer,
    terrain: TerrainClass,
    far_col: Rgb,
    mid_col: Rgb,
    near_col: Rgb,
    accent: Rgb,
) {
    match terrain {
        TerrainClass::Basin => {
            draw_poly_norm(
                buf,
                &[
                    (0.0, 0.46),
                    (0.18, 0.56),
                    (0.36, 0.68),
                    (0.64, 0.68),
                    (0.82, 0.54),
                    (1.0, 0.44),
                    (1.0, 1.0),
                    (0.0, 1.0),
                ],
                far_col,
                1.0,
                0.0,
                0.0,
                1.0,
            );
            draw_poly_norm(
                buf,
                &[
                    (0.0, 0.58),
                    (0.24, 0.74),
                    (0.76, 0.74),
                    (1.0, 0.56),
                    (1.0, 1.0),
                    (0.0, 1.0),
                ],
                mid_col,
                1.0,
                0.0,
                0.0,
                1.0,
            );
            draw_poly_norm(
                buf,
                &[
                    (0.0, 0.76),
                    (0.30, 0.84),
                    (0.70, 0.84),
                    (1.0, 0.74),
                    (1.0, 1.0),
                    (0.0, 1.0),
                ],
                near_col,
                1.0,
                0.0,
                0.0,
                1.0,
            );
            buf.stroke_line(
                buf.width as f32 * 0.25,
                buf.height as f32 * 0.75,
                buf.width as f32 * 0.75,
                buf.height as f32 * 0.75,
                accent,
                0.55,
            );
        }
        TerrainClass::Plain => {
            draw_poly_norm(
                buf,
                &[
                    (0.0, 0.64),
                    (0.22, 0.58),
                    (0.42, 0.63),
                    (0.74, 0.56),
                    (1.0, 0.64),
                    (1.0, 1.0),
                    (0.0, 1.0),
                ],
                far_col,
                1.0,
                0.0,
                0.0,
                1.0,
            );
            draw_poly_norm(
                buf,
                &[
                    (0.0, 0.72),
                    (0.35, 0.69),
                    (0.68, 0.73),
                    (1.0, 0.68),
                    (1.0, 1.0),
                    (0.0, 1.0),
                ],
                mid_col,
                1.0,
                0.0,
                0.0,
                1.0,
            );
            draw_poly_norm(
                buf,
                &[
                    (0.0, 0.82),
                    (0.50, 0.79),
                    (1.0, 0.84),
                    (1.0, 1.0),
                    (0.0, 1.0),
                ],
                near_col,
                1.0,
                0.0,
                0.0,
                1.0,
            );
        }
        TerrainClass::Highland => {
            draw_poly_norm(
                buf,
                &[
                    (0.0, 0.62),
                    (0.14, 0.42),
                    (0.34, 0.42),
                    (0.46, 0.60),
                    (0.62, 0.46),
                    (0.84, 0.46),
                    (1.0, 0.62),
                    (1.0, 1.0),
                    (0.0, 1.0),
                ],
                far_col,
                1.0,
                0.0,
                0.0,
                1.0,
            );
            draw_poly_norm(
                buf,
                &[
                    (0.0, 0.74),
                    (0.20, 0.58),
                    (0.48, 0.58),
                    (0.60, 0.74),
                    (1.0, 0.68),
                    (1.0, 1.0),
                    (0.0, 1.0),
                ],
                mid_col,
                1.0,
                0.0,
                0.0,
                1.0,
            );
            // Rim light along mesa crown.
            buf.stroke_line(
                buf.width as f32 * 0.20,
                buf.height as f32 * 0.58,
                buf.width as f32 * 0.48,
                buf.height as f32 * 0.58,
                accent,
                0.65,
            );
            draw_poly_norm(
                buf,
                &[
                    (0.0, 0.80),
                    (0.35, 0.86),
                    (0.78, 0.78),
                    (1.0, 0.82),
                    (1.0, 1.0),
                    (0.0, 1.0),
                ],
                near_col,
                1.0,
                0.0,
                0.0,
                1.0,
            );
        }
        TerrainClass::Ridge => {
            draw_poly_norm(
                buf,
                &[
                    (0.0, 0.66),
                    (0.16, 0.36),
                    (0.30, 0.58),
                    (0.50, 0.28),
                    (0.68, 0.56),
                    (0.86, 0.38),
                    (1.0, 0.64),
                    (1.0, 1.0),
                    (0.0, 1.0),
                ],
                far_col,
                1.0,
                0.0,
                0.0,
                1.0,
            );
            draw_poly_norm(
                buf,
                &[
                    (0.0, 0.76),
                    (0.24, 0.48),
                    (0.40, 0.70),
                    (0.66, 0.44),
                    (0.88, 0.72),
                    (1.0, 0.66),
                    (1.0, 1.0),
                    (0.0, 1.0),
                ],
                mid_col,
                1.0,
                0.0,
                0.0,
                1.0,
            );
            // Lit sunlit ridge facets.
            draw_poly_norm(
                buf,
                &[(0.50, 0.28), (0.55, 0.42), (0.50, 0.46)],
                accent,
                0.65,
                0.0,
                0.0,
                1.0,
            );
            draw_poly_norm(
                buf,
                &[
                    (0.0, 0.84),
                    (0.32, 0.76),
                    (0.70, 0.86),
                    (1.0, 0.78),
                    (1.0, 1.0),
                    (0.0, 1.0),
                ],
                near_col,
                1.0,
                0.0,
                0.0,
                1.0,
            );
        }
        TerrainClass::Fractured => {
            draw_poly_norm(
                buf,
                &[
                    (0.0, 0.62),
                    (0.14, 0.42),
                    (0.22, 0.66),
                    (0.36, 0.36),
                    (0.46, 0.64),
                    (0.62, 0.34),
                    (0.74, 0.62),
                    (0.88, 0.40),
                    (1.0, 0.64),
                    (1.0, 1.0),
                    (0.0, 1.0),
                ],
                far_col,
                1.0,
                0.0,
                0.0,
                1.0,
            );
            draw_poly_norm(
                buf,
                &[
                    (0.0, 0.74),
                    (0.18, 0.56),
                    (0.32, 0.78),
                    (0.54, 0.52),
                    (0.70, 0.78),
                    (0.90, 0.58),
                    (1.0, 0.76),
                    (1.0, 1.0),
                    (0.0, 1.0),
                ],
                mid_col,
                1.0,
                0.0,
                0.0,
                1.0,
            );
            draw_poly_norm(
                buf,
                &[
                    (0.0, 0.86),
                    (0.42, 0.82),
                    (0.58, 0.90),
                    (1.0, 0.82),
                    (1.0, 1.0),
                    (0.0, 1.0),
                ],
                near_col,
                1.0,
                0.0,
                0.0,
                1.0,
            );
        }
        TerrainClass::Polar => {
            draw_poly_norm(
                buf,
                &[
                    (0.0, 0.60),
                    (0.20, 0.44),
                    (0.42, 0.52),
                    (0.68, 0.38),
                    (0.90, 0.50),
                    (1.0, 0.58),
                    (1.0, 1.0),
                    (0.0, 1.0),
                ],
                far_col,
                1.0,
                0.0,
                0.0,
                1.0,
            );
            draw_poly_norm(
                buf,
                &[
                    (0.0, 0.72),
                    (0.28, 0.60),
                    (0.62, 0.66),
                    (0.84, 0.56),
                    (1.0, 0.70),
                    (1.0, 1.0),
                    (0.0, 1.0),
                ],
                mid_col,
                1.0,
                0.0,
                0.0,
                1.0,
            );
            // Crisp ice-sheet rim highlight.
            draw_poly_norm(
                buf,
                &[(0.20, 0.44), (0.28, 0.47), (0.22, 0.52)],
                accent,
                0.75,
                0.0,
                0.0,
                1.0,
            );
            draw_poly_norm(
                buf,
                &[
                    (0.0, 0.82),
                    (0.45, 0.78),
                    (1.0, 0.84),
                    (1.0, 1.0),
                    (0.0, 1.0),
                ],
                near_col,
                1.0,
                0.0,
                0.0,
                1.0,
            );
        }
    }
}

fn draw_core_monolith(buf: &mut PixelBuffer, t: f32) {
    // Sleek faceted exterior Core dome & telemetry spire seated on the central terrain.
    draw_poly_norm(
        buf,
        &[(0.42, 0.84), (0.46, 0.56), (0.54, 0.56), (0.58, 0.84)],
        Rgb(16, 62, 82),
        1.0,
        0.0,
        0.0,
        1.0,
    );
    draw_poly_norm(
        buf,
        &[(0.46, 0.56), (0.50, 0.44), (0.54, 0.56)],
        Rgb(64, 188, 198),
        1.0,
        0.0,
        0.0,
        1.0,
    );
    let cx = buf.width as f32 * 0.50;
    let top_y = buf.height as f32 * 0.28;
    buf.stroke_line(
        cx,
        buf.height as f32 * 0.44,
        cx,
        top_y,
        Rgb(134, 224, 226),
        0.95,
    );
    let pulse = (t * std::f32::consts::TAU * 1.2).sin() * 0.3 + 0.7;
    buf.add_radial_glow(cx, top_y, 3.8, 3.0, Rgb(250, 206, 76), pulse * 0.85);
}

fn draw_relay_spire(buf: &mut PixelBuffer, t: f32) {
    let cx = buf.width as f32 * 0.50;
    let base_y = buf.height as f32 * 0.82;
    let tip_y = buf.height as f32 * 0.30;
    draw_poly_norm(
        buf,
        &[(0.47, 0.84), (0.495, 0.44), (0.505, 0.44), (0.53, 0.84)],
        Rgb(26, 98, 114),
        1.0,
        0.0,
        0.0,
        1.0,
    );
    buf.stroke_line(cx, base_y, cx, tip_y, Rgb(94, 214, 214), 0.9);
    let pulse = (t * std::f32::consts::TAU * 1.5).sin() * 0.35 + 0.65;
    buf.add_radial_glow(cx, tip_y, 3.2, 2.6, Rgb(94, 214, 214), pulse * 0.85);
}

fn draw_horizon_outpost(buf: &mut PixelBuffer, t: f32) {
    draw_poly_norm(
        buf,
        &[(0.38, 0.84), (0.42, 0.66), (0.50, 0.66), (0.54, 0.84)],
        Rgb(34, 128, 138),
        1.0,
        0.0,
        0.0,
        1.0,
    );
    draw_poly_norm(
        buf,
        &[(0.54, 0.84), (0.57, 0.70), (0.64, 0.70), (0.66, 0.84)],
        Rgb(24, 92, 112),
        1.0,
        0.0,
        0.0,
        1.0,
    );
    let cx = buf.width as f32 * 0.46;
    let cy = buf.height as f32 * 0.58;
    let pulse = (t * std::f32::consts::TAU).sin() * 0.3 + 0.7;
    buf.add_radial_glow(cx, cy, 3.0, 2.4, Rgb(94, 214, 214), pulse * 0.75);
}

fn draw_horizon_scout(buf: &mut PixelBuffer, t: f32, nx: f32, ny: f32) {
    let bob = (t * std::f32::consts::TAU).sin() * 0.02;
    let cx = buf.width as f32 * nx;
    let cy = buf.height as f32 * (ny + bob);
    let scale = (buf.height as f32 / 14.0).clamp(0.45, 1.0);
    if buf.height <= 10 {
        buf.set(
            cx.round() as i32,
            cy.round() as i32,
            Rgb(118, 232, 232),
            1.0,
        );
        buf.add_radial_glow(cx, cy, 2.2, 1.4, Rgb(94, 214, 214), 0.55);
        return;
    }
    buf.fill_polygon(
        &[
            (cx, cy - 1.2 * scale),
            (cx + 2.4 * scale, cy + 0.3 * scale),
            (cx, cy + 1.0 * scale),
            (cx - 2.4 * scale, cy + 0.3 * scale),
        ],
        Rgb(106, 224, 226),
        1.0,
    );
    buf.add_radial_glow(cx, cy + 1.2 * scale, 2.2, 1.6, Rgb(248, 204, 72), 0.45);
}

fn draw_horizon_extractor(buf: &mut PixelBuffer, t: f32, nx: f32, ny: f32) {
    let bob = (t * std::f32::consts::TAU * 0.8 + 1.2).sin() * 0.02;
    let cx = buf.width as f32 * nx;
    let cy = buf.height as f32 * (ny + bob);
    let scale = (buf.height as f32 / 14.0).clamp(0.45, 1.0);
    if buf.height <= 10 {
        buf.set(cx.round() as i32, cy.round() as i32, Rgb(248, 204, 72), 1.0);
        buf.add_radial_glow(cx, cy, 2.2, 1.4, Rgb(236, 184, 58), 0.55);
        return;
    }
    buf.fill_polygon(
        &[
            (cx - 2.0 * scale, cy - 1.0 * scale),
            (cx + 2.0 * scale, cy - 1.0 * scale),
            (cx + 2.4 * scale, cy + 0.9 * scale),
            (cx, cy + 1.8 * scale),
            (cx - 2.4 * scale, cy + 0.9 * scale),
        ],
        Rgb(236, 184, 58),
        1.0,
    );
    buf.add_radial_glow(cx, cy + 1.8 * scale, 2.2, 1.6, Rgb(94, 214, 214), 0.40);
}

/// Render a high-resolution local sector & outpost polygon vista for the Local Field inspector.
pub fn render_local_vista(
    site: &SubregionSnapshot,
    base: Option<&BaseSnapshot>,
    active_survey: bool,
    width: u16,
    height: u16,
    phase: f64,
    tier: ColorTier,
) -> Text<'static> {
    let pw = usize::from(width.max(16));
    let ph = usize::from(height.max(4)) * 2;
    let mut buf = PixelBuffer::new(pw, ph, Rgb(2, 16, 26));
    let t = phase as f32;

    if site.knowledge == SubregionKnowledge::Unknown {
        draw_unknown_horizon(&mut buf, t);
        return buffer_to_text(&buf, tier);
    }

    buf.fill_gradient_stops(
        &[
            (0.0, Rgb(2, 14, 26)),
            (0.55, Rgb(8, 38, 54)),
            (0.76, Rgb(34, 118, 134)),
            (1.0, Rgb(6, 26, 36)),
        ],
        true,
    );
    buf.draw_starfield(
        site.slot as u64 * 53 + 11,
        (pw / 3).max(8),
        ph / 2,
        t,
        Rgb(48, 92, 110),
        Rgb(168, 220, 228),
    );

    let mapped_terrain = match site.class {
        Some(SubregionClass::ShelteredShelf) => TerrainClass::Highland,
        Some(SubregionClass::BasinFloor) => TerrainClass::Basin,
        Some(SubregionClass::OpenPlain) => TerrainClass::Plain,
        Some(SubregionClass::ExposedRidge) => TerrainClass::Ridge,
        Some(SubregionClass::FractureField) => TerrainClass::Fractured,
        Some(SubregionClass::PolarMargin) => TerrainClass::Polar,
        None => TerrainClass::Plain,
    };
    draw_terrain_layers(
        &mut buf,
        mapped_terrain,
        Rgb(16, 52, 68),
        Rgb(26, 84, 98),
        Rgb(10, 38, 50),
        Rgb(84, 194, 198),
    );

    if active_survey {
        let sweep_x = (t * 0.9).rem_euclid(1.0) * pw as f32;
        buf.stroke_line(sweep_x, 0.0, sweep_x, ph as f32, Rgb(250, 206, 76), 0.75);
        buf.add_radial_glow(sweep_x, ph as f32 * 0.65, 4.0, 4.0, Rgb(94, 214, 214), 0.65);
    }

    if let Some(base) = base {
        draw_local_base_modules(&mut buf, base, t);
    }

    buffer_to_text(&buf, tier)
}

fn draw_local_base_modules(buf: &mut PixelBuffer, base: &BaseSnapshot, t: f32) {
    let ground_y = 0.84_f32;
    let slots = [
        (BaseModuleKind::PowerArray, 0.14_f32),
        (BaseModuleKind::Communications, 0.28_f32),
        (BaseModuleKind::Depot, 0.44_f32),
        (BaseModuleKind::Workshop, 0.58_f32),
        (BaseModuleKind::Laboratory, 0.73_f32),
        (BaseModuleKind::Observatory, 0.87_f32),
    ];
    // Grounded foundation terrace.
    draw_poly_norm(
        buf,
        &[
            (0.08, ground_y),
            (0.92, ground_y),
            (0.95, 0.96),
            (0.05, 0.96),
        ],
        if base.connected && base.integrity > 20 {
            Rgb(22, 74, 94)
        } else {
            Rgb(92, 74, 34)
        },
        1.0,
        0.0,
        0.0,
        1.0,
    );
    let pulse = (t * std::f32::consts::TAU * 1.4).sin() * 0.25 + 0.75;
    for (kind, cx) in slots {
        let Some(module) = base.modules.iter().find(|m| m.kind == kind) else {
            continue;
        };
        let col = if !module.enabled {
            Rgb(56, 78, 88)
        } else if module.active {
            Rgb(78, 204, 212)
        } else {
            Rgb(218, 165, 32)
        };
        match kind {
            BaseModuleKind::PowerArray => {
                draw_poly_norm(
                    buf,
                    &[
                        (cx - 0.05, ground_y),
                        (cx - 0.03, 0.56),
                        (cx + 0.05, 0.50),
                        (cx + 0.04, ground_y),
                    ],
                    col,
                    1.0,
                    0.0,
                    0.0,
                    1.0,
                );
            }
            BaseModuleKind::Communications => {
                let px = cx * buf.width as f32;
                buf.stroke_line(
                    px,
                    ground_y * buf.height as f32,
                    px,
                    0.34 * buf.height as f32,
                    col,
                    1.0,
                );
                if module.active {
                    buf.add_radial_glow(
                        px,
                        0.34 * buf.height as f32,
                        3.0,
                        2.5,
                        Rgb(94, 214, 214),
                        pulse * 0.8,
                    );
                }
            }
            BaseModuleKind::Depot => {
                draw_poly_norm(
                    buf,
                    &[
                        (cx - 0.06, ground_y),
                        (cx - 0.05, 0.60),
                        (cx + 0.05, 0.60),
                        (cx + 0.06, ground_y),
                    ],
                    col,
                    1.0,
                    0.0,
                    0.0,
                    1.0,
                );
            }
            BaseModuleKind::Workshop => {
                draw_poly_norm(
                    buf,
                    &[
                        (cx - 0.05, ground_y),
                        (cx - 0.04, 0.62),
                        (cx + 0.02, 0.56),
                        (cx + 0.05, ground_y),
                    ],
                    col,
                    1.0,
                    0.0,
                    0.0,
                    1.0,
                );
            }
            BaseModuleKind::Laboratory => {
                draw_poly_norm(
                    buf,
                    &[
                        (cx - 0.06, ground_y),
                        (cx - 0.04, 0.58),
                        (cx, 0.50),
                        (cx + 0.04, 0.58),
                        (cx + 0.06, ground_y),
                    ],
                    col,
                    1.0,
                    0.0,
                    0.0,
                    1.0,
                );
            }
            BaseModuleKind::Observatory => {
                draw_poly_norm(
                    buf,
                    &[
                        (cx - 0.05, ground_y),
                        (cx - 0.03, 0.60),
                        (cx + 0.03, 0.60),
                        (cx + 0.05, ground_y),
                    ],
                    col,
                    1.0,
                    0.0,
                    0.0,
                    1.0,
                );
                let px = cx * buf.width as f32;
                let py = 0.52 * buf.height as f32;
                buf.stroke_line(
                    px - 3.0,
                    py - 2.0,
                    px + 3.0,
                    py + 1.0,
                    Rgb(250, 206, 76),
                    0.95,
                );
                if module.active {
                    buf.add_radial_glow(px, py - 1.0, 3.2, 2.5, Rgb(250, 206, 76), pulse * 0.75);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_region_vistas_render_non_empty_surfaces() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([2, 2, 2]);
        let planet = engine.planet().unwrap();
        let mut region = planet.regions[0].clone();

        // Unknown horizon
        region.knowledge = RegionKnowledge::Unknown;
        region.terrain = None;
        let unknown = build_region_vista_buffer(&region, &planet, 48, 8, 0.25);
        assert!(unknown.mean_luminance() > 0.01);

        for terrain in [
            TerrainClass::Basin,
            TerrainClass::Plain,
            TerrainClass::Highland,
            TerrainClass::Ridge,
            TerrainClass::Fractured,
            TerrainClass::Polar,
        ] {
            region.knowledge = RegionKnowledge::Surveyed;
            region.terrain = Some(terrain);
            let buf = build_region_vista_buffer(&region, &planet, 48, 8, 0.4);
            assert!(buf.mean_luminance() > 0.02, "terrain {terrain:?}");
        }
    }
}
