use crate::{
    app::{App, ConnectionState, ContextAction, OnboardingView, Workspace},
    globe::{self, GlobeRenderOptions},
    theme::Theme,
};
use mind_core::protocol::{EngineEventKind, PlanetSnapshot, ScoutState};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PanelTone {
    Focused,
    Standard,
    Modal,
    Warning,
}

#[derive(Debug, Clone, Copy)]
struct KeyHint<'a> {
    key: &'a str,
    label: &'a str,
}

pub fn render(frame: &mut Frame<'_>, app: &App, options: RenderOptions, now: Instant) {
    let area = frame.area();
    let theme = Theme::new(options.no_color);
    frame.render_widget(Block::default().style(theme.canvas_style()), area);
    if area.width < 44 || area.height < 14 {
        render_too_small(frame, area, options, theme);
        return;
    }
    if app.snapshot.is_none() {
        render_connection_required(frame, area, app, options, theme);
    } else if app.onboarding_pending() {
        render_onboarding(frame, area, app, options, theme);
    } else {
        match app.workspace {
            Workspace::Planet => render_planet(frame, area, app, options, theme, now),
            Workspace::Operations => render_operations(frame, area, app, options, theme, now),
            Workspace::System => render_dashboard(frame, area, app, options, theme, now),
        }
    }
    if app.show_help {
        render_help(frame, area, options, theme);
    }
}

fn render_planet(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    options: RenderOptions,
    theme: Theme,
    now: Instant,
) {
    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(8),
        Constraint::Length(2),
    ])
    .split(area);
    render_workspace_header(frame, rows[0], Workspace::Planet, options, theme);
    let Some(planet) = &app.planet else {
        frame.render_widget(
            Paragraph::new("Awaiting authoritative planetary state...")
                .alignment(Alignment::Center)
                .style(theme.panel_style())
                .block(panel(" PLANET ", options, theme, PanelTone::Focused)),
            rows[1],
        );
        render_workspace_footer(
            frame,
            rows[2],
            app,
            now,
            &[
                KeyHint {
                    key: "Q",
                    label: "detach",
                },
                KeyHint {
                    key: "?",
                    label: "help",
                },
            ],
            theme,
        );
        return;
    };

    if area.width >= 72 {
        let columns = Layout::horizontal([Constraint::Percentage(62), Constraint::Percentage(38)])
            .split(rows[1]);
        render_globe(frame, columns[0], app, planet, options, theme);
        render_region_summary(frame, columns[1], app, options, theme);
    } else {
        let body = Layout::vertical([Constraint::Percentage(68), Constraint::Percentage(32)])
            .split(rows[1]);
        render_globe(frame, body[0], app, planet, options, theme);
        render_region_summary(frame, body[1], app, options, theme);
    }
    if app.detail_open {
        render_region_detail(frame, area, app, options, theme);
    }
    let controls: &[KeyHint<'_>] = if area.width < 72 {
        &[
            KeyHint {
                key: "ARROWS",
                label: "",
            },
            KeyHint {
                key: "ENTER",
                label: "",
            },
            KeyHint {
                key: "TAB",
                label: "",
            },
            KeyHint {
                key: "Q",
                label: "detach",
            },
        ]
    } else {
        &[
            KeyHint {
                key: "ARROWS",
                label: "navigate",
            },
            KeyHint {
                key: "ENTER",
                label: "details",
            },
            KeyHint {
                key: "TAB",
                label: "workspace",
            },
            KeyHint {
                key: "Q",
                label: "detach",
            },
            KeyHint {
                key: "?",
                label: "help",
            },
        ]
    };
    render_workspace_footer(frame, rows[2], app, now, controls, theme);
}

fn render_operations(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    options: RenderOptions,
    theme: Theme,
    now: Instant,
) {
    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(8),
        Constraint::Length(2),
    ])
    .split(area);
    render_workspace_header(frame, rows[0], Workspace::Operations, options, theme);
    let Some(planet) = &app.planet else {
        frame.render_widget(
            Paragraph::new("Awaiting authoritative operations state...")
                .alignment(Alignment::Center)
                .style(theme.panel_style())
                .block(panel(" OPERATIONS ", options, theme, PanelTone::Focused)),
            rows[1],
        );
        render_workspace_footer(
            frame,
            rows[2],
            app,
            now,
            &[
                KeyHint {
                    key: "Q",
                    label: "detach",
                },
                KeyHint {
                    key: "?",
                    label: "help",
                },
            ],
            theme,
        );
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
        let mut lines = vec![Line::from(vec![
            Span::styled(
                planet.scout.designation.clone(),
                Style::default().fg(theme.cyan_bright).bold(),
            ),
            Span::styled("  //  ", Style::default().fg(theme.faint)),
            Span::styled(
                format!("{:?}", planet.scout.state).to_uppercase(),
                Style::default().fg(theme.amber).bold(),
            ),
        ])];
        lines.extend([
            Line::from(""),
            field_line("TARGET", target_designation.to_owned(), theme),
            field_line("PROGRESS", format!("{progress:>3}%"), theme),
            field_line("REMAINING", format!("{remaining}s"), theme),
            Line::from(""),
        ]);
        lines.push(
            if matches!(app.context_action(), Some(ContextAction::Recall)) {
                key_line("ENTER", "recall scout", theme)
            } else {
                Line::styled(
                    "Scout is returning // no action required.",
                    Style::default().fg(theme.muted),
                )
            },
        );
        Text::from(lines)
    } else {
        let location_designation = region_designation(planet, &planet.scout.region_id);
        Text::from(vec![
            Line::from(vec![
                Span::styled(
                    planet.scout.designation.clone(),
                    Style::default().fg(theme.cyan_bright).bold(),
                ),
                Span::styled("  //  ", Style::default().fg(theme.faint)),
                Span::styled("READY", Style::default().fg(theme.success).bold()),
            ]),
            Line::from(""),
            field_line("LOCATION", location_designation.to_owned(), theme),
            Line::from(""),
            Line::styled(
                "Select a reachable region in Planet to deploy.",
                Style::default().fg(theme.muted),
            ),
        ])
    };
    frame.render_widget(
        Paragraph::new(mission_text)
            .wrap(Wrap { trim: true })
            .style(theme.panel_style())
            .block(panel(" ACTIVE SCOUT ", options, theme, PanelTone::Focused)),
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
                    Line::from(vec![
                        Span::styled(
                            format!("#{:03}  ", report.mission_id),
                            Style::default().fg(theme.faint),
                        ),
                        Span::styled(
                            region_designation.to_owned(),
                            Style::default().fg(theme.text_strong),
                        ),
                        Span::styled("  //  ", Style::default().fg(theme.faint)),
                        Span::styled(
                            format!("{:?}", report.outcome).to_uppercase(),
                            Style::default().fg(theme.success).bold(),
                        ),
                    ])
                })
                .collect::<Vec<_>>(),
        )
    };
    frame.render_widget(
        Paragraph::new(reports)
            .style(theme.panel_style())
            .block(panel(
                " RECENT REPORTS ",
                options,
                theme,
                PanelTone::Standard,
            )),
        columns[1],
    );
    let controls: &[KeyHint<'_>] = if area.width < 72 {
        &[
            KeyHint {
                key: "ENTER",
                label: "",
            },
            KeyHint {
                key: "TAB",
                label: "",
            },
            KeyHint {
                key: "Q",
                label: "detach",
            },
            KeyHint {
                key: "?",
                label: "",
            },
        ]
    } else {
        &[
            KeyHint {
                key: "ENTER",
                label: "action",
            },
            KeyHint {
                key: "TAB",
                label: "workspace",
            },
            KeyHint {
                key: "Q",
                label: "detach",
            },
            KeyHint {
                key: "?",
                label: "help",
            },
        ]
    };
    render_workspace_footer(frame, rows[2], app, now, controls, theme);
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
    theme: Theme,
) {
    let tab = |workspace, label| {
        if active == workspace {
            Span::styled(
                format!(" {label} "),
                theme.selection_style().fg(theme.cyan_bright),
            )
        } else {
            Span::styled(format!(" {label} "), Style::default().fg(theme.muted))
        }
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                if options.ascii { " # " } else { " ◆ " },
                Style::default().fg(theme.amber),
            ),
            Span::styled("SUBSTRATE ", Style::default().fg(theme.text_strong).bold()),
            Span::styled("// ", Style::default().fg(theme.faint)),
            tab(Workspace::Planet, "PLANET"),
            Span::raw(" "),
            tab(Workspace::Operations, "OPERATIONS"),
            Span::raw(" "),
            tab(Workspace::System, "SYSTEM"),
        ]))
        .style(theme.panel_style())
        .block(panel("", options, theme, PanelTone::Standard)),
        area,
    );
}

fn render_globe(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    planet: &PlanetSnapshot,
    options: RenderOptions,
    theme: Theme,
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
                theme,
            },
        ))
        .alignment(Alignment::Center)
        .style(theme.canvas_style())
        .block(panel(
            " REGIONAL GLOBE ",
            options,
            theme,
            PanelTone::Focused,
        )),
        area,
    );
}

fn render_region_summary(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    options: RenderOptions,
    theme: Theme,
) {
    let text = app.selected_region().map_or_else(
        || Text::from(Line::styled("No region selected.", theme.muted)),
        |region| {
            let mut lines = vec![designation_line(&region.designation, region.core, theme)];
            lines.push(field_line(
                "KNOWLEDGE",
                format!("{:?}", region.knowledge),
                theme,
            ));
            lines.push(field_line(
                "TERRAIN",
                region
                    .terrain
                    .map_or_else(|| "unresolved".into(), |value| format!("{value:?}")),
                theme,
            ));
            let reachable = region.within_scout_range || region.core;
            if area.height < 9 {
                lines.push(Line::from(vec![
                    Span::styled("RANGE       ", Style::default().fg(theme.muted)),
                    Span::styled(
                        format!("{} hop(s)", region.hop_distance_from_core),
                        Style::default().fg(theme.text_strong),
                    ),
                    Span::styled("  //  ", Style::default().fg(theme.faint)),
                    Span::styled(
                        if reachable {
                            "REACHABLE"
                        } else {
                            "OUT OF RANGE"
                        },
                        Style::default()
                            .fg(if reachable {
                                theme.success
                            } else {
                                theme.muted
                            })
                            .bold(),
                    ),
                ]));
            } else {
                lines.insert(1, Line::from(""));
                lines.push(field_line(
                    "RANGE",
                    format!("{} hop(s)", region.hop_distance_from_core),
                    theme,
                ));
                lines.push(Line::styled(
                    if reachable {
                        if options.ascii {
                            "+ REACHABLE"
                        } else {
                            "● REACHABLE"
                        }
                    } else if options.ascii {
                        "- BEYOND SCOUT RANGE"
                    } else {
                        "○ BEYOND SCOUT RANGE"
                    },
                    Style::default().fg(if reachable {
                        theme.success
                    } else {
                        theme.muted
                    }),
                ));
                lines.push(Line::from(""));
            }
            lines.push(key_line("ENTER", "region details", theme));
            Text::from(lines)
        },
    );
    frame.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: true })
            .style(theme.panel_style())
            .block(panel(
                " SELECTED REGION ",
                options,
                theme,
                PanelTone::Standard,
            )),
        area,
    );
}

fn render_region_detail(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    options: RenderOptions,
    theme: Theme,
) {
    let detail_area = centered_rect(58.min(area.width.saturating_sub(2)), 16, area);
    let text = app.selected_region().map_or_else(
        || Text::from(Line::styled("No region selected.", theme.muted)),
        |region| {
            let action = match app.context_action() {
                Some(ContextAction::Deploy(_)) => key_line("ENTER", "deploy Scout 01", theme),
                _ if region.core => Line::styled(
                    "Scout 01 is already based here.",
                    Style::default().fg(theme.muted),
                ),
                _ if !region.within_scout_range => Line::styled(
                    "Destination exceeds current scout range.",
                    Style::default().fg(theme.warning),
                ),
                _ => Line::styled(
                    "Scout unavailable // inspect Operations.",
                    Style::default().fg(theme.muted),
                ),
            };
            Text::from(vec![
                designation_line(&region.designation, region.core, theme),
                Line::from(""),
                field_line("KNOWLEDGE", format!("{:?}", region.knowledge), theme),
                field_line("TERRAIN", option_debug(region.terrain), theme),
                field_line("TEMPERATURE", option_debug(region.temperature), theme),
                field_line(
                    "ATMOSPHERE",
                    option_debug(region.atmospheric_severity),
                    theme,
                ),
                field_line("CONFIDENCE", option_debug(region.confidence), theme),
                field_line(
                    "DISTANCE",
                    format!("{} hop(s)", region.hop_distance_from_core),
                    theme,
                ),
                Line::from(""),
                action,
                key_line("ESC", "close", theme),
            ])
        },
    );
    frame.render_widget(Clear, detail_area);
    frame.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: true })
            .style(theme.raised_style())
            .block(panel(" REGION DETAIL ", options, theme, PanelTone::Modal)),
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
    controls: &[KeyHint<'_>],
    theme: Theme,
) {
    let mut spans = connection_spans(&app.connection, theme);
    if let Some(last) = app.last_update {
        if !matches!(app.connection, ConnectionState::Connected) {
            spans.extend([
                separator(theme),
                Span::styled(
                    format!(
                        "STALE {:.1}s",
                        now.saturating_duration_since(last).as_secs_f64()
                    ),
                    Style::default().fg(theme.warning).bold(),
                ),
            ]);
        }
    }
    if let Some(notice) = &app.notice {
        spans.extend([
            separator(theme),
            Span::styled(notice.clone(), Style::default().fg(theme.text)),
        ]);
    }
    let content = if area.width < 100 {
        Text::from(vec![
            Line::from(spans),
            Line::from(hint_spans(controls, theme)),
        ])
    } else {
        spans.push(separator(theme));
        spans.extend(hint_spans(controls, theme));
        Text::from(Line::from(spans))
    };
    frame.render_widget(
        Paragraph::new(content)
            .alignment(Alignment::Center)
            .style(theme.canvas_style()),
        area,
    );
}

fn render_connection_required(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    options: RenderOptions,
    theme: Theme,
) {
    let panel_area = centered_rect(58.min(area.width.saturating_sub(2)), 12, area);
    let detail = match &app.connection {
        ConnectionState::Connecting => "Awaiting authoritative daemon status...".to_owned(),
        ConnectionState::Connected => "Requesting authoritative daemon status...".to_owned(),
        ConnectionState::Disconnected(error) => format!("The daemon is unavailable.\n\n{error}"),
    };
    let text = Text::from(vec![
        Line::styled(
            "DAEMON CONNECTION REQUIRED",
            Style::default().fg(theme.text_strong).bold(),
        ),
        Line::from(""),
        Line::styled(detail, Style::default().fg(theme.text)),
        Line::from(""),
        Line::from(vec![
            Span::styled("Start mind-daemon, then ", Style::default().fg(theme.muted)),
            key_span("R", theme),
            Span::styled(" reconnect  ", Style::default().fg(theme.muted)),
            key_span("Q", theme),
            Span::styled(" detach", Style::default().fg(theme.muted)),
        ]),
    ]);
    frame.render_widget(Clear, panel_area);
    frame.render_widget(
        Paragraph::new(text)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true })
            .style(theme.raised_style())
            .block(panel(" ATTACHING ", options, theme, PanelTone::Warning)),
        panel_area,
    );
}

fn render_onboarding(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    options: RenderOptions,
    theme: Theme,
) {
    let title = Paragraph::new(Line::from(vec![
        Span::styled(
            if options.ascii { "# " } else { "◆ " },
            Style::default().fg(theme.amber),
        ),
        Span::styled("SUBSTRATE", Style::default().fg(theme.text_strong).bold()),
        Span::styled(" // COHERENCE", Style::default().fg(theme.muted)),
    ]))
    .alignment(Alignment::Center)
    .style(theme.canvas_style());
    let footer = Paragraph::new(connection_line(app, theme, area.width < 72))
        .alignment(Alignment::Center)
        .style(theme.canvas_style());
    let chunks = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(10),
        Constraint::Length(2),
    ])
    .split(area);
    frame.render_widget(title, chunks[0]);
    frame.render_widget(footer, chunks[2]);

    let panel_area = centered_rect(58.min(area.width.saturating_sub(2)), 20, chunks[1]);
    let panel = panel(" AWAKENING ", options, theme, PanelTone::Focused);
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
                    .alignment(Alignment::Center)
                    .style(theme.panel_style().fg(theme.text_strong)),
                rows[0],
            );
            frame.render_widget(
                Paragraph::new(ring_art(
                    app.band_positions,
                    app.selected_band,
                    options.ascii,
                    theme,
                ))
                .alignment(Alignment::Center)
                .style(theme.panel_style()),
                rows[1],
            );
            frame.render_widget(
                Paragraph::new(Line::from(hint_spans(
                    &[
                        KeyHint {
                            key: if options.ascii { "UP/DN" } else { "↑↓" },
                            label: "band",
                        },
                        KeyHint {
                            key: if options.ascii {
                                "LEFT/RIGHT"
                            } else {
                                "←→"
                            },
                            label: "adjust",
                        },
                        KeyHint {
                            key: "ENTER",
                            label: "observe",
                        },
                    ],
                    theme,
                )))
                .alignment(Alignment::Center)
                .style(theme.panel_style()),
                rows[2],
            );
            if let Some(notice) = &app.notice {
                frame.render_widget(
                    Paragraph::new(notice.as_str())
                        .alignment(Alignment::Center)
                        .style(theme.panel_style().fg(theme.warning).bold()),
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
            let mut lines = marker
                .into_iter()
                .map(|line| Line::styled(line, Style::default().fg(theme.cyan)))
                .collect::<Vec<_>>();
            lines.extend([
                Line::from(""),
                Line::styled(
                    "The interval holds.",
                    Style::default().fg(theme.text_strong).bold(),
                ),
                Line::from(""),
                Line::styled(
                    "It is easier to observe when the pattern returns.",
                    Style::default().fg(theme.text),
                ),
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
                        theme.cyan_bright
                    } else {
                        theme.warning
                    }),
                ),
            ]);
            frame.render_widget(
                Paragraph::new(Text::from(lines))
                    .alignment(Alignment::Center)
                    .wrap(Wrap { trim: true })
                    .style(theme.panel_style()),
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
    theme: Theme,
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
    render_workspace_header(frame, header_area, Workspace::System, options, theme);

    let (system, diagnostic) = if let Some(snapshot) = &app.snapshot {
        (
            Text::from(vec![
                field_line("HEALTH", format!("{:?}", snapshot.health), theme),
                field_line(
                    "TICK",
                    format!("{} @ {} Hz", snapshot.tick, snapshot.tick_rate_hz),
                    theme,
                ),
                field_line(
                    "OBSERVED",
                    app.observed_tick_rate
                        .map_or_else(|| "sampling".into(), |rate| format!("{rate:.1} Hz")),
                    theme,
                ),
                field_line(
                    "CPU",
                    snapshot
                        .cpu_usage_percent
                        .map_or_else(|| "unavailable".into(), |cpu| format!("{cpu:.1}%")),
                    theme,
                ),
                field_line("CLIENTS", snapshot.connected_clients.to_string(), theme),
            ]),
            snapshot.last_diagnostic.as_ref().map_or_else(
                || {
                    Text::from(vec![
                        Line::styled(
                            "No diagnostic pulse recorded.",
                            Style::default().fg(theme.muted),
                        ),
                        key_line("D", "run diagnostic pulse", theme),
                    ])
                },
                |pulse| {
                    Text::from(vec![
                        field_line("PULSE", pulse.pulse_id.to_string(), theme),
                        field_line("TICK", pulse.tick.to_string(), theme),
                        field_line("HEALTH", format!("{:?}", pulse.health), theme),
                        Line::styled(pulse.message.clone(), Style::default().fg(theme.text)),
                    ])
                },
            ),
        )
    } else {
        (
            Text::from(Line::styled(
                "Awaiting authoritative status...",
                Style::default().fg(theme.muted),
            )),
            Text::from(Line::styled(
                "Diagnostics unavailable.",
                Style::default().fg(theme.muted),
            )),
        )
    };
    frame.render_widget(
        Paragraph::new(system)
            .style(theme.panel_style())
            .block(panel(" DAEMON ", options, theme, PanelTone::Standard)),
        system_area,
    );
    frame.render_widget(
        Paragraph::new(diagnostic)
            .wrap(Wrap { trim: true })
            .style(theme.panel_style())
            .block(panel(" DIAGNOSTIC ", options, theme, PanelTone::Standard)),
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
            let tag_color = match event.kind {
                EngineEventKind::PersistenceDegraded => theme.danger,
                EngineEventKind::ShutdownRequested | EngineEventKind::SaveReset => theme.warning,
                EngineEventKind::DiagnosticPulse
                | EngineEventKind::OnboardingCompleted
                | EngineEventKind::SurveyCompleted
                | EngineEventKind::PersistenceRecovered => theme.success,
                _ => theme.cyan,
            };
            Line::from(vec![
                Span::styled(
                    format!("{:>6} ", event.tick),
                    Style::default().fg(theme.faint),
                ),
                Span::styled(format!("{:>5}", tag), Style::default().fg(tag_color).bold()),
                Span::styled(
                    format!("  {}", event.message),
                    Style::default().fg(theme.text),
                ),
            ])
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(lines)
            .style(theme.panel_style())
            .block(panel(" ENGINE EVENTS ", options, theme, PanelTone::Focused)),
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
    let mut footer = connection_spans(&app.connection, theme);
    if !stale_text.is_empty() {
        footer.extend([
            separator(theme),
            Span::styled(
                stale_text.trim_start_matches(" // ").to_owned(),
                Style::default().fg(theme.warning).bold(),
            ),
        ]);
    }
    let hints: &[KeyHint<'_>] = if footer_area.width < 100 {
        &[
            KeyHint {
                key: "D",
                label: "",
            },
            KeyHint {
                key: "UP/DN",
                label: "",
            },
            KeyHint {
                key: "F",
                label: "",
            },
            KeyHint {
                key: "TAB",
                label: "",
            },
            KeyHint {
                key: "?",
                label: "",
            },
            KeyHint {
                key: "Q",
                label: "detach",
            },
        ]
    } else {
        &[
            KeyHint {
                key: "D",
                label: "diagnostic",
            },
            KeyHint {
                key: "UP/DN",
                label: "events",
            },
            KeyHint {
                key: "F",
                label: "follow",
            },
            KeyHint {
                key: "TAB",
                label: "workspace",
            },
            KeyHint {
                key: "?",
                label: "help",
            },
            KeyHint {
                key: "Q",
                label: "detach",
            },
        ]
    };
    let content = if footer_area.width < 100 {
        Text::from(vec![
            Line::from(footer),
            Line::from(hint_spans(hints, theme)),
        ])
    } else {
        footer.push(separator(theme));
        footer.extend(hint_spans(hints, theme));
        Text::from(Line::from(footer))
    };
    frame.render_widget(
        Paragraph::new(content)
            .alignment(Alignment::Center)
            .style(theme.canvas_style()),
        footer_area,
    );
}

fn ring_art(positions: [u8; 3], selected: usize, ascii: bool, theme: Theme) -> Text<'static> {
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
                                .fg(theme.cyan_bright)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(theme.border)
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

fn connection_line(app: &App, theme: Theme, compact: bool) -> Line<'static> {
    let mut spans = connection_spans(&app.connection, theme);
    spans.push(separator(theme));
    let hints: &[KeyHint<'_>] = if compact {
        &[
            KeyHint {
                key: "R",
                label: "",
            },
            KeyHint {
                key: "?",
                label: "",
            },
            KeyHint {
                key: "Q",
                label: "detach",
            },
        ]
    } else {
        &[
            KeyHint {
                key: "R",
                label: "reconnect",
            },
            KeyHint {
                key: "?",
                label: "help",
            },
            KeyHint {
                key: "Q",
                label: "detach",
            },
        ]
    };
    spans.extend(hint_spans(hints, theme));
    Line::from(spans)
}

fn connection_label(state: &ConnectionState) -> &'static str {
    match state {
        ConnectionState::Connecting => "CONNECTING",
        ConnectionState::Connected => "CONNECTED",
        ConnectionState::Disconnected(_) => "DISCONNECTED",
    }
}

fn render_too_small(frame: &mut Frame<'_>, area: Rect, options: RenderOptions, theme: Theme) {
    frame.render_widget(
        Paragraph::new(format!(
            "Substrate needs at least 44x14.\nCurrent terminal: {}x{}",
            area.width, area.height
        ))
        .alignment(Alignment::Center)
        .style(theme.raised_style())
        .block(panel(
            " RESIZE // MIN 44x14 ",
            options,
            theme,
            PanelTone::Warning,
        )),
        area,
    );
}

fn render_help(frame: &mut Frame<'_>, area: Rect, options: RenderOptions, theme: Theme) {
    let help_area = centered_rect(
        54.min(area.width.saturating_sub(2)),
        18.min(area.height.saturating_sub(2)),
        area,
    );
    let help = Text::from(vec![
        section_line("GLOBAL", theme),
        key_line("Q / CTRL-C", "detach without stopping daemon", theme),
        key_line("R", "reconnect when disconnected", theme),
        key_line("TAB", "cycle workspace", theme),
        key_line("? / ESC", "close help", theme),
        Line::from(""),
        section_line("PLANET", theme),
        key_line("ARROWS", "navigate regions", theme),
        key_line("ENTER", "details / deploy", theme),
        key_line("ESC", "close details", theme),
        Line::from(""),
        section_line("OPERATIONS", theme),
        key_line("ENTER", "recall active scout", theme),
    ]);
    frame.render_widget(Clear, help_area);
    frame.render_widget(
        Paragraph::new(help)
            .wrap(Wrap { trim: false })
            .style(theme.raised_style())
            .block(panel(" HELP ", options, theme, PanelTone::Modal)),
        help_area,
    );
}

fn panel<'a>(title: &'a str, options: RenderOptions, theme: Theme, tone: PanelTone) -> Block<'a> {
    let (surface, border_color, title_color, emphasized) = match tone {
        PanelTone::Focused => (theme.panel, theme.cyan, theme.cyan_bright, true),
        PanelTone::Standard => (theme.panel, theme.border, theme.muted, false),
        PanelTone::Modal => (theme.raised, theme.amber, theme.text_strong, true),
        PanelTone::Warning => (theme.raised, theme.warning, theme.warning, true),
    };
    let mut border_style = Style::default().fg(border_color);
    let mut title_style = Style::default().fg(title_color);
    if emphasized {
        border_style = border_style.add_modifier(Modifier::BOLD);
        title_style = title_style.add_modifier(Modifier::BOLD);
    }
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_set(if options.ascii {
            ASCII_BORDER
        } else {
            border::ROUNDED
        })
        .style(theme.on(surface).fg(theme.text))
        .border_style(border_style)
        .title_style(title_style);
    if !title.is_empty() {
        block = block.title(title);
    }
    block
}

fn field_line(label: &'static str, value: String, theme: Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<12}"), Style::default().fg(theme.muted)),
        Span::styled(value, Style::default().fg(theme.text_strong)),
    ])
}

fn designation_line(designation: &str, core: bool, theme: Theme) -> Line<'static> {
    let mut spans = vec![Span::styled(
        designation.to_owned(),
        Style::default().fg(theme.cyan_bright).bold(),
    )];
    if core {
        spans.push(Span::styled(
            "  // CORE",
            Style::default().fg(theme.amber).bold(),
        ));
    }
    Line::from(spans)
}

fn key_span(key: &str, theme: Theme) -> Span<'static> {
    Span::styled(format!(" {key} "), theme.key_style())
}

fn key_line(key: &str, label: &str, theme: Theme) -> Line<'static> {
    Line::from(vec![
        key_span(key, theme),
        Span::styled(format!(" {label}"), Style::default().fg(theme.text)),
    ])
}

fn section_line(label: &str, theme: Theme) -> Line<'static> {
    Line::styled(
        format!("{label} //"),
        Style::default().fg(theme.cyan_bright).bold(),
    )
}

fn hint_spans(hints: &[KeyHint<'_>], theme: Theme) -> Vec<Span<'static>> {
    let mut spans = Vec::with_capacity(hints.len() * 3);
    for (index, hint) in hints.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(key_span(hint.key, theme));
        if !hint.label.is_empty() {
            spans.push(Span::styled(
                format!(" {}", hint.label),
                Style::default().fg(theme.muted),
            ));
        }
    }
    spans
}

fn connection_spans(state: &ConnectionState, theme: Theme) -> Vec<Span<'static>> {
    let color = match state {
        ConnectionState::Connecting => theme.amber,
        ConnectionState::Connected => theme.cyan,
        ConnectionState::Disconnected(_) => theme.danger,
    };
    vec![
        Span::styled("* ", Style::default().fg(color)),
        Span::styled(connection_label(state), Style::default().fg(color).bold()),
    ]
}

fn separator(theme: Theme) -> Span<'static> {
    Span::styled("  //  ", Style::default().fg(theme.faint))
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

    fn cells(
        app: &App,
        width: u16,
        height: u16,
        options: RenderOptions,
    ) -> Vec<ratatui::buffer::Cell> {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, app, options, Instant::now()))
            .unwrap();
        terminal.backend().buffer().content().to_vec()
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
        assert!(ascii.contains("ENTER") && ascii.contains("observe"));
        assert!(ascii.contains('+'));
        assert!(ascii.is_ascii());
    }

    #[test]
    fn substrate_dark_applies_canvas_surfaces_and_semantic_accents() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([2, 2, 2]);
        let mut app = App::default();
        app.apply_snapshot(engine.status(), Instant::now());
        app.apply_planet(engine.planet().unwrap(), Instant::now());
        app.connected();

        let theme = Theme::new(false);
        let cells = cells(
            &app,
            100,
            30,
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        assert!(cells.iter().any(|cell| cell.bg == theme.canvas));
        assert!(cells.iter().any(|cell| cell.bg == theme.panel));
        assert!(cells.iter().any(|cell| cell.fg == theme.cyan));
        assert!(cells.iter().any(|cell| cell.fg == theme.amber));
        assert!(cells.iter().any(|cell| cell.fg == theme.muted));
    }

    #[test]
    fn no_color_ascii_frame_never_forces_color_or_unicode() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([2, 2, 2]);
        let mut app = App::default();
        app.apply_snapshot(engine.status(), Instant::now());
        app.apply_planet(engine.planet().unwrap(), Instant::now());
        app.connected();

        let options = RenderOptions {
            ascii: true,
            no_color: true,
        };
        let output = screen(&app, 50, 28, options);
        assert!(output.is_ascii());
        assert!(cells(&app, 50, 28, options)
            .iter()
            .all(|cell| cell.fg == ratatui::style::Color::Reset
                && cell.bg == ratatui::style::Color::Reset));
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
        assert!(planet.contains("detach"));
        assert!(planet.contains("help"));

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
        assert!(narrow.contains("REACHABLE"));
        assert!(narrow.contains("region details"));
        assert!(narrow.contains("detach"));
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
