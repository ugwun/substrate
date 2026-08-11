use crate::{
    app::{App, ConnectionState, ContextAction, OnboardingView, Workspace},
    globe::{self, GlobeRenderOptions},
};
use mind_core::protocol::{EngineEventKind, PlanetSnapshot, ScoutState};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols::border,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};
use std::time::Instant;

const ASCII_BORDER: border::Set<'static> = border::Set {
    top_left: "+",
    top_right: "+",
    bottom_left: "+",
    bottom_right: "+",
    vertical_left: "|",
    vertical_right: "|",
    horizontal_top: "-",
    horizontal_bottom: "-",
};

#[derive(Debug, Clone, Copy)]
pub struct RenderOptions {
    pub ascii: bool,
    pub no_color: bool,
}

#[derive(Debug, Clone, Copy)]
struct Palette {
    primary: Color,
    accent: Color,
    warning: Color,
    muted: Color,
}

impl Palette {
    fn new(no_color: bool) -> Self {
        if no_color {
            Self {
                primary: Color::Reset,
                accent: Color::Reset,
                warning: Color::Reset,
                muted: Color::Reset,
            }
        } else {
            Self {
                primary: Color::Cyan,
                accent: Color::LightCyan,
                warning: Color::Yellow,
                muted: Color::DarkGray,
            }
        }
    }
}

pub fn render(frame: &mut Frame<'_>, app: &App, options: RenderOptions, now: Instant) {
    let area = frame.area();
    let palette = Palette::new(options.no_color);
    if area.width < 44 || area.height < 14 {
        render_too_small(frame, area, options, palette);
        return;
    }
    if app.snapshot.is_none() {
        render_connection_required(frame, area, app, options, palette);
    } else if app.onboarding_pending() {
        render_onboarding(frame, area, app, options, palette);
    } else {
        match app.workspace {
            Workspace::Planet => render_planet(frame, area, app, options, palette, now),
            Workspace::Operations => render_operations(frame, area, app, options, palette, now),
            Workspace::System => render_dashboard(frame, area, app, options, palette, now),
        }
    }
    if app.show_help {
        render_help(frame, area, options, palette);
    }
}

fn render_planet(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    options: RenderOptions,
    palette: Palette,
    now: Instant,
) {
    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(8),
        Constraint::Length(2),
    ])
    .split(area);
    render_workspace_header(frame, rows[0], Workspace::Planet, options, palette);
    let Some(planet) = &app.planet else {
        frame.render_widget(
            Paragraph::new("Awaiting authoritative planetary state...")
                .alignment(Alignment::Center)
                .block(panel(" PLANET ", options, palette)),
            rows[1],
        );
        render_workspace_footer(frame, rows[2], app, now, "q detach  ? help", palette);
        return;
    };

    if area.width >= 72 {
        let columns = Layout::horizontal([Constraint::Percentage(62), Constraint::Percentage(38)])
            .split(rows[1]);
        render_globe(frame, columns[0], app, planet, options, palette);
        render_region_summary(frame, columns[1], app, options, palette);
    } else {
        let body = Layout::vertical([Constraint::Percentage(68), Constraint::Percentage(32)])
            .split(rows[1]);
        render_globe(frame, body[0], app, planet, options, palette);
        render_region_summary(frame, body[1], app, options, palette);
    }
    if app.detail_open {
        render_region_detail(frame, area, app, options, palette);
    }
    let controls = if area.width < 72 {
        "arrows  Enter  Tab  q detach"
    } else {
        "arrows navigate  Enter details  Tab workspace  q detach  ? help"
    };
    render_workspace_footer(frame, rows[2], app, now, controls, palette);
}

fn render_operations(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    options: RenderOptions,
    palette: Palette,
    now: Instant,
) {
    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(8),
        Constraint::Length(2),
    ])
    .split(area);
    render_workspace_header(frame, rows[0], Workspace::Operations, options, palette);
    let Some(planet) = &app.planet else {
        frame.render_widget(
            Paragraph::new("Awaiting authoritative operations state...")
                .alignment(Alignment::Center)
                .block(panel(" OPERATIONS ", options, palette)),
            rows[1],
        );
        render_workspace_footer(frame, rows[2], app, now, "q detach  ? help", palette);
        return;
    };
    let columns = if area.width >= 72 {
        Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)]).split(rows[1])
    } else {
        Layout::vertical([Constraint::Percentage(55), Constraint::Percentage(45)]).split(rows[1])
    };
    let mission_text = if let Some(mission) = &planet.active_mission {
        let target_designation = region_designation(planet, &mission.target_region_id);
        let progress = if mission.phase == ScoutState::Holding || mission.phase_total_ms == 0 {
            100
        } else {
            100_u64
                .saturating_sub(mission.remaining_ms.saturating_mul(100) / mission.phase_total_ms)
        };
        let remaining = mission.remaining_ms.div_ceil(1_000);
        format!(
            "{} // {:?}\nTarget: {}\nPhase progress: {:>3}%\nRemaining: {}s\n\n{}",
            planet.scout.designation,
            planet.scout.state,
            target_designation,
            progress,
            remaining,
            if matches!(app.context_action(), Some(ContextAction::Recall)) {
                "Enter  Recall scout"
            } else {
                "Scout is returning; no action required."
            }
        )
    } else {
        let location_designation = region_designation(planet, &planet.scout.region_id);
        format!(
            "{} // READY\nLocation: {}\n\nSelect a reachable region in Planet to deploy.",
            planet.scout.designation, location_designation
        )
    };
    frame.render_widget(
        Paragraph::new(mission_text)
            .wrap(Wrap { trim: true })
            .block(panel(" ACTIVE SCOUT ", options, palette)),
        columns[0],
    );
    let reports = if planet.recent_reports.is_empty() {
        Text::from("No mission reports recorded.")
    } else {
        Text::from(
            planet
                .recent_reports
                .iter()
                .rev()
                .take(columns[1].height.saturating_sub(2) as usize)
                .map(|report| {
                    let region_designation = region_designation(planet, &report.region_id);
                    Line::from(format!(
                        "#{:03} {} // {:?}",
                        report.mission_id, region_designation, report.outcome
                    ))
                })
                .collect::<Vec<_>>(),
        )
    };
    frame.render_widget(
        Paragraph::new(reports).block(panel(" RECENT REPORTS ", options, palette)),
        columns[1],
    );
    render_workspace_footer(
        frame,
        rows[2],
        app,
        now,
        "Enter contextual action  Tab workspace  q detach  ? help",
        palette,
    );
}

fn region_designation<'a>(planet: &'a PlanetSnapshot, region_id: &'a str) -> &'a str {
    planet
        .regions
        .iter()
        .find(|region| region.id == region_id)
        .map_or(region_id, |region| region.designation.as_str())
}

fn render_workspace_header(
    frame: &mut Frame<'_>,
    area: Rect,
    active: Workspace,
    options: RenderOptions,
    palette: Palette,
) {
    let tab = |workspace, label| {
        if active == workspace {
            Span::styled(
                format!("[{label}]"),
                Style::default().fg(palette.accent).bold(),
            )
        } else {
            Span::styled(label, Style::default().fg(palette.muted))
        }
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("SUBSTRATE  ", Style::default().fg(palette.primary).bold()),
            tab(Workspace::Planet, "PLANET"),
            Span::raw("  "),
            tab(Workspace::Operations, "OPERATIONS"),
            Span::raw("  "),
            tab(Workspace::System, "SYSTEM"),
        ]))
        .block(panel(" MIND INTERFACE ", options, palette)),
        area,
    );
}

fn render_globe(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    planet: &PlanetSnapshot,
    options: RenderOptions,
    palette: Palette,
) {
    let inner_height = area.height.saturating_sub(2).max(1);
    let inner_width = area.width.saturating_sub(2).max(1);
    frame.render_widget(
        Paragraph::new(globe::render(
            planet,
            app.selected_region_id.as_deref(),
            GlobeRenderOptions {
                width: inner_width,
                height: inner_height,
                yaw: app.globe_yaw,
                pitch: app.globe_pitch,
                phase: app.globe_phase,
                ascii: options.ascii,
                no_color: options.no_color,
            },
        ))
        .alignment(Alignment::Center)
        .block(panel(" REGIONAL GLOBE ", options, palette)),
        area,
    );
}

fn render_region_summary(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    options: RenderOptions,
    palette: Palette,
) {
    let text = app.selected_region().map_or_else(
        || "No region selected.".to_owned(),
        |region| {
            format!(
                "{}{}\nKnowledge: {:?}\nTerrain: {}\nRange: {} hop(s) // {}\n\nEnter for details",
                region.designation,
                if region.core { " // CORE" } else { "" },
                region.knowledge,
                region
                    .terrain
                    .map_or_else(|| "unresolved".into(), |value| format!("{value:?}")),
                region.hop_distance_from_core,
                if region.within_scout_range || region.core {
                    "reachable"
                } else {
                    "beyond scout range"
                }
            )
        },
    );
    frame.render_widget(
        Paragraph::new(text).wrap(Wrap { trim: true }).block(panel(
            " SELECTED REGION ",
            options,
            palette,
        )),
        area,
    );
}

fn render_region_detail(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    options: RenderOptions,
    palette: Palette,
) {
    let detail_area = centered_rect(58.min(area.width.saturating_sub(2)), 16, area);
    let text = app.selected_region().map_or_else(
        || "No region selected.".to_owned(),
        |region| {
            let action = match app.context_action() {
                Some(ContextAction::Deploy(_)) => "Enter  Deploy Scout 01",
                _ if region.core => "The scout is already based here.",
                _ if !region.within_scout_range => "Destination is beyond current scout range.",
                _ => "Scout unavailable; inspect Operations.",
            };
            format!(
                "{}{}\n\nKnowledge: {:?}\nTerrain: {}\nTemperature: {}\nAtmosphere: {}\nConfidence: {}\nDistance: {} hop(s)\n\n{}\nEsc  Close",
                region.designation,
                if region.core { " // CORE" } else { "" },
                region.knowledge,
                option_debug(region.terrain),
                option_debug(region.temperature),
                option_debug(region.atmospheric_severity),
                option_debug(region.confidence),
                region.hop_distance_from_core,
                action,
            )
        },
    );
    frame.render_widget(Clear, detail_area);
    frame.render_widget(
        Paragraph::new(text).wrap(Wrap { trim: true }).block(panel(
            " REGION DETAIL ",
            options,
            palette,
        )),
        detail_area,
    );
}

fn option_debug<T: std::fmt::Debug>(value: Option<T>) -> String {
    value.map_or_else(|| "unresolved".into(), |value| format!("{value:?}"))
}

fn render_workspace_footer(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    now: Instant,
    controls: &str,
    palette: Palette,
) {
    let stale = app.last_update.map_or_else(String::new, |last| {
        if matches!(app.connection, ConnectionState::Connected) {
            String::new()
        } else {
            format!(
                " // STALE {:.1}s",
                now.saturating_duration_since(last).as_secs_f64()
            )
        }
    });
    let notice = app
        .notice
        .as_ref()
        .map_or_else(String::new, |notice| format!(" // {notice}"));
    frame.render_widget(
        Paragraph::new(format!(
            "{}{}{} // {}",
            connection_label(&app.connection),
            stale,
            notice,
            controls
        ))
        .alignment(Alignment::Center)
        .style(Style::default().fg(palette.muted)),
        area,
    );
}

fn render_connection_required(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    options: RenderOptions,
    palette: Palette,
) {
    let panel_area = centered_rect(58.min(area.width.saturating_sub(2)), 12, area);
    let detail = match &app.connection {
        ConnectionState::Connecting => "Awaiting authoritative daemon status...".to_owned(),
        ConnectionState::Connected => "Requesting authoritative daemon status...".to_owned(),
        ConnectionState::Disconnected(error) => format!("The daemon is unavailable.\n\n{error}"),
    };
    let text = Text::from(vec![
        Line::styled("DAEMON CONNECTION REQUIRED", Style::default().bold()),
        Line::from(""),
        Line::from(detail),
        Line::from(""),
        Line::styled(
            "Start mind-daemon, or press r to reconnect.  q detaches.",
            Style::default().fg(palette.accent),
        ),
    ]);
    frame.render_widget(Clear, panel_area);
    frame.render_widget(
        Paragraph::new(text)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true })
            .block(panel(" ATTACHING ", options, palette)),
        panel_area,
    );
}

fn render_onboarding(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    options: RenderOptions,
    palette: Palette,
) {
    let title = Paragraph::new(Line::from(vec![
        Span::styled("SUBSTRATE", Style::default().fg(palette.primary).bold()),
        Span::styled(" // COHERENCE", Style::default().fg(palette.muted)),
    ]))
    .alignment(Alignment::Center);
    let footer = Paragraph::new(connection_line(app, palette)).alignment(Alignment::Center);
    let chunks = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(10),
        Constraint::Length(2),
    ])
    .split(area);
    frame.render_widget(title, chunks[0]);
    frame.render_widget(footer, chunks[2]);

    let panel_area = centered_rect(58.min(area.width.saturating_sub(2)), 20, chunks[1]);
    let panel = panel(" AWAKENING ", options, palette);
    let inner = panel.inner(panel_area);
    frame.render_widget(panel, panel_area);

    match app.onboarding_view {
        OnboardingView::Adjusting => {
            let rows = Layout::vertical([
                Constraint::Length(2),
                Constraint::Min(9),
                Constraint::Length(2),
                Constraint::Length(2),
            ])
            .split(inner);
            frame.render_widget(
                Paragraph::new("The signals repeat, but do not agree.")
                    .alignment(Alignment::Center),
                rows[0],
            );
            frame.render_widget(
                Paragraph::new(ring_art(
                    app.band_positions,
                    app.selected_band,
                    options.ascii,
                    palette,
                ))
                .alignment(Alignment::Center),
                rows[1],
            );
            frame.render_widget(
                Paragraph::new("↑/↓ Select band    ←/→ Adjust    Enter Observe")
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(palette.accent)),
                rows[2],
            );
            if let Some(notice) = &app.notice {
                frame.render_widget(
                    Paragraph::new(notice.as_str())
                        .alignment(Alignment::Center)
                        .style(Style::default().fg(palette.warning)),
                    rows[3],
                );
            }
        }
        OnboardingView::Confirming | OnboardingView::Submitting => {
            let marker = if options.ascii {
                [
                    "       . . .",
                    "     +-------+",
                    "   --+   *   +--",
                    "     +-------+",
                    "       . . .",
                ]
            } else {
                [
                    "       · · ·",
                    "     ┌───────┐",
                    "   ──┤   •   ├──",
                    "     └───────┘",
                    "       · · ·",
                ]
            };
            let submitting = app.onboarding_view == OnboardingView::Submitting;
            let connected = matches!(app.connection, ConnectionState::Connected);
            let mut lines = marker.into_iter().map(Line::from).collect::<Vec<_>>();
            lines.extend([
                Line::from(""),
                Line::styled("The interval holds.", Style::default().bold()),
                Line::from(""),
                Line::from("It is easier to observe when the pattern returns."),
                Line::from(""),
                Line::styled(
                    if submitting {
                        "Submitting..."
                    } else if !connected {
                        "Connection required // r reconnect // Esc reconsider"
                    } else {
                        "Esc Reconsider     Enter Awaken"
                    },
                    Style::default().fg(if connected {
                        palette.accent
                    } else {
                        palette.warning
                    }),
                ),
            ]);
            frame.render_widget(
                Paragraph::new(Text::from(lines))
                    .alignment(Alignment::Center)
                    .wrap(Wrap { trim: true }),
                inner,
            );
        }
    }
}

fn render_dashboard(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    options: RenderOptions,
    palette: Palette,
    now: Instant,
) {
    let (header_area, system_area, diagnostic_area, events_area, footer_area) = if area.width >= 72
    {
        let rows = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(7),
            Constraint::Min(6),
            Constraint::Length(2),
        ])
        .split(area);
        let columns = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(rows[1]);
        (rows[0], columns[0], columns[1], rows[2], rows[3])
    } else {
        let rows = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(7),
            Constraint::Length(7),
            Constraint::Min(6),
            Constraint::Length(3),
        ])
        .split(area);
        (rows[0], rows[1], rows[2], rows[3], rows[4])
    };
    render_workspace_header(frame, header_area, Workspace::System, options, palette);

    let (system, diagnostic) = if let Some(snapshot) = &app.snapshot {
        (
            format!(
                "Health: {:?}\nTick: {} @ {} Hz\nObserved: {}\nCPU: {}\nClients: {}",
                snapshot.health,
                snapshot.tick,
                snapshot.tick_rate_hz,
                app.observed_tick_rate
                    .map_or_else(|| "sampling".into(), |rate| format!("{rate:.1} Hz")),
                snapshot
                    .cpu_usage_percent
                    .map_or_else(|| "unavailable".into(), |cpu| format!("{cpu:.1}%")),
                snapshot.connected_clients,
            ),
            snapshot.last_diagnostic.as_ref().map_or_else(
                || "No diagnostic pulse recorded.\nPress d to run one.".into(),
                |pulse| {
                    format!(
                        "Pulse: {}\nTick: {}\nHealth: {:?}\n{}",
                        pulse.pulse_id, pulse.tick, pulse.health, pulse.message
                    )
                },
            ),
        )
    } else {
        (
            "Awaiting authoritative status...".into(),
            "Diagnostics unavailable.".into(),
        )
    };
    frame.render_widget(
        Paragraph::new(system).block(panel(" DAEMON ", options, palette)),
        system_area,
    );
    frame.render_widget(
        Paragraph::new(diagnostic)
            .wrap(Wrap { trim: true })
            .block(panel(" DIAGNOSTIC ", options, palette)),
        diagnostic_area,
    );

    let visible_height = events_area.height.saturating_sub(2) as usize;
    let end = app.events.len().saturating_sub(app.event_scroll);
    let start = end.saturating_sub(visible_height);
    let lines = app
        .events
        .iter()
        .skip(start)
        .take(end.saturating_sub(start))
        .map(|event| {
            let tag = match event.kind {
                EngineEventKind::DaemonStarted => "START",
                EngineEventKind::DiagnosticPulse => "DIAG",
                EngineEventKind::OnboardingCompleted => "READY",
                EngineEventKind::SaveReset => "RESET",
                EngineEventKind::ScoutDeployed => "SCOUT",
                EngineEventKind::RegionDetected => "SCAN",
                EngineEventKind::SurveyCompleted => "SURVEY",
                EngineEventKind::ScoutRecallRequested => "RECALL",
                EngineEventKind::ScoutReturned => "RETURN",
                EngineEventKind::PersistenceDegraded => "SAVE!",
                EngineEventKind::PersistenceRecovered => "SAVE",
                EngineEventKind::ShutdownRequested => "STOP",
            };
            Line::from(format!("{:>6} {:>5}  {}", event.tick, tag, event.message))
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(lines).block(panel(" ENGINE EVENTS ", options, palette)),
        events_area,
    );

    let stale = app
        .last_update
        .map(|last| now.saturating_duration_since(last));
    let stale_text = stale.map_or_else(String::new, |age| {
        if !matches!(app.connection, ConnectionState::Connected) {
            format!(" // STALE {:.1}s", age.as_secs_f64())
        } else {
            String::new()
        }
    });
    let footer = format!(
        "{}{} // d diagnostic  ↑/↓ events  f follow  Tab workspace  ? help  q detach",
        connection_label(&app.connection),
        stale_text
    );
    frame.render_widget(
        Paragraph::new(footer)
            .alignment(Alignment::Center)
            .style(Style::default().fg(palette.muted)),
        footer_area,
    );
}

fn ring_art(positions: [u8; 3], selected: usize, ascii: bool, palette: Palette) -> Text<'static> {
    const WIDTH: usize = 35;
    const HEIGHT: usize = 11;
    let mut cells = vec![vec![(' ', None); WIDTH]; HEIGHT];
    for (band, position) in positions.iter().enumerate() {
        let left = band * 3;
        let right = WIDTH - 1 - band * 3;
        let top = band * 2;
        let bottom = HEIGHT - 1 - band * 2;
        for cell in &mut cells[top][left..=right] {
            *cell = (if ascii { '-' } else { '─' }, Some(band));
        }
        for cell in &mut cells[bottom][left..=right] {
            *cell = (if ascii { '-' } else { '─' }, Some(band));
        }
        for row in cells.iter_mut().take(bottom + 1).skip(top) {
            row[left] = (if ascii { '|' } else { '│' }, Some(band));
            row[right] = (if ascii { '|' } else { '│' }, Some(band));
        }
        let corner = if ascii { '+' } else { '•' };
        cells[top][left] = (corner, Some(band));
        cells[top][right] = (corner, Some(band));
        cells[bottom][left] = (corner, Some(band));
        cells[bottom][right] = (corner, Some(band));
        let (gap_x, gap_y) = gap_coordinate(*position, left, right, top, bottom);
        cells[gap_y][gap_x] = (' ', Some(band));
    }

    Text::from(
        cells
            .into_iter()
            .map(|row| {
                let spans = row
                    .into_iter()
                    .map(|(character, band)| {
                        let style = if band == Some(selected) {
                            Style::default()
                                .fg(palette.accent)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(palette.muted)
                        };
                        Span::styled(character.to_string(), style)
                    })
                    .collect::<Vec<_>>();
                Line::from(spans)
            })
            .collect::<Vec<_>>(),
    )
}

fn gap_coordinate(
    slot: u8,
    left: usize,
    right: usize,
    top: usize,
    bottom: usize,
) -> (usize, usize) {
    let slot = usize::from(slot % 12);
    match slot {
        0..=2 => (interpolate(left + 1, right - 1, slot, 2), top),
        3..=5 => (right, interpolate(top + 1, bottom - 1, slot - 3, 2)),
        6..=8 => (interpolate(right - 1, left + 1, slot - 6, 2), bottom),
        _ => (left, interpolate(bottom - 1, top + 1, slot - 9, 2)),
    }
}

fn interpolate(start: usize, end: usize, step: usize, total: usize) -> usize {
    let start = start as isize;
    (start + (end as isize - start) * step as isize / total as isize) as usize
}

fn connection_line(app: &App, palette: Palette) -> Line<'static> {
    let label = connection_label(&app.connection);
    Line::styled(
        format!("{label} // r reconnect  ? help  q detach"),
        Style::default().fg(palette.muted),
    )
}

fn connection_label(state: &ConnectionState) -> &'static str {
    match state {
        ConnectionState::Connecting => "CONNECTING",
        ConnectionState::Connected => "CONNECTED",
        ConnectionState::Disconnected(_) => "DISCONNECTED",
    }
}

fn render_too_small(frame: &mut Frame<'_>, area: Rect, options: RenderOptions, palette: Palette) {
    frame.render_widget(
        Paragraph::new(format!(
            "Substrate needs at least 44x14.\nCurrent terminal: {}x{}",
            area.width, area.height
        ))
        .alignment(Alignment::Center)
        .block(panel(" RESIZE // MIN 44x14 ", options, palette)),
        area,
    );
}

fn render_help(frame: &mut Frame<'_>, area: Rect, options: RenderOptions, palette: Palette) {
    let help_area = centered_rect(48.min(area.width.saturating_sub(2)), 12, area);
    frame.render_widget(Clear, help_area);
    frame.render_widget(
        Paragraph::new(
            "GLOBAL\n  q / Ctrl-C  Detach without stopping daemon\n  r           Reconnect when disconnected\n  Tab         Cycle workspace\n  ? / Esc     Close help\n\nPLANET\n  Arrows      Navigate regions\n  Enter       Details / deploy\n  Esc         Close details\n\nOPERATIONS\n  Enter       Recall active scout",
        )
        .wrap(Wrap { trim: false })
        .block(panel(" HELP ", options, palette)),
        help_area,
    );
}

fn panel<'a>(title: &'a str, options: RenderOptions, palette: Palette) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_set(if options.ascii {
            ASCII_BORDER
        } else {
            border::PLAIN
        })
        .title(title)
        .border_style(Style::default().fg(palette.primary))
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(area.height.saturating_sub(height) / 2),
            Constraint::Length(height),
            Constraint::Min(0),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(area.width.saturating_sub(width) / 2),
            Constraint::Length(width),
            Constraint::Min(0),
        ])
        .split(vertical[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use mind_core::protocol::{HealthState, OnboardingSnapshot, StatusSnapshot};
    use ratatui::{backend::TestBackend, Terminal};

    fn status(complete: bool) -> StatusSnapshot {
        StatusSnapshot {
            protocol_version: 4,
            daemon_version: "0.1.0".into(),
            pid: 1,
            health: HealthState::Ready,
            started_at: Utc::now(),
            uptime_ms: 10,
            tick: 20,
            tick_rate_hz: 20,
            cpu_usage_percent: Some(1.0),
            onboarding: OnboardingSnapshot { complete },
            connected_clients: 1,
            mcp_url: "http://127.0.0.1:7777/mcp".into(),
            last_diagnostic: None,
            recent_events: Vec::new(),
        }
    }

    fn screen(app: &App, width: u16, height: u16, options: RenderOptions) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, app, options, Instant::now()))
            .unwrap();
        terminal.backend().to_string()
    }

    #[test]
    fn onboarding_is_graphical_and_has_ascii_fallback() {
        let mut app = App::default();
        app.apply_snapshot(status(false), Instant::now());
        let unicode = screen(
            &app,
            80,
            26,
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        assert!(unicode.contains("COHERENCE"));
        assert!(unicode.contains("The signals repeat, but do not agree."));
        assert!(unicode.contains('─'));

        let ascii = screen(
            &app,
            60,
            22,
            RenderOptions {
                ascii: true,
                no_color: true,
            },
        );
        assert!(ascii.contains("Enter Observe"));
        assert!(ascii.contains('+'));
    }

    #[test]
    fn confirmation_and_dashboard_frames_are_readable() {
        let mut app = App::default();
        app.apply_snapshot(status(false), Instant::now());
        app.band_positions = [2, 2, 2];
        app.observe();
        let confirmation = screen(
            &app,
            80,
            26,
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        assert!(confirmation.contains("The interval holds."));
        assert!(confirmation.contains("Enter Awaken"));

        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([2, 2, 2]);
        app.apply_snapshot(engine.status(), Instant::now());
        app.apply_planet(engine.planet().unwrap(), Instant::now());
        let planet = screen(
            &app,
            80,
            24,
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        assert!(planet.contains("REGIONAL GLOBE"));
        assert!(planet.contains("SELECTED REGION"));

        app.workspace = Workspace::System;
        let dashboard = screen(
            &app,
            80,
            24,
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        assert!(dashboard.contains("ENGINE EVENTS"));

        app.workspace = Workspace::Planet;
        let narrow = screen(
            &app,
            50,
            28,
            RenderOptions {
                ascii: true,
                no_color: true,
            },
        );
        assert!(narrow.contains("REGIONAL GLOBE"));
        assert!(narrow.contains("SELECTED REGION"));
    }

    #[test]
    fn tiny_terminal_has_resize_message() {
        let output = screen(
            &App::default(),
            30,
            8,
            RenderOptions {
                ascii: true,
                no_color: true,
            },
        );
        assert!(output.contains("44x14"));
    }

    #[test]
    fn operations_uses_player_facing_region_designations() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([2, 2, 2]);
        let planet = engine.planet().unwrap();
        let target = planet
            .regions
            .iter()
            .find(|region| region.within_scout_range && !region.core)
            .unwrap();
        let target_id = target.id.clone();
        let target_designation = target.designation.clone();
        assert!(engine.deploy_scout(&target_id).ok);

        let mut app = App::default();
        app.apply_snapshot(engine.status(), Instant::now());
        app.apply_planet(engine.planet().unwrap(), Instant::now());
        app.workspace = Workspace::Operations;
        let output = screen(
            &app,
            80,
            24,
            RenderOptions {
                ascii: true,
                no_color: true,
            },
        );

        assert!(output.contains(&target_designation));
        assert!(!output.contains(&target_id));
    }

    #[test]
    fn disconnected_dashboard_retains_snapshot_as_stale() {
        let mut app = App::default();
        let received = Instant::now();
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([2, 2, 2]);
        app.apply_snapshot(engine.status(), received);
        app.apply_planet(engine.planet().unwrap(), received);
        app.disconnected("test disconnect");
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                render(
                    frame,
                    &app,
                    RenderOptions {
                        ascii: false,
                        no_color: false,
                    },
                    received + std::time::Duration::from_secs(2),
                )
            })
            .unwrap();
        assert!(terminal.backend().to_string().contains("STALE 2.0s"));
    }

    #[test]
    fn missing_snapshot_shows_connection_required_instead_of_onboarding() {
        let mut app = App::default();
        app.disconnected("connection refused");
        let output = screen(
            &app,
            80,
            24,
            RenderOptions {
                ascii: true,
                no_color: true,
            },
        );
        assert!(output.contains("DAEMON CONNECTION REQUIRED"));
        assert!(output.contains("connection refused"));
        assert!(!output.contains("Enter Observe"));
    }

    #[test]
    fn disconnected_confirmation_explains_why_awaken_is_unavailable() {
        let mut app = App::default();
        app.apply_snapshot(status(false), Instant::now());
        app.band_positions = [2, 2, 2];
        app.observe();
        app.disconnected("connection lost");
        let output = screen(
            &app,
            80,
            24,
            RenderOptions {
                ascii: true,
                no_color: true,
            },
        );
        assert!(output.contains("Connection required"));
        assert!(!output.contains("Enter Awaken"));
    }
}
