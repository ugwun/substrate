use crate::app::{App, ConnectionState};
use chrono::Local;
use mind_core::protocol::EngineEventKind;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Padding, Paragraph, Wrap},
    Frame,
};
use std::time::Instant;

#[derive(Debug, Clone, Copy)]
pub struct RenderOptions {
    pub ascii: bool,
    pub no_color: bool,
}

#[derive(Clone, Copy)]
struct Palette {
    primary: Color,
    healthy: Color,
    warning: Color,
    failure: Color,
    muted: Color,
}

impl Palette {
    fn new(no_color: bool) -> Self {
        if no_color {
            Self {
                primary: Color::Reset,
                healthy: Color::Reset,
                warning: Color::Reset,
                failure: Color::Reset,
                muted: Color::Reset,
            }
        } else {
            Self {
                primary: Color::Cyan,
                healthy: Color::Green,
                warning: Color::Yellow,
                failure: Color::Red,
                muted: Color::DarkGray,
            }
        }
    }
}

pub fn render(frame: &mut Frame<'_>, app: &App, options: RenderOptions, now: Instant) {
    let area = frame.area();
    let palette = Palette::new(options.no_color);
    if area.width < 60 || area.height < 18 {
        render_too_small(frame, area, app, options, palette);
        return;
    }

    let root = panel(" SUBSTRATE // AWAKENING MIND ", options)
        .border_style(Style::default().fg(palette.primary));
    let inner = root.inner(area);
    frame.render_widget(root, area);

    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(5),
        Constraint::Min(8),
        Constraint::Length(1),
    ])
    .split(inner);
    render_header(frame, rows[0], app, palette, now);
    render_metrics(frame, rows[1], app, options, palette);

    if area.width >= 100 && area.height >= 28 {
        let body = Layout::horizontal([Constraint::Percentage(65), Constraint::Percentage(35)])
            .split(rows[2]);
        render_events(frame, body[0], app, options, palette);
        render_diagnostics(frame, body[1], app, options, palette);
    } else {
        let body = Layout::vertical([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(rows[2]);
        render_events(frame, body[0], app, options, palette);
        render_diagnostics(frame, body[1], app, options, palette);
    }
    render_footer(frame, rows[3], app, palette);

    if app.show_help {
        render_help(frame, centered_rect(64, 18, area), options, palette);
    }
}

fn render_header(frame: &mut Frame<'_>, area: Rect, app: &App, palette: Palette, now: Instant) {
    let (label, color) = match &app.connection {
        ConnectionState::Connecting => ("LINKING", palette.warning),
        ConnectionState::Connected => ("SYNCHRONIZED", palette.healthy),
        ConnectionState::Disconnected(_) if app.snapshot.is_some() => {
            ("SIGNAL LOST", palette.warning)
        }
        ConnectionState::Disconnected(_) => ("OFFLINE", palette.failure),
    };
    let stale = match (app.last_update, &app.connection) {
        (Some(last), ConnectionState::Disconnected(_)) => {
            format!(
                " // LAST SIGNAL {:.1}s AGO",
                now.saturating_duration_since(last).as_secs_f32()
            )
        }
        _ => String::new(),
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" CORE LINK: ", Style::default().fg(palette.muted)),
            Span::styled(
                label,
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(stale, Style::default().fg(palette.warning)),
        ])),
        area,
    );
}

fn render_metrics(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    options: RenderOptions,
    palette: Palette,
) {
    let columns = Layout::horizontal([Constraint::Ratio(1, 4); 4]).split(area);
    let (tick, cpu, uptime, health) = app.snapshot.as_ref().map_or_else(
        || {
            (
                "--".to_owned(),
                "--".to_owned(),
                "--".to_owned(),
                "UNKNOWN".to_owned(),
            )
        },
        |snapshot| {
            let observed = app
                .observed_tick_rate
                .map_or_else(|| "--".to_owned(), |rate| format!("{rate:.1}"));
            (
                format!(
                    "{}\n{} / {} Hz",
                    snapshot.tick, observed, snapshot.tick_rate_hz
                ),
                format_cpu(snapshot.cpu_usage_percent, options.ascii),
                format!("{}\n{}", format_uptime(snapshot.uptime_ms), snapshot.pid),
                format!("{:?}", snapshot.health).to_uppercase(),
            )
        },
    );
    metric(
        frame,
        columns[0],
        " TICK // HZ ",
        tick,
        options,
        palette.primary,
    );
    metric(
        frame,
        columns[1],
        " PROCESS CPU ",
        cpu,
        options,
        palette.primary,
    );
    metric(
        frame,
        columns[2],
        " SIMULATED POWER ",
        "OFFLINE\nPHASE 1.4".to_owned(),
        options,
        palette.warning,
    );
    metric(
        frame,
        columns[3],
        " UPTIME // PID ",
        format!("{uptime}\n{health}"),
        options,
        palette.healthy,
    );
}

fn metric(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    value: String,
    options: RenderOptions,
    color: Color,
) {
    frame.render_widget(
        Paragraph::new(value)
            .alignment(Alignment::Center)
            .style(Style::default().fg(color))
            .block(panel(title, options)),
        area,
    );
}

fn render_events(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    options: RenderOptions,
    palette: Palette,
) {
    let visible = usize::from(area.height.saturating_sub(2)).max(1);
    let end = app.events.len().saturating_sub(app.event_scroll);
    let start = end.saturating_sub(visible);
    let mut lines = Vec::new();
    for event in app
        .events
        .iter()
        .skip(start)
        .take(end.saturating_sub(start))
    {
        let label = match event.kind {
            EngineEventKind::DaemonStarted => "BOOT",
            EngineEventKind::DiagnosticPulse => "PULSE",
            EngineEventKind::ShutdownRequested => "HALT",
        };
        lines.push(Line::from(vec![
            Span::styled(
                event
                    .occurred_at
                    .with_timezone(&Local)
                    .format("%H:%M:%S ")
                    .to_string(),
                Style::default().fg(palette.muted),
            ),
            Span::styled(format!("{label:<5} "), Style::default().fg(palette.primary)),
            Span::raw(&event.message),
        ]));
    }
    if lines.is_empty() {
        lines.push(Line::styled(
            "Awaiting daemon event stream...",
            Style::default().fg(palette.muted),
        ));
    }
    let mode = if app.follow_events { "FOLLOW" } else { "HOLD" };
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(panel(&format!(" EVENT STREAM // {mode} "), options)),
        area,
    );
}

fn render_diagnostics(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    options: RenderOptions,
    palette: Palette,
) {
    let mut lines = Vec::new();
    if let Some(snapshot) = &app.snapshot {
        lines.push(Line::from(
            format!("HEALTH       {:?}", snapshot.health).to_uppercase(),
        ));
        lines.push(Line::from(format!(
            "CLIENTS      {}",
            snapshot.connected_clients
        )));
        lines.push(Line::from(format!("MCP          {}", snapshot.mcp_url)));
        lines.push(Line::from(""));
        if let Some(pulse) = &snapshot.last_diagnostic {
            lines.push(Line::styled(
                format!("PULSE {} // TICK {}", pulse.pulse_id, pulse.tick),
                Style::default().fg(palette.healthy),
            ));
            lines.push(Line::from(pulse.message.clone()));
            lines.push(Line::from(format!(
                "COMPONENTS {} // PATHS {}",
                pulse.components_checked, pulse.paths_traversed
            )));
        } else {
            lines.push(Line::styled(
                "NO DIAGNOSTIC PULSE RECORDED",
                Style::default().fg(palette.muted),
            ));
        }
    } else {
        lines.push(Line::styled(
            "NO DAEMON SNAPSHOT AVAILABLE",
            Style::default().fg(palette.warning),
        ));
    }
    if let ConnectionState::Disconnected(reason) = &app.connection {
        lines.push(Line::from(""));
        lines.push(Line::styled(reason, Style::default().fg(palette.failure)));
    }
    if let Some(notice) = &app.notice {
        lines.push(Line::from(""));
        lines.push(Line::styled(notice, Style::default().fg(palette.warning)));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(panel(" DIAGNOSTIC ARRAY ", options)),
        area,
    );
}

fn render_footer(frame: &mut Frame<'_>, area: Rect, app: &App, palette: Palette) {
    let follow = if app.follow_events {
        "FOLLOWING"
    } else {
        "LOG HELD"
    };
    frame.render_widget(
        Paragraph::new(format!(
            " q detach  d pulse  r reconnect  j/k scroll  f follow  ? help   // {follow}"
        ))
        .style(Style::default().fg(palette.muted)),
        area,
    );
}

fn render_too_small(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    options: RenderOptions,
    palette: Palette,
) {
    let state = match &app.connection {
        ConnectionState::Connecting => "LINKING",
        ConnectionState::Connected => "SYNCHRONIZED",
        ConnectionState::Disconnected(_) => "OFFLINE",
    };
    frame.render_widget(
        Paragraph::new(format!(
            "SUBSTRATE // {state}\n\nTerminal {}x{} is too small.\nResize to at least 60x18.\n\nq detach // r reconnect",
            area.width, area.height
        ))
        .alignment(Alignment::Center)
        .style(Style::default().fg(palette.warning))
        .block(panel(" AWAKENING MIND ", options)),
        area,
    );
}

fn render_help(frame: &mut Frame<'_>, area: Rect, options: RenderOptions, palette: Palette) {
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(
                "ATTACHMENT CONTROLS",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Line::from(""),
            Line::from("q / Ctrl-C    detach; daemon keeps running"),
            Line::from("d             trigger diagnostic pulse"),
            Line::from("r             retry daemon connection now"),
            Line::from("j/k or arrows scroll event stream"),
            Line::from("PgUp/PgDn     scroll ten events"),
            Line::from("Home/End       oldest/latest event"),
            Line::from("f             toggle event follow mode"),
            Line::from("? / Esc        close this overlay"),
            Line::from(""),
            Line::styled(
                "POWER REMAINS OFFLINE UNTIL CIRCUIT INITIALIZATION",
                Style::default().fg(palette.warning),
            ),
        ])
        .alignment(Alignment::Left)
        .block(
            panel(" HELP // LOCAL TERMINAL LINK ", options)
                .border_style(Style::default().fg(palette.primary)),
        ),
        area,
    );
}

fn panel<'a>(title: &'a str, options: RenderOptions) -> Block<'a> {
    if options.ascii {
        Block::default()
            .title(format!("[{}]", title.trim()))
            .padding(Padding::horizontal(1))
    } else {
        Block::default().title(title).borders(Borders::ALL)
    }
}

fn format_cpu(value: Option<f32>, ascii: bool) -> String {
    let Some(value) = value else {
        return "--\nSAMPLING".to_owned();
    };
    let filled = ((value.clamp(0.0, 100.0) / 10.0).round() as usize).min(10);
    let (on, off) = if ascii { ('#', '.') } else { ('█', '░') };
    let meter = std::iter::repeat_n(on, filled)
        .chain(std::iter::repeat_n(off, 10 - filled))
        .collect::<String>();
    format!("{value:.1}%\n{meter}")
}

fn format_uptime(milliseconds: u64) -> String {
    let seconds = milliseconds / 1_000;
    let hours = seconds / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let seconds = seconds % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width.saturating_sub(2));
    let height = height.min(area.height.saturating_sub(2));
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length((area.height - height) / 2),
            Constraint::Length(height),
            Constraint::Min(0),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length((area.width - width) / 2),
            Constraint::Length(width),
            Constraint::Min(0),
        ])
        .split(vertical[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use mind_core::protocol::{EngineEvent, HealthState, StatusSnapshot};
    use ratatui::{backend::TestBackend, Terminal};

    fn populated_app() -> App {
        let mut app = App::default();
        let event = EngineEvent {
            event_id: 1,
            occurred_at: Utc::now(),
            tick: 1,
            kind: EngineEventKind::DaemonStarted,
            message: "Mind daemon started".into(),
        };
        app.apply_snapshot(
            StatusSnapshot {
                protocol_version: 1,
                daemon_version: "0.1.0".into(),
                pid: 42,
                health: HealthState::Ready,
                started_at: Utc::now(),
                uptime_ms: 65_000,
                tick: 1_300,
                tick_rate_hz: 20,
                cpu_usage_percent: Some(4.2),
                connected_clients: 1,
                mcp_url: "http://127.0.0.1:7777/mcp".into(),
                last_diagnostic: None,
                recent_events: vec![event],
            },
            Instant::now(),
        );
        app
    }

    fn draw(width: u16, height: u16, app: &App, options: RenderOptions) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, app, options, Instant::now()))
            .unwrap();
        format!("{}", terminal.backend())
    }

    #[test]
    fn wide_dashboard_contains_core_panels() {
        let screen = draw(
            120,
            36,
            &populated_app(),
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        assert!(screen.contains("SUBSTRATE // AWAKENING MIND"));
        assert!(screen.contains("SIMULATED POWER"));
        assert!(screen.contains("Mind daemon started"));
    }

    #[test]
    fn ascii_and_small_terminal_modes_are_readable() {
        let screen = draw(
            80,
            24,
            &populated_app(),
            RenderOptions {
                ascii: true,
                no_color: true,
            },
        );
        assert!(screen.contains("[SUBSTRATE // AWAKENING MIND]"));
        assert!(!screen.contains('█'));

        let tiny = draw(
            50,
            12,
            &App::default(),
            RenderOptions {
                ascii: true,
                no_color: true,
            },
        );
        assert!(tiny.contains("Resize to at least 60x18"));
    }

    #[test]
    fn stale_and_help_states_are_visible() {
        let mut app = populated_app();
        app.disconnected("connection refused");
        app.show_help = true;
        let screen = draw(
            100,
            30,
            &app,
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        assert!(screen.contains("SIGNAL LOST"));
        assert!(screen.contains("ATTACHMENT CONTROLS"));
        assert!(screen.contains("daemon keeps running"));
    }
}
