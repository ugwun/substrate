use crate::app::{App, ConnectionState, OnboardingView};
use mind_core::protocol::EngineEventKind;
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
        render_dashboard(frame, area, app, options, palette, now);
    }
    if app.show_help {
        render_help(frame, area, options, palette);
    }
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
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("SUBSTRATE", Style::default().fg(palette.primary).bold()),
            Span::raw(" // ORIENTATION COMPLETE"),
        ]))
        .block(panel(" STATUS ", options, palette)),
        header_area,
    );

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
                EngineEventKind::LegacySaveReset => "RESET",
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
        "{}{} // d diagnostic  r reconnect  ↑/↓ events  f follow  ? help  q detach",
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
            "GLOBAL\n  q / Ctrl-C  Detach without stopping daemon\n  r           Reconnect immediately\n  ? / Esc     Close help\n\nONBOARDING\n  ↑/↓         Select band\n  ←/→         Adjust selected band\n  Enter       Observe / awaken",
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
            protocol_version: 3,
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

        app.apply_snapshot(status(true), Instant::now());
        let dashboard = screen(
            &app,
            80,
            24,
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        assert!(dashboard.contains("ORIENTATION COMPLETE"));
        assert!(dashboard.contains("ENGINE EVENTS"));

        let narrow = screen(
            &app,
            50,
            28,
            RenderOptions {
                ascii: true,
                no_color: true,
            },
        );
        assert!(narrow.contains("DAEMON"));
        assert!(narrow.contains("DIAGNOSTIC"));
        assert!(narrow.contains("ENGINE EVENTS"));
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
    fn disconnected_dashboard_retains_snapshot_as_stale() {
        let mut app = App::default();
        let received = Instant::now();
        app.apply_snapshot(status(true), received);
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
