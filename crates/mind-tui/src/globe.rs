use crate::theme::{Theme, AMBER_RGB, CYAN_RGB};
use mind_core::protocol::{
    PlanetSnapshot, RegionKnowledge, RegionSnapshot, ScoutState, TerrainClass,
};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
};
use std::{
    borrow::Cow,
    f64::consts::{FRAC_PI_2, FRAC_PI_4, PI, TAU},
};

const LONGITUDE_STEP: f64 = PI / 3.0;
const REGION_EDGE_WIDTH: f64 = 0.022;

#[derive(Debug, Clone, Copy)]
pub struct GlobeRenderOptions {
    pub width: u16,
    pub height: u16,
    pub yaw: f64,
    pub pitch: f64,
    pub phase: f64,
    pub ascii: bool,
    pub no_color: bool,
    pub theme: Theme,
}

#[derive(Debug, Clone, Copy)]
struct SurfaceSample {
    color: Rgb,
    luminance: f64,
    selected: bool,
    selected_edge: bool,
    region_edge: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rgb(u8, u8, u8);

impl Rgb {
    fn scale(self, factor: f64) -> Self {
        Self(
            (f64::from(self.0) * factor).clamp(0.0, 255.0) as u8,
            (f64::from(self.1) * factor).clamp(0.0, 255.0) as u8,
            (f64::from(self.2) * factor).clamp(0.0, 255.0) as u8,
        )
    }

    fn mix(self, other: Self, amount: f64) -> Self {
        let amount = amount.clamp(0.0, 1.0);
        let keep = 1.0 - amount;
        Self(
            (f64::from(self.0) * keep + f64::from(other.0) * amount) as u8,
            (f64::from(self.1) * keep + f64::from(other.1) * amount) as u8,
            (f64::from(self.2) * keep + f64::from(other.2) * amount) as u8,
        )
    }

    fn color(self) -> Color {
        Color::Rgb(quantize(self.0), quantize(self.1), quantize(self.2))
    }
}

fn quantize(channel: u8) -> u8 {
    channel.saturating_add(4) / 8 * 8
}

#[derive(Debug, Clone, Copy)]
struct ProjectedPoint {
    x: f64,
    y: f64,
    depth: f64,
}

#[derive(Debug, Clone)]
struct Cell {
    symbol: Cow<'static, str>,
    style: Style,
}

impl Cell {
    fn blank() -> Self {
        Self {
            symbol: Cow::Borrowed(" "),
            style: Style::default(),
        }
    }
}

pub fn render(
    planet: &PlanetSnapshot,
    selected_id: Option<&str>,
    options: GlobeRenderOptions,
) -> Text<'static> {
    let width = usize::from(options.width.max(1));
    let height = usize::from(options.height.max(1));
    let subpixel_height = height * 2;
    let radius = ((width.saturating_sub(4)) as f64 / 2.0)
        .min((subpixel_height.saturating_sub(2)) as f64 / 2.0)
        .max(2.0);
    let center_x = (width.saturating_sub(1)) as f64 / 2.0;
    let center_y = (subpixel_height.saturating_sub(1)) as f64 / 2.0;
    let rich_unicode = !options.ascii && !options.no_color;

    let mut cells = (0..height)
        .map(|y| {
            (0..width)
                .map(|x| {
                    let top = sample_surface(
                        planet,
                        selected_id,
                        x,
                        y * 2,
                        center_x,
                        center_y,
                        radius,
                        options.yaw,
                        options.pitch,
                    );
                    let bottom = sample_surface(
                        planet,
                        selected_id,
                        x,
                        y * 2 + 1,
                        center_x,
                        center_y,
                        radius,
                        options.yaw,
                        options.pitch,
                    );
                    if rich_unicode {
                        rich_cell(top, bottom, x, y, options.theme)
                    } else {
                        fallback_cell(
                            top,
                            bottom,
                            options.ascii,
                            options.no_color,
                            x,
                            y,
                            options.theme,
                        )
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    draw_mission_path(&mut cells, planet, options, center_x, center_y, radius);
    draw_extraction_path(&mut cells, planet, options, center_x, center_y, radius);
    draw_construction_path(&mut cells, planet, options, center_x, center_y, radius);
    draw_relay_pulse(&mut cells, planet, options, center_x, center_y, radius);
    draw_region_markers(
        &mut cells,
        planet,
        selected_id,
        options,
        center_x,
        center_y,
        radius,
    );

    Text::from(
        cells
            .into_iter()
            .map(|row| {
                Line::from(
                    row.into_iter()
                        .map(|cell| Span::styled(cell.symbol, cell.style))
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>(),
    )
}

#[allow(clippy::too_many_arguments)]
fn sample_surface(
    planet: &PlanetSnapshot,
    selected_id: Option<&str>,
    x: usize,
    y: usize,
    center_x: f64,
    center_y: f64,
    radius: f64,
    yaw: f64,
    pitch: f64,
) -> Option<SurfaceSample> {
    let screen_x = (x as f64 - center_x) / radius;
    let screen_y = (center_y - y as f64) / radius;
    let radius_squared = screen_x.mul_add(screen_x, screen_y * screen_y);
    if radius_squared > 1.0 {
        return None;
    }

    let depth = (1.0 - radius_squared).sqrt();
    let sin_pitch = pitch.sin();
    let cos_pitch = pitch.cos();
    let latitude = (depth.mul_add(sin_pitch, screen_y * cos_pitch))
        .clamp(-1.0, 1.0)
        .asin();
    let longitude =
        normalize_angle(yaw + screen_x.atan2(depth.mul_add(cos_pitch, -screen_y * sin_pitch)));
    let region = region_at(planet, latitude, longitude)?;
    let selected = selected_id.is_some_and(|id| region.id == id);
    let region_edge = near_region_edge(latitude, longitude, radius);
    let selected_edge = selected && region_edge;

    let light = normalized3((-0.38, 0.46, 0.80));
    let diffuse = (screen_x * light.0 + screen_y * light.1 + depth * light.2).max(0.0);
    let limb = depth.powf(0.38);
    let mut luminance = (0.18 + diffuse * 0.70 + limb * 0.12).clamp(0.10, 1.0);
    luminance += terrain_texture(region, latitude, longitude);
    if region_edge {
        luminance *= 0.78;
    }
    if selected {
        luminance = (luminance + 0.045).min(1.0);
    }

    let mut color = knowledge_color(region.knowledge);
    if let Some(terrain) = region.terrain {
        color = color.mix(terrain_tint(terrain), 0.14);
    }
    color = color.scale(luminance);
    if radius_squared > 0.93 {
        color = color.mix(Rgb::from(CYAN_RGB), ((radius_squared - 0.93) / 0.07) * 0.38);
    }
    if selected_edge {
        color = Rgb::from(AMBER_RGB).scale(0.68 + diffuse * 0.32);
        luminance = 0.86;
    }

    Some(SurfaceSample {
        color,
        luminance: luminance.clamp(0.0, 1.0),
        selected,
        selected_edge,
        region_edge,
    })
}

fn rich_cell(
    top: Option<SurfaceSample>,
    bottom: Option<SurfaceSample>,
    x: usize,
    y: usize,
    theme: Theme,
) -> Cell {
    match (top, bottom) {
        (Some(top), Some(bottom)) => Cell {
            symbol: Cow::Borrowed("▀"),
            style: Style::default()
                .fg(top.color.color())
                .bg(bottom.color.color()),
        },
        (Some(top), None) => Cell {
            symbol: Cow::Borrowed("▀"),
            style: Style::default().fg(top.color.color()),
        },
        (None, Some(bottom)) => Cell {
            symbol: Cow::Borrowed("▄"),
            style: Style::default().fg(bottom.color.color()),
        },
        (None, None) => star_cell(x, y, false, theme),
    }
}

fn fallback_cell(
    top: Option<SurfaceSample>,
    bottom: Option<SurfaceSample>,
    ascii: bool,
    no_color: bool,
    x: usize,
    y: usize,
    theme: Theme,
) -> Cell {
    let sample = match (top, bottom) {
        (Some(top), Some(bottom)) => Some(if top.luminance >= bottom.luminance {
            top
        } else {
            bottom
        }),
        (Some(sample), None) | (None, Some(sample)) => Some(sample),
        (None, None) => None,
    };
    let Some(sample) = sample else {
        return star_cell(x, y, ascii, theme);
    };
    let symbol = if sample.selected_edge {
        if ascii {
            "+"
        } else {
            "▓"
        }
    } else if sample.region_edge {
        if ascii {
            ":"
        } else {
            "░"
        }
    } else {
        let ramps = if ascii {
            [".", ":", "-", "=", "+", "*", "#", "%", "@"]
        } else {
            ["·", "·", "░", "░", "▒", "▒", "▓", "▓", "█"]
        };
        ramps[((sample.luminance * (ramps.len() - 1) as f64).round() as usize).min(ramps.len() - 1)]
    };
    let style = if no_color {
        if sample.selected || sample.selected_edge {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        }
    } else if sample.selected || sample.selected_edge {
        Style::default()
            .fg(theme.amber)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(sample.color.color())
    };
    Cell {
        symbol: Cow::Borrowed(symbol),
        style,
    }
}

fn star_cell(x: usize, y: usize, ascii: bool, theme: Theme) -> Cell {
    let hash = x
        .wrapping_mul(0x9e37)
        .wrapping_add(y.wrapping_mul(0x85eb))
        .rotate_left(7);
    if hash % 89 == 3 {
        Cell {
            symbol: Cow::Borrowed(if ascii { "." } else { "·" }),
            style: Style::default().fg(theme.faint),
        }
    } else {
        Cell::blank()
    }
}

fn draw_region_markers(
    cells: &mut [Vec<Cell>],
    planet: &PlanetSnapshot,
    selected_id: Option<&str>,
    options: GlobeRenderOptions,
    center_x: f64,
    center_y: f64,
    radius: f64,
) {
    let selected = selected_id.and_then(|id| find_region(planet, id));
    let scout_region = find_region(planet, &planet.scout.region_id);
    let target = planet
        .active_mission
        .as_ref()
        .and_then(|mission| find_region(planet, &mission.target_region_id));
    let extraction_target = planet
        .active_extraction
        .as_ref()
        .and_then(|mission| find_region(planet, &mission.target_region_id));
    let construction_target = planet
        .active_construction
        .as_ref()
        .and_then(|construction| find_region(planet, &construction.region_id));
    let relay = planet
        .relay_region_id
        .as_ref()
        .and_then(|id| find_region(planet, id));

    if let Some(core) = planet.regions.iter().find(|region| region.core) {
        place_marker(
            cells,
            core,
            "◇",
            "o",
            options.theme.blue,
            false,
            options,
            center_x,
            center_y,
            radius,
        );
    }
    if let Some(relay) = relay {
        place_marker(
            cells,
            relay,
            if (options.phase * 3.0).floor() as i64 % 2 == 0 {
                "◈"
            } else {
                "◇"
            },
            "R",
            options.theme.success,
            false,
            options,
            center_x,
            center_y,
            radius,
        );
    }
    if let Some(target) = construction_target {
        place_marker(
            cells,
            target,
            if (options.phase * 4.0).floor() as i64 % 2 == 0 {
                "✦"
            } else {
                "·"
            },
            "B",
            options.theme.amber,
            false,
            options,
            center_x,
            center_y,
            radius,
        );
    }
    if let Some(target) = extraction_target {
        place_marker(
            cells,
            target,
            "▣",
            "X",
            options.theme.warning,
            false,
            options,
            center_x,
            center_y,
            radius,
        );
    }
    if let Some(target) = target {
        place_marker(
            cells,
            target,
            if (options.phase * 2.0).floor() as i64 % 2 == 0 {
                "◎"
            } else {
                "◉"
            },
            "T",
            options.theme.amber,
            false,
            options,
            center_x,
            center_y,
            radius,
        );
    }
    if let Some(scout) = scout_region {
        place_marker(
            cells,
            scout,
            "●",
            "S",
            options.theme.cyan_bright,
            false,
            options,
            center_x,
            center_y,
            radius,
        );
    }
    if let Some(selected) = selected {
        place_marker(
            cells,
            selected,
            "◆",
            "@",
            options.theme.amber,
            true,
            options,
            center_x,
            center_y,
            radius,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn place_marker(
    cells: &mut [Vec<Cell>],
    region: &RegionSnapshot,
    unicode_symbol: &'static str,
    ascii_symbol: &'static str,
    color: Color,
    show_label: bool,
    options: GlobeRenderOptions,
    center_x: f64,
    center_y: f64,
    radius: f64,
) {
    let (latitude, longitude) = region_center(region);
    let Some(point) = project(latitude, longitude, options.yaw, options.pitch) else {
        return;
    };
    let x = (center_x + point.x * radius).round() as isize;
    let y = ((center_y - point.y * radius) / 2.0).round() as isize;
    if y < 0 || y as usize >= cells.len() || x < 0 || x as usize >= cells[0].len() {
        return;
    }

    let symbol = if options.ascii {
        ascii_symbol
    } else {
        unicode_symbol
    };
    let foreground = if options.no_color {
        Color::Reset
    } else {
        color
    };
    let style = overlay_style(&cells[y as usize][x as usize], foreground, true);
    cells[y as usize][x as usize] = Cell {
        symbol: Cow::Borrowed(symbol),
        style,
    };

    if show_label && point.depth > 0.62 {
        let label = format!(" {} ", region.designation);
        let label_width = label.chars().count() as isize;
        let start = if x + label_width + 1 < cells[0].len() as isize {
            x + 1
        } else {
            x - label_width
        };
        for (offset, character) in label.chars().enumerate() {
            let label_x = start + offset as isize;
            if label_x >= 0 && label_x < cells[0].len() as isize {
                let style = overlay_style(&cells[y as usize][label_x as usize], foreground, true);
                cells[y as usize][label_x as usize] = Cell {
                    symbol: Cow::Owned(character.to_string()),
                    style,
                };
            }
        }
    }
}

fn draw_mission_path(
    cells: &mut [Vec<Cell>],
    planet: &PlanetSnapshot,
    options: GlobeRenderOptions,
    center_x: f64,
    center_y: f64,
    radius: f64,
) {
    let Some(mission) = &planet.active_mission else {
        return;
    };
    let endpoints = match mission.phase {
        ScoutState::Outbound => (
            find_region(planet, &planet.scout.region_id),
            find_region(planet, &mission.target_region_id),
        ),
        ScoutState::Returning => (
            find_region(planet, &planet.scout.region_id),
            planet.regions.iter().find(|region| region.core),
        ),
        _ => return,
    };
    let (Some(start), Some(end)) = endpoints else {
        return;
    };
    draw_route_between(
        cells, start, end, options, center_x, center_y, radius, false,
    );
}

fn draw_extraction_path(
    cells: &mut [Vec<Cell>],
    planet: &PlanetSnapshot,
    options: GlobeRenderOptions,
    center_x: f64,
    center_y: f64,
    radius: f64,
) {
    let Some(mission) = &planet.active_extraction else {
        return;
    };
    if !matches!(
        mission.phase,
        mind_core::protocol::ExtractorState::Outbound
            | mind_core::protocol::ExtractorState::Returning
    ) {
        return;
    }
    let Some(core) = planet.regions.iter().find(|region| region.core) else {
        return;
    };
    let Some(target) = find_region(planet, &mission.target_region_id) else {
        return;
    };
    let (start, end) = if mission.phase == mind_core::protocol::ExtractorState::Returning {
        (target, core)
    } else {
        (core, target)
    };
    draw_route_between(cells, start, end, options, center_x, center_y, radius, true);
}

fn draw_construction_path(
    cells: &mut [Vec<Cell>],
    planet: &PlanetSnapshot,
    options: GlobeRenderOptions,
    center_x: f64,
    center_y: f64,
    radius: f64,
) {
    let Some(construction) = &planet.active_construction else {
        return;
    };
    let Some(core) = planet.regions.iter().find(|region| region.core) else {
        return;
    };
    let Some(target) = find_region(planet, &construction.region_id) else {
        return;
    };
    draw_route_between(
        cells, core, target, options, center_x, center_y, radius, false,
    );
}

fn draw_relay_pulse(
    cells: &mut [Vec<Cell>],
    planet: &PlanetSnapshot,
    options: GlobeRenderOptions,
    center_x: f64,
    center_y: f64,
    radius: f64,
) {
    let Some(relay) = planet
        .relay_region_id
        .as_ref()
        .and_then(|id| find_region(planet, id))
    else {
        return;
    };
    let (latitude, longitude) = region_center(relay);
    let Some(point) = project(latitude, longitude, options.yaw, options.pitch) else {
        return;
    };
    let origin_x = (center_x + point.x * radius).round() as isize;
    let origin_y = ((center_y - point.y * radius).round() as isize).div_euclid(2);
    let ring = 1 + (options.phase.fract() * 4.0).floor() as isize;
    let offsets = [
        (-ring, 0),
        (ring, 0),
        (0, -ring),
        (0, ring),
        (-ring, -ring / 2),
        (ring, ring / 2),
        (-ring, ring / 2),
        (ring, -ring / 2),
    ];
    for (offset_x, offset_y) in offsets {
        let x = origin_x + offset_x;
        let y = origin_y + offset_y;
        if y < 0 || y as usize >= cells.len() || x < 0 || x as usize >= cells[0].len() {
            continue;
        }
        let cell = &mut cells[y as usize][x as usize];
        *cell = Cell {
            symbol: Cow::Borrowed(if options.ascii { "." } else { "·" }),
            style: overlay_style(
                cell,
                if options.no_color {
                    Color::Reset
                } else {
                    options.theme.cyan
                },
                false,
            ),
        };
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_route_between(
    cells: &mut [Vec<Cell>],
    start: &RegionSnapshot,
    end: &RegionSnapshot,
    options: GlobeRenderOptions,
    center_x: f64,
    center_y: f64,
    radius: f64,
    extraction: bool,
) {
    let start_vector = sphere_vector(region_center(start));
    let end_vector = sphere_vector(region_center(end));
    let pulse = options.phase.fract();

    for step in 0..=72 {
        let amount = f64::from(step) / 72.0;
        let vector = slerp(start_vector, end_vector, amount);
        let latitude = vector.1.asin();
        let longitude = vector.0.atan2(vector.2);
        let Some(point) = project(latitude, longitude, options.yaw, options.pitch) else {
            continue;
        };
        let x = (center_x + point.x * radius).round() as isize;
        let subpixel_y = (center_y - point.y * radius).round() as isize;
        let y = subpixel_y.div_euclid(2);
        if y < 0 || y as usize >= cells.len() || x < 0 || x as usize >= cells[0].len() {
            continue;
        }
        let pulse_distance = (amount - pulse).abs().min(1.0 - (amount - pulse).abs());
        let is_pulse = pulse_distance < 0.025;
        if step % 3 == 0 || is_pulse {
            let foreground = if options.no_color {
                Color::Reset
            } else if is_pulse {
                options.theme.amber
            } else {
                if extraction {
                    options.theme.warning
                } else {
                    options.theme.cyan
                }
            };
            let cell = &mut cells[y as usize][x as usize];
            if options.ascii || options.no_color {
                let style = overlay_style(cell, foreground, is_pulse);
                *cell = Cell {
                    symbol: Cow::Borrowed(if is_pulse {
                        "*"
                    } else if extraction {
                        ":"
                    } else {
                        "~"
                    }),
                    style,
                };
            } else {
                paint_route_subpixel(cell, foreground, subpixel_y.rem_euclid(2) == 0, is_pulse);
            }
        }
    }
}

fn paint_route_subpixel(cell: &mut Cell, foreground: Color, upper: bool, bold: bool) {
    let preserved_half = if upper { cell.style.bg } else { cell.style.fg };
    let mut style = Style::default().fg(foreground);
    if let Some(background) = preserved_half {
        style = style.bg(background);
    }
    if bold {
        style = style.add_modifier(Modifier::BOLD);
    }
    cell.symbol = Cow::Borrowed(if upper { "▀" } else { "▄" });
    cell.style = style;
}

fn overlay_style(cell: &Cell, foreground: Color, bold: bool) -> Style {
    let mut style = Style::default().fg(foreground);
    if let Some(background) = cell.style.bg {
        style = style.bg(background);
    }
    if bold {
        style = style.add_modifier(Modifier::BOLD);
    }
    style
}

fn project(latitude: f64, longitude: f64, yaw: f64, pitch: f64) -> Option<ProjectedPoint> {
    let delta = normalize_signed_angle(longitude - yaw);
    let sin_latitude = latitude.sin();
    let cos_latitude = latitude.cos();
    let sin_pitch = pitch.sin();
    let cos_pitch = pitch.cos();
    let cos_delta = delta.cos();
    let depth = sin_latitude.mul_add(sin_pitch, cos_latitude * cos_pitch * cos_delta);
    (depth >= 0.0).then_some(ProjectedPoint {
        x: cos_latitude * delta.sin(),
        y: sin_latitude.mul_add(cos_pitch, -cos_latitude * sin_pitch * cos_delta),
        depth,
    })
}

fn region_at(planet: &PlanetSnapshot, latitude: f64, longitude: f64) -> Option<&RegionSnapshot> {
    let latitude_band = (((FRAC_PI_2 - latitude) / FRAC_PI_4).floor() as i16).clamp(0, 3) as u8;
    let longitude_sector =
        ((normalize_angle(longitude) + LONGITUDE_STEP / 2.0) / LONGITUDE_STEP).floor() as u8 % 6;
    planet.regions.iter().find(|region| {
        region.position.latitude_band == latitude_band
            && region.position.longitude_sector == longitude_sector
    })
}

fn near_region_edge(latitude: f64, longitude: f64, radius: f64) -> bool {
    let latitude_edge = [-FRAC_PI_4, 0.0, FRAC_PI_4]
        .into_iter()
        .map(|edge| (latitude - edge).abs())
        .fold(f64::INFINITY, f64::min);
    let sector_center = (normalize_angle(longitude) / LONGITUDE_STEP).round() * LONGITUDE_STEP;
    let longitude_edge =
        (normalize_signed_angle(longitude - sector_center).abs() - LONGITUDE_STEP / 2.0).abs()
            * latitude.cos().abs().max(0.18);
    let threshold = REGION_EDGE_WIDTH.max(0.48 / radius);
    latitude_edge < threshold || longitude_edge < threshold
}

fn region_center(region: &RegionSnapshot) -> (f64, f64) {
    let latitude = FRAC_PI_2 - (f64::from(region.position.latitude_band) + 0.5) * FRAC_PI_4;
    let longitude = f64::from(region.position.longitude_sector) * LONGITUDE_STEP;
    (latitude, longitude)
}

fn find_region<'a>(planet: &'a PlanetSnapshot, id: &str) -> Option<&'a RegionSnapshot> {
    planet.regions.iter().find(|region| region.id == id)
}

fn knowledge_color(knowledge: RegionKnowledge) -> Rgb {
    match knowledge {
        RegionKnowledge::Unknown => Rgb(18, 55, 64),
        RegionKnowledge::Detected => Rgb(22, 75, 85),
        RegionKnowledge::Surveyed => Rgb(28, 102, 112),
        RegionKnowledge::Observed => Rgb(42, 145, 145),
    }
}

impl From<(u8, u8, u8)> for Rgb {
    fn from((red, green, blue): (u8, u8, u8)) -> Self {
        Self(red, green, blue)
    }
}

fn terrain_tint(terrain: TerrainClass) -> Rgb {
    match terrain {
        TerrainClass::Basin => Rgb(22, 88, 110),
        TerrainClass::Plain => Rgb(48, 142, 137),
        TerrainClass::Highland => Rgb(95, 151, 141),
        TerrainClass::Ridge => Rgb(132, 154, 142),
        TerrainClass::Fractured => Rgb(100, 126, 135),
        TerrainClass::Polar => Rgb(150, 205, 210),
    }
}

fn terrain_texture(region: &RegionSnapshot, latitude: f64, longitude: f64) -> f64 {
    let surface = ((longitude * 11.0 + (latitude * 7.0).sin()).sin()
        * (latitude * 13.0 - longitude * 3.0).cos())
        * 0.022;
    let Some(terrain) = region.terrain else {
        return surface;
    };
    surface
        + match terrain {
            TerrainClass::Basin => -0.040 + (longitude * 5.0).sin() * 0.025,
            TerrainClass::Plain => (longitude * 3.0 + latitude * 4.0).sin() * 0.025,
            TerrainClass::Highland => 0.040 + (longitude * 7.0).cos() * 0.040,
            TerrainClass::Ridge => (longitude * 10.0 + latitude * 7.0).sin().abs() * 0.085,
            TerrainClass::Fractured => {
                let fracture = (longitude * 19.0 + (latitude * 8.0).sin() * 2.0)
                    .sin()
                    .abs()
                    .powf(8.0);
                fracture * 0.15 - 0.035
            }
            TerrainClass::Polar => 0.075 + (longitude * 4.0).cos() * 0.025,
        }
}

fn sphere_vector((latitude, longitude): (f64, f64)) -> (f64, f64, f64) {
    let cos_latitude = latitude.cos();
    (
        cos_latitude * longitude.sin(),
        latitude.sin(),
        cos_latitude * longitude.cos(),
    )
}

fn slerp(start: (f64, f64, f64), end: (f64, f64, f64), amount: f64) -> (f64, f64, f64) {
    let dot = (start.0 * end.0 + start.1 * end.1 + start.2 * end.2).clamp(-1.0, 1.0);
    let angle = dot.acos();
    if angle.abs() < f64::EPSILON {
        return start;
    }
    let sin_angle = angle.sin();
    let start_weight = ((1.0 - amount) * angle).sin() / sin_angle;
    let end_weight = (amount * angle).sin() / sin_angle;
    normalized3((
        start.0 * start_weight + end.0 * end_weight,
        start.1 * start_weight + end.1 * end_weight,
        start.2 * start_weight + end.2 * end_weight,
    ))
}

fn normalized3(vector: (f64, f64, f64)) -> (f64, f64, f64) {
    let length = (vector.0 * vector.0 + vector.1 * vector.1 + vector.2 * vector.2).sqrt();
    (vector.0 / length, vector.1 / length, vector.2 / length)
}

fn normalize_angle(angle: f64) -> f64 {
    angle.rem_euclid(TAU)
}

fn normalize_signed_angle(angle: f64) -> f64 {
    (angle + PI).rem_euclid(TAU) - PI
}

#[cfg(test)]
mod tests {
    use super::*;
    use mind_core::engine::MindEngine;

    fn planet() -> PlanetSnapshot {
        let engine = MindEngine::new(20);
        engine.complete_onboarding([2, 2, 2]);
        engine.planet().unwrap()
    }

    fn plain_text(text: &Text<'_>) -> String {
        text.lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn orthographic_projection_occludes_the_far_hemisphere() {
        assert!(project(0.0, 0.0, 0.0, 0.0).is_some());
        assert!(project(0.0, PI, 0.0, 0.0).is_none());
        assert!(project(FRAC_PI_2, 0.0, 0.0, 0.0).is_some());
    }

    #[test]
    fn unicode_render_is_round_and_labels_the_selected_region() {
        let planet = planet();
        let selected = planet.regions.iter().find(|region| region.core).unwrap();
        let output = render(
            &planet,
            Some(&selected.id),
            GlobeRenderOptions {
                width: 58,
                height: 24,
                yaw: region_center(selected).1,
                pitch: 0.0,
                phase: 0.0,
                ascii: false,
                no_color: false,
                theme: Theme::new(false),
            },
        );
        let plain = plain_text(&output);
        assert!(plain.contains(&selected.designation));
        assert!(plain.contains('▀'));
        assert_eq!(plain.lines().count(), 24);
        assert!(plain.lines().all(|line| line.chars().count() == 58));
    }

    #[test]
    fn ascii_monochrome_render_preserves_shape_and_selection() {
        let planet = planet();
        let selected = planet.regions.iter().find(|region| region.core).unwrap();
        let output = render(
            &planet,
            Some(&selected.id),
            GlobeRenderOptions {
                width: 42,
                height: 16,
                yaw: region_center(selected).1,
                pitch: 0.0,
                phase: 0.0,
                ascii: true,
                no_color: true,
                theme: Theme::new(true),
            },
        );
        let plain = plain_text(&output);
        assert!(plain.contains(&selected.designation));
        assert!(plain.contains('@'));
        assert!(plain.contains('#') || plain.contains('%'));
        assert!(plain
            .chars()
            .all(|character| character.is_ascii() || character == '\n'));
    }

    #[test]
    fn selected_region_remains_correct_across_rotation() {
        let planet = planet();
        let selected = &planet.regions[13];
        let (latitude, longitude) = region_center(selected);
        let point = project(latitude, longitude, longitude, latitude * 0.35).unwrap();
        assert!(point.depth > 0.8);
    }

    #[test]
    fn outbound_mission_draws_projected_route_and_endpoint_markers() {
        let engine = MindEngine::new(20);
        engine.complete_onboarding([2, 2, 2]);
        let planet = engine.planet().unwrap();
        let target = planet
            .regions
            .iter()
            .find(|region| region.hop_distance_from_core == 1)
            .unwrap();
        assert!(engine.deploy_scout(&target.id).ok);
        let planet = engine.planet().unwrap();
        let output = render(
            &planet,
            None,
            GlobeRenderOptions {
                width: 58,
                height: 24,
                yaw: region_center(target).1,
                pitch: 0.0,
                phase: 0.5,
                ascii: true,
                no_color: true,
                theme: Theme::new(true),
            },
        );
        let plain = plain_text(&output);
        assert!(plain.contains('~'));
        assert!(plain.contains('S'));
        assert!(plain.contains('T'));

        let rich_output = render(
            &planet,
            None,
            GlobeRenderOptions {
                width: 58,
                height: 24,
                yaw: region_center(target).1,
                pitch: 0.0,
                phase: 0.5,
                ascii: false,
                no_color: false,
                theme: Theme::new(false),
            },
        );
        assert!(rich_output
            .lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .any(|span| span.style.fg == Some(Theme::new(false).cyan)));
        assert!(!plain_text(&rich_output).contains('•'));
    }
}
