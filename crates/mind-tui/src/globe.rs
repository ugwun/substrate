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
    f64::consts::{FRAC_PI_2, PI, TAU},
};

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
    knowledge: RegionKnowledge,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct GlobeCacheKey {
    width: u16,
    height: u16,
    yaw_q: i32,
    pitch_q: i32,
    revision: u64,
    selected_index: Option<usize>,
    ascii: bool,
    no_color: bool,
    ansi16: bool,
}

#[derive(Debug, Default)]
pub struct GlobeCache {
    key: Option<GlobeCacheKey>,
    base_cells: Vec<Vec<Cell>>,
    hits: u64,
    misses: u64,
}

impl GlobeCache {
    pub fn stats(&self) -> (u64, u64) {
        (self.hits, self.misses)
    }

    pub fn clear(&mut self) {
        self.key = None;
        self.base_cells.clear();
    }
}

struct RegionGrid {
    latitude_bands: usize,
    longitude_sectors: usize,
    indices: Vec<Option<usize>>,
}

impl RegionGrid {
    fn from_planet(planet: &PlanetSnapshot) -> Self {
        let latitude_bands = usize::from(planet.latitude_bands.max(1));
        let longitude_sectors = usize::from(planet.longitude_sectors.max(1));
        let mut indices = vec![None; latitude_bands * longitude_sectors];
        for (index, region) in planet.regions.iter().enumerate() {
            let band = usize::from(region.position.latitude_band);
            let sector = usize::from(region.position.longitude_sector);
            if band < latitude_bands && sector < longitude_sectors {
                indices[band * longitude_sectors + sector] = Some(index);
            }
        }
        Self {
            latitude_bands,
            longitude_sectors,
            indices,
        }
    }

    #[inline]
    fn get(&self, latitude_band: u8, longitude_sector: u8) -> Option<usize> {
        let band = usize::from(latitude_band);
        let sector = usize::from(longitude_sector);
        if band < self.latitude_bands && sector < self.longitude_sectors {
            self.indices[band * self.longitude_sectors + sector]
        } else {
            None
        }
    }
}

struct SurfaceContext<'a> {
    planet: &'a PlanetSnapshot,
    grid: &'a RegionGrid,
    selected_index: Option<usize>,
    center_x: f64,
    center_y: f64,
    radius: f64,
    yaw: f64,
    sin_pitch: f64,
    cos_pitch: f64,
    light: (f64, f64, f64),
    latitude_step: f64,
    longitude_step: f64,
    edge_threshold: f64,
}

pub fn render(
    planet: &PlanetSnapshot,
    selected_id: Option<&str>,
    options: GlobeRenderOptions,
) -> Text<'static> {
    let mut cache = GlobeCache::default();
    render_cached(planet, selected_id, options, &mut cache)
}

pub fn render_cached(
    planet: &PlanetSnapshot,
    selected_id: Option<&str>,
    options: GlobeRenderOptions,
    cache: &mut GlobeCache,
) -> Text<'static> {
    render_cached_with_flashes(planet, selected_id, options, cache, &[])
}

pub fn render_cached_with_flashes(
    planet: &PlanetSnapshot,
    selected_id: Option<&str>,
    options: GlobeRenderOptions,
    cache: &mut GlobeCache,
    flash_region_ids: &[&str],
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
    let selected_index =
        selected_id.and_then(|id| planet.regions.iter().position(|region| region.id == id));

    let key = GlobeCacheKey {
        width: options.width.max(1),
        height: options.height.max(1),
        yaw_q: (options.yaw * 1_000.0).round() as i32,
        pitch_q: (options.pitch * 1_000.0).round() as i32,
        revision: planet.revision,
        selected_index,
        ascii: options.ascii,
        no_color: options.no_color,
        ansi16: options.theme.ansi16(),
    };

    if cache.key.as_ref() == Some(&key) && cache.base_cells.len() == height {
        cache.hits = cache.hits.saturating_add(1);
    } else {
        cache.misses = cache.misses.saturating_add(1);
        let grid = RegionGrid::from_planet(planet);
        let ctx = SurfaceContext {
            planet,
            grid: &grid,
            selected_index,
            center_x,
            center_y,
            radius,
            yaw: options.yaw,
            sin_pitch: options.pitch.sin(),
            cos_pitch: options.pitch.cos(),
            light: normalized3((-0.38, 0.46, 0.80)),
            latitude_step: PI / f64::from(planet.latitude_bands.max(1)),
            longitude_step: TAU / f64::from(planet.longitude_sectors.max(1)),
            edge_threshold: REGION_EDGE_WIDTH.max(0.48 / radius),
        };
        cache.base_cells = (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| {
                        let top = sample_surface(&ctx, x, y * 2);
                        let bottom = sample_surface(&ctx, x, y * 2 + 1);
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
        cache.key = Some(key);
    }

    let mut cells = cache.base_cells.clone();

    draw_mission_path(&mut cells, planet, options, center_x, center_y, radius);
    draw_extraction_path(&mut cells, planet, options, center_x, center_y, radius);
    draw_construction_path(&mut cells, planet, options, center_x, center_y, radius);
    draw_relay_pulse(&mut cells, planet, options, center_x, center_y, radius);
    draw_region_markers(
        &mut cells,
        planet,
        selected_id,
        flash_region_ids,
        options,
        center_x,
        center_y,
        radius,
    );

    Text::from(cells.into_iter().map(merge_row_spans).collect::<Vec<_>>())
}

fn merge_row_spans(row: Vec<Cell>) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::with_capacity(row.len() / 2 + 1);
    let mut current_style: Option<Style> = None;
    let mut current_text = String::new();

    for cell in row {
        if current_style == Some(cell.style) {
            current_text.push_str(&cell.symbol);
        } else {
            if let Some(style) = current_style {
                spans.push(Span::styled(std::mem::take(&mut current_text), style));
            }
            current_style = Some(cell.style);
            current_text.push_str(&cell.symbol);
        }
    }
    if let Some(style) = current_style {
        spans.push(Span::styled(current_text, style));
    }
    Line::from(spans)
}

fn sample_surface(ctx: &SurfaceContext<'_>, x: usize, y: usize) -> Option<SurfaceSample> {
    let screen_x = (x as f64 - ctx.center_x) / ctx.radius;
    let screen_y = (ctx.center_y - y as f64) / ctx.radius;
    let radius_squared = screen_x.mul_add(screen_x, screen_y * screen_y);
    if radius_squared > 1.0 {
        return None;
    }

    let depth = (1.0 - radius_squared).sqrt();
    let latitude = (depth.mul_add(ctx.sin_pitch, screen_y * ctx.cos_pitch))
        .clamp(-1.0, 1.0)
        .asin();
    let longitude = normalize_angle(
        ctx.yaw + screen_x.atan2(depth.mul_add(ctx.cos_pitch, -screen_y * ctx.sin_pitch)),
    );
    let region_index = region_index_at(ctx, latitude, longitude)?;
    let region = &ctx.planet.regions[region_index];
    let selected = ctx.selected_index == Some(region_index);
    let region_edge = near_region_edge(ctx, latitude, longitude);
    let selected_edge = selected && region_edge;

    let diffuse = (screen_x * ctx.light.0 + screen_y * ctx.light.1 + depth * ctx.light.2).max(0.0);
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
        knowledge: region.knowledge,
        luminance: luminance.clamp(0.0, 1.0),
        selected,
        selected_edge,
        region_edge,
    })
}

fn sample_color(sample: &SurfaceSample, theme: Theme) -> Color {
    if !theme.ansi16() {
        return sample.color.color();
    }
    if sample.selected_edge {
        return theme.gold;
    }
    if sample.selected {
        return theme.amber;
    }
    match sample.knowledge {
        RegionKnowledge::Unknown => {
            if sample.luminance >= 0.45 {
                Color::DarkGray
            } else {
                Color::Black
            }
        }
        RegionKnowledge::Detected => {
            if sample.luminance >= 0.50 {
                Color::LightBlue
            } else {
                Color::Blue
            }
        }
        RegionKnowledge::Surveyed => {
            if sample.luminance >= 0.55 {
                Color::LightCyan
            } else {
                Color::Cyan
            }
        }
        RegionKnowledge::Observed => {
            if sample.luminance >= 0.65 {
                Color::White
            } else {
                Color::LightCyan
            }
        }
    }
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
                .fg(sample_color(&top, theme))
                .bg(sample_color(&bottom, theme)),
        },
        (Some(top), None) => Cell {
            symbol: Cow::Borrowed("▀"),
            style: Style::default().fg(sample_color(&top, theme)),
        },
        (None, Some(bottom)) => Cell {
            symbol: Cow::Borrowed("▄"),
            style: Style::default().fg(sample_color(&bottom, theme)),
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
        Style::default().fg(sample_color(&sample, theme))
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

#[allow(clippy::too_many_arguments)]
fn draw_region_markers(
    cells: &mut [Vec<Cell>],
    planet: &PlanetSnapshot,
    selected_id: Option<&str>,
    flash_region_ids: &[&str],
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
    let campaign_lead = planet
        .campaign
        .lead_region_id
        .as_ref()
        .filter(|lead_id| selected_id != Some(lead_id.as_str()))
        .and_then(|id| find_region(planet, id));

    if let Some(lead) = campaign_lead {
        place_marker(
            cells,
            planet,
            lead,
            if (options.phase * 2.5).floor() as i64 % 2 == 0 {
                "◎"
            } else {
                "◌"
            },
            "+",
            options.theme.cyan_bright,
            false,
            options,
            center_x,
            center_y,
            radius,
        );
    }

    for flash_id in flash_region_ids {
        if let Some(region) = find_region(planet, flash_id) {
            place_marker(
                cells,
                planet,
                region,
                if (options.phase * 8.0).floor() as i64 % 2 == 0 {
                    "◈"
                } else {
                    "◎"
                },
                "*",
                options.theme.gold,
                true,
                options,
                center_x,
                center_y,
                radius,
            );
        }
    }

    if let Some(core) = planet.regions.iter().find(|region| region.core) {
        place_marker(
            cells,
            planet,
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
            planet,
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
            planet,
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
            planet,
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
            planet,
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
            planet,
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
            planet,
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
    planet: &PlanetSnapshot,
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
    let (latitude, longitude) = region_center(planet, region);
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
        cells, planet, start, end, options, center_x, center_y, radius, false,
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
    draw_route_between(
        cells, planet, start, end, options, center_x, center_y, radius, true,
    );
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
        cells, planet, core, target, options, center_x, center_y, radius, false,
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
    let (latitude, longitude) = region_center(planet, relay);
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
    planet: &PlanetSnapshot,
    start: &RegionSnapshot,
    end: &RegionSnapshot,
    options: GlobeRenderOptions,
    center_x: f64,
    center_y: f64,
    radius: f64,
    extraction: bool,
) {
    let start_vector = sphere_vector(region_center(planet, start));
    let end_vector = sphere_vector(region_center(planet, end));
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

#[inline]
fn region_index_at(ctx: &SurfaceContext<'_>, latitude: f64, longitude: f64) -> Option<usize> {
    let latitude_band = (((FRAC_PI_2 - latitude) / ctx.latitude_step).floor() as i16)
        .clamp(0, i16::from(ctx.planet.latitude_bands.saturating_sub(1)))
        as u8;
    let longitude_sector = ((normalize_angle(longitude) + ctx.longitude_step / 2.0)
        / ctx.longitude_step)
        .floor() as u8
        % ctx.planet.longitude_sectors;
    ctx.grid.get(latitude_band, longitude_sector)
}

#[inline]
fn near_region_edge(ctx: &SurfaceContext<'_>, latitude: f64, longitude: f64) -> bool {
    let bands = ctx.planet.latitude_bands;
    let latitude_edge = if bands > 1 {
        let band_pos = (FRAC_PI_2 - latitude) / ctx.latitude_step;
        let nearest_edge = band_pos.round().clamp(1.0, f64::from(bands - 1));
        (band_pos - nearest_edge).abs() * ctx.latitude_step
    } else {
        f64::INFINITY
    };
    let sector_center =
        (normalize_angle(longitude) / ctx.longitude_step).round() * ctx.longitude_step;
    let longitude_edge =
        (normalize_signed_angle(longitude - sector_center).abs() - ctx.longitude_step / 2.0).abs()
            * latitude.cos().abs().max(0.18);
    latitude_edge < ctx.edge_threshold || longitude_edge < ctx.edge_threshold
}

fn region_center(planet: &PlanetSnapshot, region: &RegionSnapshot) -> (f64, f64) {
    let latitude_step = PI / f64::from(planet.latitude_bands);
    let longitude_step = TAU / f64::from(planet.longitude_sectors);
    let latitude = FRAC_PI_2 - (f64::from(region.position.latitude_band) + 0.5) * latitude_step;
    let longitude = f64::from(region.position.longitude_sector) * longitude_step;
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
                yaw: region_center(&planet, selected).1,
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
                yaw: region_center(&planet, selected).1,
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
        let (latitude, longitude) = region_center(&planet, selected);
        let point = project(latitude, longitude, longitude, latitude).unwrap();
        assert!(point.depth > 0.99);
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
                yaw: region_center(&planet, target).1,
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
                yaw: region_center(&planet, target).1,
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

    #[test]
    fn cached_globe_render_hits_cache_invalidates_on_selection_and_merges_spans() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([2, 2, 2]);
        let planet = engine.planet().unwrap();
        let options = GlobeRenderOptions {
            width: 58,
            height: 24,
            yaw: 0.4,
            pitch: 0.1,
            phase: 0.25,
            ascii: false,
            no_color: false,
            theme: Theme::new(false),
        };
        let mut cache = GlobeCache::default();
        let first = render_cached(&planet, Some(&planet.core_region_id), options, &mut cache);
        assert_eq!(cache.stats(), (0, 1));
        let second = render_cached(
            &planet,
            Some(&planet.core_region_id),
            GlobeRenderOptions {
                phase: 0.75,
                ..options
            },
            &mut cache,
        );
        assert_eq!(cache.stats(), (1, 1));
        assert!(plain_text(&first).contains('◎'));
        assert!(plain_text(&second).contains('◌'));
        assert_eq!(
            plain_text(&first),
            plain_text(&render(&planet, Some(&planet.core_region_id), options))
        );
        // Merged spans must be substantially fewer than total cells (58 * 24 = 1392).
        let span_count: usize = first.lines.iter().map(|line| line.spans.len()).sum();
        assert!(span_count < 900, "expected span merging, got {span_count}");

        let other_region = planet
            .regions
            .iter()
            .find(|region| region.id != planet.core_region_id)
            .unwrap();
        let _third = render_cached(&planet, Some(&other_region.id), options, &mut cache);
        assert_eq!(cache.stats(), (1, 2));
    }
}
