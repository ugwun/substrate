use crate::{
    app::{App, ConnectionState, ContextAction, OnboardingView, Workspace},
    globe::{self, GlobeRenderOptions},
    theme::Theme,
};
use mind_core::protocol::{EngineEventKind, MissionReportOutcome, PlanetSnapshot, ScoutState};
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
            Workspace::Overview => render_overview(frame, area, app, options, theme, now),
            Workspace::Research => render_research(frame, area, app, options, theme, now),
            Workspace::System => render_dashboard(frame, area, app, options, theme, now),
        }
    }
    if app.show_return_summary {
        render_return_summary(frame, area, app, options, theme);
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
        Constraint::Length(5),
        Constraint::Min(8),
        Constraint::Length(2),
    ])
    .split(area);
    render_workspace_header(frame, rows[0], app, Workspace::Planet, options, theme);
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

    if area.width >= 100 {
        let columns = Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)])
            .split(rows[1]);
        render_globe(frame, columns[0], app, planet, options, theme);
        render_region_inspector(frame, columns[1], app, planet, options, theme);
    } else {
        let body = Layout::vertical([Constraint::Percentage(56), Constraint::Percentage(44)])
            .split(rows[1]);
        render_globe(frame, body[0], app, planet, options, theme);
        render_region_inspector(frame, body[1], app, planet, options, theme);
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
    } else if area.width < 140 {
        &[
            KeyHint {
                key: "ARROWS",
                label: "",
            },
            KeyHint {
                key: "ENTER",
                label: "action",
            },
            KeyHint {
                key: "PGUP/DN",
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
                label: "help",
            },
        ]
    } else {
        &[
            KeyHint {
                key: "ARROWS",
                label: "region",
            },
            KeyHint {
                key: "ENTER",
                label: "primary action",
            },
            KeyHint {
                key: "PGUP/DN",
                label: "inspect",
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

fn render_overview(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    options: RenderOptions,
    theme: Theme,
    now: Instant,
) {
    let rows = Layout::vertical([
        Constraint::Length(4),
        Constraint::Min(8),
        Constraint::Length(2),
    ])
    .split(area);
    render_workspace_header(frame, rows[0], app, Workspace::Overview, options, theme);
    let Some(planet) = &app.planet else {
        frame.render_widget(
            Paragraph::new("Awaiting authoritative planetary overview...")
                .alignment(Alignment::Center)
                .style(theme.panel_style())
                .block(panel(" OVERVIEW ", options, theme, PanelTone::Focused)),
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
    let columns = if area.width >= 100 {
        Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)]).split(rows[1])
    } else {
        Layout::vertical([Constraint::Percentage(58), Constraint::Percentage(42)]).split(rows[1])
    };

    let mut activity = Vec::new();
    let mut row_index = 0;
    let mut has_active_operation = false;
    let mut selected_line = 0;
    activity.push(section_line("ACTIVE OPERATIONS", theme));
    if let Some(research) = &planet.research.active {
        has_active_operation = true;
        let title = planet
            .research
            .options
            .iter()
            .find(|option| option.project == research.project)
            .map_or("Research", |option| option.title.as_str());
        activity.push(Line::styled(
            format!(
                "  {title} // {}% // {}s remaining",
                operation_progress(research.remaining_ms, research.phase_total_ms),
                research.remaining_ms.div_ceil(1_000)
            ),
            Style::default().fg(theme.cyan_bright).bold(),
        ));
    }
    if let Some(mission) = &planet.active_mission {
        has_active_operation = true;
        let target_designation = region_designation(planet, &mission.target_region_id);
        let progress = if mission.phase == ScoutState::Holding || mission.phase_total_ms == 0 {
            100
        } else {
            100_u64
                .saturating_sub(mission.remaining_ms.saturating_mul(100) / mission.phase_total_ms)
        };
        if app.overview_selection == row_index {
            selected_line = activity.len();
        }
        activity.push(Line::from(vec![
            overview_marker(app.overview_selection == row_index, options, theme),
            Span::styled(
                format!("{} #{:03}", planet.scout.designation, mission.mission_id),
                Style::default().fg(theme.cyan_bright).bold(),
            ),
            Span::styled("  //  ", Style::default().fg(theme.faint)),
            Span::styled(
                format!("{:?}", planet.scout.state).to_uppercase(),
                Style::default().fg(theme.amber).bold(),
            ),
        ]));
        activity.push(Line::styled(
            format!(
                "   {target_designation} // {progress}% // {}{}",
                scout_remaining_text(mission),
                if mission.automated { " // AUTO" } else { "" }
            ),
            Style::default().fg(theme.muted),
        ));
        row_index += 1;
    }
    if let Some(mission) = &planet.active_extraction {
        has_active_operation = true;
        if app.overview_selection == row_index {
            selected_line = activity.len();
        }
        activity.push(Line::from(vec![
            overview_marker(app.overview_selection == row_index, options, theme),
            Span::styled(
                format!(
                    "{} #{:03}",
                    planet.extractor.designation, mission.mission_id
                ),
                Style::default().fg(theme.cyan_bright).bold(),
            ),
            Span::styled("  //  ", Style::default().fg(theme.faint)),
            Span::styled(
                format!("{:?}", mission.phase).to_uppercase(),
                Style::default().fg(theme.amber).bold(),
            ),
        ]));
        activity.push(Line::styled(
            format!(
                "   {} // {}% // {}s remaining{}",
                region_designation(planet, &mission.target_region_id),
                operation_progress(mission.remaining_ms, mission.phase_total_ms),
                mission.remaining_ms.div_ceil(1_000),
                if mission.automated { " // AUTO" } else { "" }
            ),
            Style::default().fg(theme.muted),
        ));
        activity.push(Line::styled(
            format!(
                "   {} // capacity {} units",
                risk_text(mission.risk),
                mission.cargo_capacity
            ),
            Style::default().fg(theme.muted),
        ));
        row_index += 1;
    }
    if let Some(construction) = &planet.active_construction {
        has_active_operation = true;
        if app.overview_selection == row_index {
            selected_line = activity.len();
        }
        activity.push(Line::from(vec![
            overview_marker(app.overview_selection == row_index, options, theme),
            Span::styled("RANGE RELAY", Style::default().fg(theme.cyan_bright).bold()),
            Span::styled("  //  ", Style::default().fg(theme.faint)),
            Span::styled("CONSTRUCTING", Style::default().fg(theme.amber).bold()),
        ]));
        activity.push(Line::styled(
            format!(
                "   {} // {}% // {}s remaining",
                region_designation(planet, &construction.region_id),
                operation_progress(construction.remaining_ms, construction.phase_total_ms),
                construction.remaining_ms.div_ceil(1_000)
            ),
            Style::default().fg(theme.muted),
        ));
        row_index += 1;
    }
    if !has_active_operation {
        activity.push(Line::styled(
            "No active field operations.",
            Style::default().fg(theme.muted),
        ));
    }
    activity.push(Line::from(""));
    activity.push(section_line("RECENT REPORTS", theme));
    if planet.recent_reports.is_empty() {
        activity.push(Line::styled(
            "No mission reports recorded.",
            Style::default().fg(theme.muted),
        ));
    } else {
        for report in planet.recent_reports.iter().rev() {
            if app.overview_selection == row_index {
                selected_line = activity.len();
            }
            activity.push(Line::from(vec![
                overview_marker(app.overview_selection == row_index, options, theme),
                Span::styled(
                    format!("#{:03}  ", report.mission_id),
                    Style::default().fg(theme.faint),
                ),
                Span::styled(
                    region_designation(planet, &report.region_id).to_owned(),
                    Style::default().fg(theme.text_strong),
                ),
                Span::styled("  //  ", Style::default().fg(theme.faint)),
                Span::styled(
                    report_outcome_label(report.outcome),
                    Style::default()
                        .fg(if report.outcome == MissionReportOutcome::ExtractorLost {
                            theme.danger
                        } else {
                            theme.success
                        })
                        .bold(),
                ),
            ]));
            row_index += 1;
        }
    }
    let activity_height = columns[0].height.saturating_sub(2) as usize;
    let activity_scroll = selected_line.saturating_sub(activity_height.saturating_sub(1)) as u16;
    frame.render_widget(
        Paragraph::new(Text::from(activity))
            .wrap(Wrap { trim: true })
            .scroll((activity_scroll, 0))
            .style(theme.panel_style())
            .block(panel(
                " PLANETARY ACTIVITY ",
                options,
                theme,
                PanelTone::Focused,
            )),
        columns[0],
    );

    let scout_location = planet.active_mission.as_ref().map_or_else(
        || region_designation(planet, &planet.scout.region_id),
        |mission| region_designation(planet, &mission.target_region_id),
    );
    let extractor_status = if let Some(mission) = &planet.active_extraction {
        format!(
            "{:?} // {}",
            mission.phase,
            region_designation(planet, &mission.target_region_id)
        )
    } else if planet.extractor.rebuild_remaining_ms > 0 {
        format!(
            "Rebuilding // {}s",
            planet.extractor.rebuild_remaining_ms.div_ceil(1_000)
        )
    } else {
        format!("{:?} // core", planet.extractor.state)
    };
    let relay_status = planet.relay_region_id.as_ref().map_or_else(
        || "not constructed".to_owned(),
        |region_id| format!("active // {}", region_designation(planet, region_id)),
    );
    let stock = planet.refined_inventory;
    let raw = planet.raw_inventory;
    let construction_status = planet.active_construction.as_ref().map_or_else(
        || {
            planet.relay_region_id.as_ref().map_or_else(
                || "idle // no relay constructed".to_owned(),
                |region_id| format!("relay active // {}", region_designation(planet, region_id)),
            )
        },
        |construction| {
            format!(
                "relay {}% // {} // {}s",
                operation_progress(construction.remaining_ms, construction.phase_total_ms),
                region_designation(planet, &construction.region_id),
                construction.remaining_ms.div_ceil(1_000)
            )
        },
    );
    let active_profile = planet
        .research
        .profiles
        .iter()
        .find(|profile| profile.active)
        .map_or("Balanced", |profile| profile.title.as_str());
    let survey_automation = if planet.survey_queue.unlocked {
        format!(
            "{} / {} queued",
            planet.survey_queue.queued_region_ids.len(),
            planet.survey_queue.capacity
        )
    } else {
        "locked // research required".into()
    };
    let extraction_automation = if !planet.extraction_route.unlocked {
        "locked // research required".into()
    } else if let Some(region_id) = &planet.extraction_route.region_id {
        let status = planet
            .extraction_route
            .paused_reason
            .as_ref()
            .map_or("active".into(), |reason| {
                format!("paused: {}", reason.replace('_', " "))
            });
        format!("{status} // {}", region_designation(planet, region_id))
    } else {
        "earned // no route active".into()
    };
    let summary = Text::from(vec![
        section_line("MATERIAL STORES", theme),
        field_line(
            "STRUCTURAL",
            format!(
                "raw {} // refined {} // relay needs 8",
                raw.structural, stock.structural
            ),
            theme,
        ),
        field_line(
            "CONDUCTIVE",
            format!(
                "raw {} // refined {} // relay needs 6",
                raw.conductive, stock.conductive
            ),
            theme,
        ),
        field_line(
            "SILICATE",
            format!(
                "raw {} // refined {} // relay needs 6",
                raw.silicate, stock.silicate
            ),
            theme,
        ),
        field_line(
            "REFINING",
            if planet.refining_remaining_ms == 0 {
                "idle".into()
            } else {
                format!(
                    "{}s to next unit",
                    planet.refining_remaining_ms.div_ceil(1_000)
                )
            },
            theme,
        ),
        Line::from(""),
        section_line("FLEET & NETWORK", theme),
        field_line(
            "SCOUT",
            format!("{:?} // {scout_location}", planet.scout.state),
            theme,
        ),
        field_line("EXTRACTOR", extractor_status, theme),
        field_line("RELAY", relay_status, theme),
        Line::from(""),
        section_line("RESEARCH & AUTOMATION", theme),
        field_line("PROFILE", active_profile.to_owned(), theme),
        field_line(
            "ANALYSIS",
            format!("{} available", planet.research.analysis_units),
            theme,
        ),
        field_line("SURVEYS", survey_automation, theme),
        field_line("EXTRACTION", extraction_automation, theme),
        Line::from(""),
        section_line("FABRICATION", theme),
        Line::styled(construction_status, Style::default().fg(theme.text_strong)),
        Line::from(""),
        Line::styled(
            "Select an operation or report and press Enter to inspect its region.",
            Style::default().fg(theme.muted),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(summary)
            .wrap(Wrap { trim: true })
            .scroll((app.overview_status_scroll, 0))
            .style(theme.panel_style())
            .block(panel(
                " PLANET STATUS ",
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
                label: "inspect region",
            },
            KeyHint {
                key: "ARROWS",
                label: "select",
            },
            KeyHint {
                key: "PGUP/DN",
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

fn render_research(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    options: RenderOptions,
    theme: Theme,
    now: Instant,
) {
    let rows = Layout::vertical([
        Constraint::Length(4),
        Constraint::Min(8),
        Constraint::Length(2),
    ])
    .split(area);
    render_workspace_header(frame, rows[0], app, Workspace::Research, options, theme);
    let Some(planet) = &app.planet else {
        frame.render_widget(
            Paragraph::new("Awaiting authoritative research state...")
                .alignment(Alignment::Center)
                .style(theme.panel_style())
                .block(panel(" RESEARCH ", options, theme, PanelTone::Focused)),
            rows[1],
        );
        return;
    };
    let research = &planet.research;
    let mut lines = vec![field_line(
        "ANALYSIS",
        format!("{} units available", research.analysis_units),
        theme,
    )];
    if !research.unlocked {
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            "Research planning will become available after extended reach is proven.",
            Style::default().fg(theme.muted),
        ));
    } else {
        if let Some(active) = &research.active {
            let title = research
                .options
                .iter()
                .find(|option| option.project == active.project)
                .map_or("Active research", |option| option.title.as_str());
            lines.push(field_line(
                "ACTIVE",
                format!(
                    "{title} // {}% // {}s",
                    operation_progress(active.remaining_ms, active.phase_total_ms),
                    active.remaining_ms.div_ceil(1_000)
                ),
                theme,
            ));
        }
        lines.push(Line::raw(""));
        lines.push(section_line("RESEARCH DIRECTIONS", theme));
        for (index, option) in research.options.iter().enumerate() {
            let marker = if index == app.research_selection {
                ">"
            } else {
                " "
            };
            let state = if option.completed {
                "COMPLETED"
            } else if option.available {
                "READY"
            } else {
                "LOCKED"
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{marker} {:<22}", option.title),
                    Style::default().fg(theme.text),
                ),
                Span::styled(
                    format!(" {state:<9}"),
                    Style::default().fg(if option.available {
                        theme.amber
                    } else {
                        theme.muted
                    }),
                ),
                Span::styled(
                    format!(" {} analysis", option.analysis_cost),
                    Style::default().fg(theme.faint),
                ),
            ]));
            lines.push(Line::styled(
                format!("    {}", option.summary),
                Style::default().fg(theme.muted),
            ));
        }
        lines.push(Line::raw(""));
        lines.push(section_line("OPERATIONAL PROFILES", theme));
        let offset = research.options.len();
        let field_operation_active =
            planet.active_mission.is_some() || planet.active_extraction.is_some();
        for (index, profile) in research.profiles.iter().enumerate() {
            let marker = if offset + index == app.research_selection {
                ">"
            } else {
                " "
            };
            let state = if profile.active {
                "ACTIVE"
            } else if profile.unlocked && field_operation_active {
                "WAIT"
            } else if profile.unlocked {
                "AVAILABLE"
            } else {
                "LOCKED"
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{marker} {:<16}", profile.title),
                    Style::default().fg(theme.text),
                ),
                Span::styled(
                    format!(" {state:<9} "),
                    Style::default().fg(if profile.active {
                        theme.success
                    } else if profile.unlocked {
                        theme.cyan
                    } else {
                        theme.muted
                    }),
                ),
                Span::styled(profile.summary.clone(), Style::default().fg(theme.muted)),
            ]));
        }
        if field_operation_active {
            lines.push(Line::styled(
                "    Profiles can be changed when survey and extraction probes are not in flight.",
                Style::default().fg(theme.muted),
            ));
        }
    }
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .style(theme.panel_style())
            .block(panel(
                " RESEARCH & SPECIALIZATION ",
                options,
                theme,
                PanelTone::Focused,
            )),
        rows[1],
    );
    render_workspace_footer(
        frame,
        rows[2],
        app,
        now,
        &[
            KeyHint {
                key: "UP/DOWN",
                label: "select",
            },
            KeyHint {
                key: "ENTER",
                label: "commit / activate",
            },
            KeyHint {
                key: "TAB",
                label: "workspace",
            },
            KeyHint {
                key: "Q",
                label: "detach",
            },
        ],
        theme,
    );
}

fn render_return_summary(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    options: RenderOptions,
    theme: Theme,
) {
    let Some(planet) = &app.planet else {
        return;
    };
    let width = area.width.saturating_sub(8).clamp(36, 84);
    let height = (planet.unreviewed_activity.len() as u16 + 13)
        .min(area.height.saturating_sub(4))
        .max(10);
    let popup = centered_rect(width, height, area);
    frame.render_widget(Clear, popup);
    let mut lines = vec![
        Line::styled(
            "While the daemon remained active:",
            Style::default().fg(theme.text).bold(),
        ),
        Line::raw(""),
    ];
    if planet.unreviewed_activity.is_empty() {
        lines.push(Line::styled(
            "No unreviewed operational changes.",
            Style::default().fg(theme.muted),
        ));
    } else {
        for record in planet
            .unreviewed_activity
            .iter()
            .rev()
            .take(height.saturating_sub(7) as usize)
            .rev()
        {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("#{:03} ", record.activity_id),
                    Style::default().fg(theme.faint),
                ),
                Span::styled(record.summary.clone(), Style::default().fg(theme.text)),
            ]));
        }
    }
    let raw = planet.raw_inventory;
    let refined = planet.refined_inventory;
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        format!(
            "STORES NOW  raw S {} / C {} / Si {} // refined S {} / C {} / Si {}",
            raw.structural,
            raw.conductive,
            raw.silicate,
            refined.structural,
            refined.conductive,
            refined.silicate
        ),
        Style::default().fg(theme.text),
    ));
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        format!(
            "CURRENT  {} // {}",
            planet.objective.title, planet.objective.guidance
        ),
        Style::default().fg(theme.amber),
    ));
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        "NEXT  Planet: inspect a reachable region and choose its local operation.",
        Style::default().fg(theme.text),
    ));
    lines.push(Line::styled(
        "      Overview: review active work, stores, and visible blockers.",
        Style::default().fg(theme.text),
    ));
    if planet.research.unlocked {
        lines.push(Line::styled(
            "      Research: review analysis, active work, and available profiles.",
            Style::default().fg(theme.text),
        ));
    }
    lines.push(Line::styled(
        "Enter / Esc  acknowledge and continue",
        Style::default().fg(theme.cyan),
    ));
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .style(theme.raised_style())
            .block(panel(" RETURN SUMMARY ", options, theme, PanelTone::Modal)),
        popup,
    );
}

fn overview_marker(selected: bool, options: RenderOptions, theme: Theme) -> Span<'static> {
    Span::styled(
        if selected {
            if options.ascii {
                "> "
            } else {
                "› "
            }
        } else {
            "  "
        },
        Style::default()
            .fg(if selected { theme.amber } else { theme.faint })
            .bold(),
    )
}

fn operation_progress(remaining_ms: u64, total_ms: u64) -> u64 {
    remaining_ms
        .saturating_mul(100)
        .checked_div(total_ms)
        .map_or(100, |remaining| 100_u64.saturating_sub(remaining))
}

fn region_designation<'a>(planet: &'a PlanetSnapshot, region_id: &'a str) -> &'a str {
    planet
        .regions
        .iter()
        .find(|region| region.id == region_id)
        .map_or(region_id, |region| region.designation.as_str())
}

fn report_outcome_label(outcome: MissionReportOutcome) -> &'static str {
    match outcome {
        MissionReportOutcome::SurveyCompleted => "SURVEY COMPLETE",
        MissionReportOutcome::Recalled => "RECALLED",
        MissionReportOutcome::Returned => "RETURNED",
        MissionReportOutcome::ExtractionCompleted => "CARGO RETURNED",
        MissionReportOutcome::ExtractorLost => "EXTRACTOR LOST",
    }
}

fn render_workspace_header(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
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
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                if options.ascii { " # " } else { " ◆ " },
                Style::default().fg(theme.amber),
            ),
            Span::styled("SUBSTRATE ", Style::default().fg(theme.text_strong).bold()),
            Span::styled("// ", Style::default().fg(theme.faint)),
            tab(Workspace::Planet, "PLANET"),
            Span::raw(" "),
            tab(Workspace::Overview, "OVERVIEW"),
            Span::raw(" "),
            tab(Workspace::Research, "RESEARCH"),
            Span::raw(" "),
            tab(Workspace::System, "SYSTEM"),
        ]),
        Line::from(vec![
            Span::styled(" OBJECTIVE // ", Style::default().fg(theme.amber).bold()),
            Span::styled(
                app.planet
                    .as_ref()
                    .map_or("Awaiting planetary state", |planet| {
                        planet.objective.title.as_str()
                    }),
                Style::default().fg(theme.text_strong).bold(),
            ),
            Span::styled(" // ", Style::default().fg(theme.faint)),
            Span::styled(
                app.planet
                    .as_ref()
                    .map_or("", |planet| planet.objective.guidance.as_str()),
                Style::default().fg(theme.muted),
            ),
        ]),
    ];
    if active == Workspace::Planet {
        lines.push(planet_store_header_line(
            app.planet.as_ref(),
            area.width,
            theme,
        ));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .style(theme.panel_style())
            .block(panel("", options, theme, PanelTone::Standard)),
        area,
    );
}

fn planet_store_header_line(
    planet: Option<&PlanetSnapshot>,
    width: u16,
    theme: Theme,
) -> Line<'static> {
    let Some(planet) = planet else {
        return Line::styled(" STORES // awaiting planetary state", theme.muted);
    };
    let raw = planet.raw_inventory;
    let refined = planet.refined_inventory;
    let surveyed = planet
        .regions
        .iter()
        .filter(|region| region.knowledge >= mind_core::protocol::RegionKnowledge::Surveyed)
        .count();
    let fabrication = planet.active_construction.as_ref().map_or_else(
        || {
            planet.relay_region_id.as_ref().map_or_else(
                || "idle".to_owned(),
                |region_id| format!("relay active at {}", region_designation(planet, region_id)),
            )
        },
        |construction| {
            format!(
                "relay {}% at {}",
                operation_progress(construction.remaining_ms, construction.phase_total_ms),
                region_designation(planet, &construction.region_id)
            )
        },
    );
    if width < 86 {
        return Line::from(vec![
            Span::styled(" STORES // ", Style::default().fg(theme.amber).bold()),
            Span::styled(
                format!(
                    "MAP {surveyed}/{}  RAW {}/{}/{}  REF {}/{}/{}  FAB {fabrication}",
                    planet.regions.len(),
                    raw.structural,
                    raw.conductive,
                    raw.silicate,
                    refined.structural,
                    refined.conductive,
                    refined.silicate
                ),
                Style::default().fg(theme.text_strong),
            ),
        ]);
    }
    Line::from(vec![
        Span::styled(" MAP // ", Style::default().fg(theme.amber).bold()),
        Span::styled(
            format!("{surveyed} / {}", planet.regions.len()),
            Style::default().fg(theme.text_strong),
        ),
        Span::styled(" GATHERED // ", Style::default().fg(theme.amber).bold()),
        Span::styled(
            format!(
                "S {}  C {}  Si {}",
                raw.structural, raw.conductive, raw.silicate
            ),
            Style::default().fg(theme.text_strong),
        ),
        Span::styled("   REFINED // ", Style::default().fg(theme.amber).bold()),
        Span::styled(
            format!(
                "S {}  C {}  Si {}",
                refined.structural, refined.conductive, refined.silicate
            ),
            Style::default().fg(theme.text_strong),
        ),
        Span::styled(
            "   FABRICATION // ",
            Style::default().fg(theme.amber).bold(),
        ),
        Span::styled(fabrication, Style::default().fg(theme.text_strong)),
        Span::styled("   PROFILE // ", Style::default().fg(theme.amber).bold()),
        Span::styled(
            planet
                .research
                .profiles
                .iter()
                .find(|profile| profile.active)
                .map_or("Balanced", |profile| profile.title.as_str())
                .to_owned(),
            Style::default().fg(theme.text_strong),
        ),
        Span::styled("   ANALYSIS // ", Style::default().fg(theme.amber).bold()),
        Span::styled(
            planet.research.analysis_units.to_string(),
            Style::default().fg(theme.text_strong),
        ),
    ])
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

fn render_region_inspector(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    planet: &PlanetSnapshot,
    options: RenderOptions,
    theme: Theme,
) {
    let text = app.selected_region().map_or_else(
        || Text::from(Line::styled("No region selected.", theme.muted)),
        |region| {
            if let Some(action) = &app.pending_action {
                let (title, warning) = match action {
                    ContextAction::Extract(_) => {
                        let risk = region
                            .deposit
                            .as_ref()
                            .map_or("UNKNOWN", |deposit| risk_text(deposit.risk));
                        (
                            "CONFIRM EXTRACTION",
                            format!(
                                "{risk} loss risk. Probe and carried material may be permanently lost."
                            ),
                        )
                    }
                    ContextAction::ConstructRelay(_) => (
                        "CONFIRM CONSTRUCTION",
                        "Consume 8 structural, 6 conductive, and 6 silicate units here.".into(),
                    ),
                    _ => ("CONFIRM ACTION", "Confirm the selected operation.".into()),
                };
                return Text::from(vec![
                    Line::styled(title, Style::default().fg(theme.warning).bold()),
                    Line::from(""),
                    designation_line(&region.designation, region.core, theme),
                    Line::from(""),
                    Line::styled(warning, Style::default().fg(theme.text_strong)),
                    Line::from(""),
                    key_line("ENTER", "confirm", theme),
                    key_line("ESC", "cancel", theme),
                ]);
            }

            let mut lines = vec![designation_line(&region.designation, region.core, theme)];
            if area.height < 14 && area.width < 60 {
                lines.push(Line::from(""));
                lines.extend(region_data_lines(region, theme));
                lines.push(Line::from(""));
                lines.push(section_line("ACTIVE IN REGION", theme));
                lines.extend(local_operation_lines(planet, region, theme));
                lines.push(Line::from(""));
                lines.push(section_line("REGION OPERATIONS", theme));
                lines.extend(region_action_lines(app, planet, region, theme));
                lines.push(Line::from(""));
                lines.push(section_line("LOCAL PRESENCE", theme));
                lines.extend(region_presence_lines(planet, region, theme));
            } else {
                lines.push(Line::from(""));
                lines.push(section_line("ACTIVE IN REGION", theme));
                lines.extend(local_operation_lines(planet, region, theme));
                if area.height >= 14 {
                    lines.push(Line::from(""));
                    lines.push(section_line("LOCAL PRESENCE", theme));
                    lines.extend(region_presence_lines(planet, region, theme));
                }
                lines.push(Line::from(""));
                lines.push(section_line("REGION OPERATIONS", theme));
                lines.extend(region_action_lines(app, planet, region, theme));
                lines.push(Line::from(""));
                lines.extend(region_data_lines(region, theme));
                if area.height < 14 {
                    lines.push(Line::from(""));
                    lines.push(section_line("LOCAL PRESENCE", theme));
                    lines.extend(region_presence_lines(planet, region, theme));
                }
            }
            Text::from(lines)
        },
    );
    frame.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: true })
            .scroll((app.region_panel_scroll, 0))
            .style(if app.pending_action.is_some() {
                theme.raised_style()
            } else {
                theme.panel_style()
            })
            .block(panel(
                if app.pending_action.is_some() {
                    " REGION CONFIRMATION "
                } else {
                    " REGION INSPECTOR "
                },
                options,
                theme,
                if app.pending_action.is_some() {
                    PanelTone::Modal
                } else {
                    PanelTone::Standard
                },
            )),
        area,
    );
}

fn region_data_lines(
    region: &mind_core::protocol::RegionSnapshot,
    theme: Theme,
) -> Vec<Line<'static>> {
    let mut lines = vec![
        section_line("REGION DATA", theme),
        field_line("KNOWLEDGE", format!("{:?}", region.knowledge), theme),
        field_line(
            "GRID",
            format!(
                "band {:02} / sector {:02}",
                region.position.latitude_band + 1,
                region.position.longitude_sector + 1
            ),
            theme,
        ),
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
        field_line(
            "NETWORK",
            if region.within_scout_range || region.core {
                "reachable".into()
            } else {
                "beyond current reach".into()
            },
            theme,
        ),
    ];
    if let Some(deposit) = &region.deposit {
        lines.push(field_line(
            "DEPOSIT",
            format!("{:?} // {} units", deposit.resource, deposit.remaining),
            theme,
        ));
        lines.push(field_line(
            "LOSS RISK",
            risk_text(deposit.risk).to_owned(),
            theme,
        ));
    } else if region.knowledge >= mind_core::protocol::RegionKnowledge::Surveyed {
        lines.push(field_line("DEPOSIT", "none detected".into(), theme));
    }
    lines
}

fn local_operation_lines(
    planet: &PlanetSnapshot,
    region: &mind_core::protocol::RegionSnapshot,
    theme: Theme,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut has_active_operation = false;

    if let Some(mission) = planet
        .active_mission
        .as_ref()
        .filter(|mission| mission.target_region_id == region.id)
    {
        has_active_operation = true;
        lines.push(operation_title(
            &planet.scout.designation,
            mission.mission_id,
            mission.phase,
            theme,
        ));
        lines.push(field_line(
            "PROGRESS",
            format!(
                "{}% // {}",
                operation_progress(mission.remaining_ms, mission.phase_total_ms),
                scout_remaining_text(mission)
            ),
            theme,
        ));
        lines.push(field_line(
            "CONTROL",
            if mission.automated {
                "queued automation".into()
            } else {
                "direct".into()
            },
            theme,
        ));
    }

    if let Some(mission) = planet
        .active_extraction
        .as_ref()
        .filter(|mission| mission.target_region_id == region.id)
    {
        has_active_operation = true;
        lines.push(operation_title(
            &planet.extractor.designation,
            mission.mission_id,
            mission.phase,
            theme,
        ));
        lines.push(field_line(
            "PROGRESS",
            format!(
                "{}% // {}s remaining",
                operation_progress(mission.remaining_ms, mission.phase_total_ms),
                mission.remaining_ms.div_ceil(1_000)
            ),
            theme,
        ));
        lines.push(field_line(
            "EXPOSURE",
            risk_text(mission.risk).to_owned(),
            theme,
        ));
        lines.push(field_line(
            "CAPACITY",
            format!("{} units", mission.cargo_capacity),
            theme,
        ));
        lines.push(field_line(
            "CONTROL",
            if mission.automated {
                "proven route automation".into()
            } else {
                "direct".into()
            },
            theme,
        ));
    }

    if let Some(construction) = planet
        .active_construction
        .as_ref()
        .filter(|construction| construction.region_id == region.id)
    {
        has_active_operation = true;
        lines.push(Line::styled(
            "RANGE RELAY // CONSTRUCTING",
            Style::default().fg(theme.cyan_bright).bold(),
        ));
        lines.push(field_line(
            "PROGRESS",
            format!(
                "{}% // {}s remaining",
                operation_progress(construction.remaining_ms, construction.phase_total_ms),
                construction.remaining_ms.div_ceil(1_000)
            ),
            theme,
        ));
    }

    if region.core && planet.refining_remaining_ms > 0 {
        has_active_operation = true;
        lines.push(Line::styled(
            "CORE REFINING // ACTIVE",
            Style::default().fg(theme.cyan_bright).bold(),
        ));
        lines.push(field_line(
            "NEXT UNIT",
            format!("{}s", planet.refining_remaining_ms.div_ceil(1_000)),
            theme,
        ));
    }
    if region.core && planet.extractor.rebuild_remaining_ms > 0 {
        has_active_operation = true;
        lines.push(Line::styled(
            "EXTRACTOR REBUILD // ACTIVE",
            Style::default().fg(theme.warning).bold(),
        ));
        lines.push(field_line(
            "REMAINING",
            format!("{}s", planet.extractor.rebuild_remaining_ms.div_ceil(1_000)),
            theme,
        ));
    }

    if !has_active_operation {
        lines.push(Line::styled(
            "No active operation in this region.",
            Style::default().fg(theme.muted),
        ));
    }
    if let Some(report) = planet
        .recent_reports
        .iter()
        .rev()
        .find(|report| report.region_id == region.id)
    {
        lines.push(field_line(
            "LAST REPORT",
            format!(
                "#{:03} // {}",
                report.mission_id,
                report_outcome_label(report.outcome)
            ),
            theme,
        ));
    }
    lines
}

fn operation_title<T: std::fmt::Debug>(
    designation: &str,
    mission_id: u64,
    phase: T,
    theme: Theme,
) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            designation.to_owned(),
            Style::default().fg(theme.cyan_bright).bold(),
        ),
        Span::styled(
            format!(" #{mission_id:03} // "),
            Style::default().fg(theme.faint),
        ),
        Span::styled(
            format!("{phase:?}").to_uppercase(),
            Style::default().fg(theme.amber).bold(),
        ),
    ])
}

fn scout_remaining_text(mission: &mind_core::protocol::ActiveMissionSnapshot) -> String {
    if mission.phase == ScoutState::Holding {
        "holding until recalled".into()
    } else {
        format!("{}s remaining", mission.remaining_ms.div_ceil(1_000))
    }
}

fn region_presence_lines(
    planet: &PlanetSnapshot,
    region: &mind_core::protocol::RegionSnapshot,
    theme: Theme,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if region.core {
        lines.push(field_line("STRUCTURE", "starting core".into(), theme));
    }
    if region.relay {
        lines.push(field_line("STRUCTURE", "active range relay".into(), theme));
    }
    if let Some(position) = planet
        .survey_queue
        .queued_region_ids
        .iter()
        .position(|queued| queued == &region.id)
    {
        lines.push(field_line(
            "SURVEY QUEUE",
            format!(
                "position {} of {}",
                position + 1,
                planet.survey_queue.queued_region_ids.len()
            ),
            theme,
        ));
    }
    if planet.extraction_route.region_id.as_deref() == Some(region.id.as_str()) {
        lines.push(field_line(
            "AUTO ROUTE",
            planet.extraction_route.paused_reason.as_ref().map_or_else(
                || "active".into(),
                |reason| format!("paused // {}", reason.replace('_', " ")),
            ),
            theme,
        ));
    } else if planet
        .extraction_route
        .proven_region_ids
        .iter()
        .any(|proven| proven == &region.id)
    {
        lines.push(field_line("AUTO ROUTE", "proven // inactive".into(), theme));
    }
    if let Some(construction) = planet
        .active_construction
        .as_ref()
        .filter(|construction| construction.region_id == region.id)
    {
        lines.push(field_line(
            "CONSTRUCTION",
            format!(
                "range relay // {}% // {}s",
                operation_progress(construction.remaining_ms, construction.phase_total_ms),
                construction.remaining_ms.div_ceil(1_000)
            ),
            theme,
        ));
    }

    let scout_presence = if let Some(mission) = planet
        .active_mission
        .as_ref()
        .filter(|mission| mission.target_region_id == region.id)
    {
        let relation = match mission.phase {
            ScoutState::Outbound => "inbound",
            ScoutState::Surveying => "surveying locally",
            ScoutState::Holding => "holding locally",
            ScoutState::Returning => "returning from region",
            ScoutState::Ready => "ready",
        };
        format!("{} // {relation}", planet.scout.designation)
    } else if planet.active_mission.is_none() && planet.scout.region_id == region.id {
        format!("{} // stationed", planet.scout.designation)
    } else {
        "none stationed".into()
    };
    lines.push(field_line("SCOUT", scout_presence, theme));

    let extractor_presence = if let Some(mission) = planet
        .active_extraction
        .as_ref()
        .filter(|mission| mission.target_region_id == region.id)
    {
        let relation = match mission.phase {
            mind_core::protocol::ExtractorState::Outbound => "inbound",
            mind_core::protocol::ExtractorState::Extracting => "extracting locally",
            mind_core::protocol::ExtractorState::Returning => "returning from region",
            mind_core::protocol::ExtractorState::Ready => "ready",
            mind_core::protocol::ExtractorState::Rebuilding => "rebuilding",
        };
        format!("{} // {relation}", planet.extractor.designation)
    } else if region.core && planet.active_extraction.is_none() {
        match planet.extractor.state {
            mind_core::protocol::ExtractorState::Ready => {
                format!("{} // stationed", planet.extractor.designation)
            }
            mind_core::protocol::ExtractorState::Rebuilding => format!(
                "{} // rebuilding // {}s",
                planet.extractor.designation,
                planet.extractor.rebuild_remaining_ms.div_ceil(1_000)
            ),
            _ => "none stationed".into(),
        }
    } else {
        "none stationed".into()
    };
    lines.push(field_line("EXTRACTOR", extractor_presence, theme));
    lines
}

fn region_action_lines(
    app: &App,
    planet: &PlanetSnapshot,
    region: &mind_core::protocol::RegionSnapshot,
    theme: Theme,
) -> Vec<Line<'static>> {
    let primary = app.primary_region_action();
    let automation = app.automation_action();
    let automation_label = match automation.as_ref() {
        Some(ContextAction::QueueSurvey(_)) => "queue survey",
        Some(ContextAction::RemoveQueuedSurvey(_)) => "remove queued survey",
        Some(ContextAction::SetExtractionRoute(_))
            if planet.extraction_route.region_id.as_deref() == Some(region.id.as_str()) =>
        {
            "resume route"
        }
        Some(ContextAction::SetExtractionRoute(_)) => "activate route",
        Some(ContextAction::ClearExtractionRoute) => "clear route",
        _ => "automation",
    };
    let automation_identity = automation
        .clone()
        .unwrap_or_else(|| ContextAction::QueueSurvey(region.id.clone()));
    vec![
        region_action_line(
            "S",
            "survey",
            ContextAction::Deploy(region.id.clone()),
            app.scout_action(),
            primary.as_ref(),
            survey_block_reason(planet, region),
            theme,
        ),
        region_action_line(
            "X",
            "extract",
            ContextAction::Extract(region.id.clone()),
            app.extraction_action(),
            primary.as_ref(),
            extraction_block_reason(planet, region),
            theme,
        ),
        region_action_line(
            "B",
            "build relay",
            ContextAction::ConstructRelay(region.id.clone()),
            app.construction_action(),
            primary.as_ref(),
            construction_block_reason(planet, region),
            theme,
        ),
        region_action_line(
            "R",
            "recall scout",
            ContextAction::Recall,
            app.scout_recall_action(),
            primary.as_ref(),
            recall_block_reason(planet, region),
            theme,
        ),
        region_action_line(
            "A",
            automation_label,
            automation_identity,
            automation,
            None,
            automation_block_reason(planet, region),
            theme,
        ),
    ]
}

fn automation_block_reason(
    planet: &PlanetSnapshot,
    region: &mind_core::protocol::RegionSnapshot,
) -> String {
    if !planet.survey_queue.unlocked {
        return "complete research to earn routine automation".into();
    }
    if !region.core
        && region.knowledge < mind_core::protocol::RegionKnowledge::Surveyed
        && !region.within_scout_range
    {
        return "region is beyond current network reach".into();
    }
    if region.knowledge < mind_core::protocol::RegionKnowledge::Surveyed {
        return "survey is already active or queued".into();
    }
    if let Some(deposit) = &region.deposit {
        if deposit.remaining == 0 {
            return "deposit depleted".into();
        }
        if !planet
            .extraction_route
            .proven_region_ids
            .iter()
            .any(|proven| proven == &region.id)
        {
            return "complete one manual extraction to prove this route".into();
        }
    }
    "no routine operation available here".into()
}

#[allow(clippy::too_many_arguments)]
fn region_action_line(
    key: &'static str,
    label: &'static str,
    action: ContextAction,
    available: Option<ContextAction>,
    primary: Option<&ContextAction>,
    block_reason: String,
    theme: Theme,
) -> Line<'static> {
    let is_available = available.as_ref() == Some(&action);
    let is_primary = is_available && primary == Some(&action);
    Line::from(vec![
        Span::styled(format!("[{key}] "), Style::default().fg(theme.amber).bold()),
        Span::styled(
            label,
            Style::default()
                .fg(if is_available {
                    theme.text_strong
                } else {
                    theme.muted
                })
                .add_modifier(if is_available {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ),
        Span::styled(" // ", Style::default().fg(theme.faint)),
        Span::styled(
            if is_primary {
                "ready // ENTER".to_owned()
            } else if is_available {
                "ready".to_owned()
            } else {
                block_reason
            },
            Style::default().fg(if is_available {
                theme.success
            } else {
                theme.muted
            }),
        ),
    ])
}

fn risk_text(risk: mind_core::protocol::RiskLevel) -> &'static str {
    match risk {
        mind_core::protocol::RiskLevel::Low => "LOW // 5%",
        mind_core::protocol::RiskLevel::Moderate => "MODERATE // 15%",
        mind_core::protocol::RiskLevel::High => "HIGH // 30%",
    }
}

fn survey_block_reason(
    planet: &PlanetSnapshot,
    region: &mind_core::protocol::RegionSnapshot,
) -> String {
    if region.core {
        return "core region needs no deployment".into();
    }
    if !region.within_scout_range {
        return "beyond current network reach".into();
    }
    if planet.scout.state != ScoutState::Ready {
        return planet.active_mission.as_ref().map_or_else(
            || format!("scout is {:?}", planet.scout.state).to_lowercase(),
            |mission| {
                format!(
                    "scout assigned to {}",
                    region_designation(planet, &mission.target_region_id)
                )
            },
        );
    }
    "ready".into()
}

fn extraction_block_reason(
    planet: &PlanetSnapshot,
    region: &mind_core::protocol::RegionSnapshot,
) -> String {
    let Some(deposit) = &region.deposit else {
        return if region.knowledge >= mind_core::protocol::RegionKnowledge::Surveyed {
            "no deposit detected".into()
        } else {
            "survey required".into()
        };
    };
    if !region.within_scout_range {
        return "beyond current network reach".into();
    }
    if deposit.remaining == 0 {
        return "deposit depleted".into();
    }
    match planet.extractor.state {
        mind_core::protocol::ExtractorState::Ready => "ready".into(),
        mind_core::protocol::ExtractorState::Rebuilding => format!(
            "rebuilding // {}s remaining",
            planet.extractor.rebuild_remaining_ms.div_ceil(1_000)
        ),
        _ => planet.active_extraction.as_ref().map_or_else(
            || format!("extractor is {:?}", planet.extractor.state).to_lowercase(),
            |mission| {
                format!(
                    "extractor assigned to {}",
                    region_designation(planet, &mission.target_region_id)
                )
            },
        ),
    }
}

fn construction_block_reason(
    planet: &PlanetSnapshot,
    region: &mind_core::protocol::RegionSnapshot,
) -> String {
    if region.core {
        return "core already provides network presence".into();
    }
    if region.knowledge < mind_core::protocol::RegionKnowledge::Surveyed {
        return "survey required".into();
    }
    if !region.within_scout_range {
        return "beyond current network reach".into();
    }
    if let Some(relay_region_id) = &planet.relay_region_id {
        return format!(
            "relay already active at {}",
            region_designation(planet, relay_region_id)
        );
    }
    if let Some(construction) = &planet.active_construction {
        return format!(
            "construction active at {}",
            region_designation(planet, &construction.region_id)
        );
    }
    let stock = planet.refined_inventory;
    if stock.structural < 8 || stock.conductive < 6 || stock.silicate < 6 {
        return format!(
            "needs S 8 / C 6 / Si 6 // have {} / {} / {}",
            stock.structural, stock.conductive, stock.silicate
        );
    }
    "ready".into()
}

fn recall_block_reason(
    planet: &PlanetSnapshot,
    region: &mind_core::protocol::RegionSnapshot,
) -> String {
    let Some(mission) = &planet.active_mission else {
        return "no active scout operation".into();
    };
    if mission.target_region_id != region.id {
        return format!(
            "scout assigned to {}",
            region_designation(planet, &mission.target_region_id)
        );
    }
    if mission.phase == ScoutState::Returning {
        return "already returning".into();
    }
    "ready".into()
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
    let content = if area.width < 140 {
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
            Constraint::Length(4),
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
            Constraint::Length(4),
            Constraint::Length(7),
            Constraint::Length(7),
            Constraint::Min(6),
            Constraint::Length(3),
        ])
        .split(area);
        (rows[0], rows[1], rows[2], rows[3], rows[4])
    };
    render_workspace_header(frame, header_area, app, Workspace::System, options, theme);

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
                EngineEventKind::SaveMigrated => "MIGRATE",
                EngineEventKind::ScoutDeployed => "SCOUT",
                EngineEventKind::RegionDetected => "SCAN",
                EngineEventKind::SurveyCompleted => "SURVEY",
                EngineEventKind::ScoutRecallRequested => "RECALL",
                EngineEventKind::ScoutReturned => "RETURN",
                EngineEventKind::ExtractorDeployed => "EXTRACT",
                EngineEventKind::ExtractionCompleted => "CARGO",
                EngineEventKind::ExtractorLost => "LOSS",
                EngineEventKind::ExtractorRebuilt => "REBUILD",
                EngineEventKind::RelayConstructionStarted => "BUILD",
                EngineEventKind::RelayActivated => "RELAY",
                EngineEventKind::AlphaMilestoneCompleted => "ALPHA",
                EngineEventKind::AlphaSliceCompleted => "STABLE",
                EngineEventKind::ResearchStarted => "R&D",
                EngineEventKind::ResearchCompleted => "R&D+",
                EngineEventKind::ProfileActivated => "PROFILE",
                EngineEventKind::SurveyQueued => "QUEUE",
                EngineEventKind::ExtractionRouteSet => "ROUTE",
                EngineEventKind::AutomationPaused => "PAUSE",
                EngineEventKind::ActivityReviewed => "REVIEW",
                EngineEventKind::PersistenceDegraded => "SAVE!",
                EngineEventKind::PersistenceRecovered => "SAVE",
                EngineEventKind::ShutdownRequested => "STOP",
            };
            let tag_color = match event.kind {
                EngineEventKind::PersistenceDegraded | EngineEventKind::ExtractorLost => {
                    theme.danger
                }
                EngineEventKind::ShutdownRequested | EngineEventKind::SaveReset => theme.warning,
                EngineEventKind::DiagnosticPulse
                | EngineEventKind::OnboardingCompleted
                | EngineEventKind::SurveyCompleted
                | EngineEventKind::ExtractionCompleted
                | EngineEventKind::ExtractorRebuilt
                | EngineEventKind::RelayActivated
                | EngineEventKind::AlphaMilestoneCompleted
                | EngineEventKind::AlphaSliceCompleted
                | EngineEventKind::ResearchCompleted
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
        30.min(area.height.saturating_sub(2)),
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
        key_line("ARROWS", "navigate one region", theme),
        key_line("SHIFT + ARROWS", "jump across the 10 x 20 grid", theme),
        key_line("ENTER", "use the selected region's primary action", theme),
        key_line("S", "deploy survey scout", theme),
        key_line("X", "prepare extraction from a known deposit", theme),
        key_line("B", "prepare relay construction when funded", theme),
        key_line("R", "recall the scout assigned to this region", theme),
        key_line("A", "queue or clear earned local automation", theme),
        key_line("PGUP / PGDN", "scroll the region inspector", theme),
        Line::from(""),
        section_line("OVERVIEW", theme),
        key_line("ARROWS", "select an operation or report", theme),
        key_line("ENTER", "inspect its region on the globe", theme),
        key_line("PGUP / PGDN", "scroll planet-wide details", theme),
        Line::from(""),
        section_line("RESEARCH", theme),
        key_line("UP / DOWN", "select research or a profile", theme),
        key_line("ENTER", "commit research or activate a profile", theme),
        Line::styled(
            "Extraction can permanently lose its probe and cargo.",
            Style::default().fg(theme.warning),
        ),
        Line::styled(
            "The core always rebuilds a replacement; progress cannot softlock.",
            Style::default().fg(theme.muted),
        ),
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
            protocol_version: mind_core::PROTOCOL_VERSION,
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
    fn overview_and_risk_confirmation_explain_progress_and_loss() {
        let mut state = mind_core::game::GameState::with_seed(44);
        state.complete_onboarding([2, 2, 2]);
        let target = state
            .planet_snapshot()
            .unwrap()
            .regions
            .into_iter()
            .find(|region| region.hop_distance_from_core == 1)
            .unwrap();
        state.deploy_scout(&target.id);
        state.advance(45_000);

        let mut app = App::default();
        app.apply_snapshot(status(true), Instant::now());
        app.apply_planet(state.planet_snapshot().unwrap(), Instant::now());
        app.connected();
        app.show_return_summary = false;
        app.workspace = Workspace::Overview;
        let overview = screen(
            &app,
            100,
            30,
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        assert!(overview.contains("PLANETARY ACTIVITY"));
        assert!(overview.contains("PLANET STATUS"));
        assert!(overview.contains("MATERIALS"));
        assert!(overview.contains("OBJECTIVE"));

        app.workspace = Workspace::Planet;
        app.selected_region_id = Some(target.id.clone());
        app.request_confirmation(ContextAction::Extract(target.id));
        let confirmation = screen(
            &app,
            100,
            30,
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        assert!(confirmation.contains("CONFIRM EXTRACTION"));
        assert!(confirmation.contains("permanently"));
        assert!(confirmation.contains("lost"));
        assert!(confirmation.contains("confirm"));
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
        assert!(planet.contains("REGION INSPECTOR"));
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
        assert!(narrow.contains("REGION INSPECTOR"));
        assert!(narrow.contains("REGION DATA"));
        assert!(narrow.contains("detach"));

        assert!((0..=40).any(|scroll| {
            app.region_panel_scroll = scroll;
            screen(
                &app,
                50,
                28,
                RenderOptions {
                    ascii: true,
                    no_color: true,
                },
            )
            .contains("REGION OPERATIONS")
        }));
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
    fn overview_uses_player_facing_region_designations() {
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
        app.workspace = Workspace::Overview;
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
    fn planet_inspector_and_overview_show_concurrent_probe_relationships() {
        let mut state = mind_core::game::GameState::with_seed(91);
        state.complete_onboarding([2, 2, 2]);
        let candidates = state
            .planet_snapshot()
            .unwrap()
            .regions
            .into_iter()
            .filter(|region| region.within_scout_range && !region.core)
            .map(|region| (region.id, region.hop_distance_from_core))
            .collect::<Vec<_>>();
        let mut target = None;
        for (region_id, hops) in candidates {
            assert!(state.deploy_scout(&region_id).ok);
            state.advance(u64::from(hops) * 15_000 + 30_000);
            let surveyed = state
                .planet_snapshot()
                .unwrap()
                .regions
                .into_iter()
                .find(|region| region.id == region_id)
                .unwrap();
            assert!(state.recall_scout().ok);
            state.advance(u64::from(hops) * 15_000);
            if surveyed.deposit.is_some() {
                target = Some(surveyed);
                break;
            }
        }
        let target = target.expect("generated starting range should contain a deposit");
        let expected_risk = risk_text(target.deposit.as_ref().unwrap().risk);
        assert!(state.deploy_scout(&target.id).ok);
        assert!(state.deploy_extractor(&target.id).ok);

        let mut app = App::default();
        app.apply_snapshot(status(true), Instant::now());
        app.apply_planet(state.planet_snapshot().unwrap(), Instant::now());
        app.connected();
        app.show_return_summary = false;
        app.selected_region_id = Some(target.id.clone());
        let planet = screen(
            &app,
            120,
            40,
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        assert!(planet.contains("GATHERED"));
        assert!(planet.contains("REFINED"));
        assert!(planet.contains("FABRICATION"));
        assert!(planet.contains("ACTIVE IN REGION"));
        assert!(planet.contains("Scout 01 #"));
        assert!(planet.contains("Extractor 01 #"));
        assert!(planet.contains("PROGRESS"));
        assert!(planet.contains(expected_risk));
        assert!(planet.contains("CAPACITY"));
        assert!(planet.contains("4 units"));
        assert!(planet.contains("LOCAL PRESENCE"));
        assert!(planet.matches("inbound").count() >= 2);
        assert!(planet.contains("recall scout"));

        app.workspace = Workspace::Overview;
        let overview = screen(
            &app,
            120,
            40,
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        assert!(overview.contains(&state.planet_snapshot().unwrap().scout.designation));
        assert!(overview.contains(&state.planet_snapshot().unwrap().extractor.designation));
        assert!(overview.matches("OUTBOUND").count() >= 2);
        assert!(overview.contains("MATERIAL STORES"));
        assert!(overview.contains("raw 0 // refined 0"));
        assert!(overview.contains("FABRICATION"));
        assert!(overview.contains("capacity 4 units"));
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

    #[test]
    fn return_summary_reorients_with_visible_activity_stores_and_next_steps() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([2, 2, 2]);
        let mut planet = engine.planet().unwrap();
        planet
            .unreviewed_activity
            .push(mind_core::protocol::ActivityRecord {
                activity_id: 1,
                kind: mind_core::protocol::ActivityKind::Survey,
                region_id: Some("region_082".into()),
                summary: "Survey completed at R-082; one analysis unit recorded.".into(),
            });
        let mut app = App::default();
        app.apply_snapshot(engine.status(), Instant::now());
        app.apply_planet(planet, Instant::now());
        app.connected();

        let output = screen(
            &app,
            100,
            36,
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        assert!(output.contains("RETURN SUMMARY"));
        assert!(output.contains("Survey completed at R-082"));
        assert!(output.contains("STORES NOW"));
        assert!(output.contains("CURRENT"));
        assert!(output.contains("NEXT"));
        assert!(output.contains("acknowledge and continue"));
    }

    #[test]
    fn research_workspace_explains_permanent_choices_and_active_profile() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([2, 2, 2]);
        let mut planet = engine.planet().unwrap();
        planet.research.unlocked = true;
        planet.research.analysis_units = 2;
        planet.research.options[0].completed = true;
        planet.research.profiles[1].unlocked = true;
        planet.research.profiles[1].active = true;
        planet.research.profiles[0].active = false;
        let mut app = App::default();
        app.apply_snapshot(engine.status(), Instant::now());
        app.apply_planet(planet, Instant::now());
        app.connected();
        app.workspace = Workspace::Research;

        let output = screen(
            &app,
            100,
            32,
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        assert!(output.contains("RESEARCH & SPECIALIZATION"));
        assert!(output.contains("Extended Telemetry"));
        assert!(output.contains("COMPLETED"));
        assert!(output.contains("Reach"));
        assert!(output.contains("ACTIVE"));
        assert!(output.contains("+1 operational hop"));
    }
}
