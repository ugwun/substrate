use crate::{
    app::{
        App, ConnectionState, ContextAction, OnboardingView, SessionMenuAction, SessionMenuState,
        Workspace,
    },
    art, boot, cinematic,
    globe::{self, GlobeRenderOptions},
    probes,
    theme::Theme,
    transmission, vista,
};
use mind_core::protocol::{
    BaseModuleKind, CampaignLeadKind, EngineEventKind, ExtractorLoadout, ExtractorState,
    MissionReportOutcome, PlanetSnapshot, ScoutState, SubregionKnowledge, SubregionSnapshot,
};
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

const ASCII_DOUBLE_BORDER: border::Set<'static> = border::Set {
    top_left: "+",
    top_right: "+",
    bottom_left: "+",
    bottom_right: "+",
    vertical_left: "|",
    vertical_right: "|",
    horizontal_top: "=",
    horizontal_bottom: "=",
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

fn archive_source_label(
    planet: &PlanetSnapshot,
    record: &mind_core::protocol::ArchiveRecordSnapshot,
) -> String {
    if let Some(region_id) = &record.region_id {
        return region_designation(planet, region_id).to_owned();
    }
    match record.record_id.as_str() {
        "observation_004" => "MULTI-SITE".to_owned(),
        "observation_005" => "ORBITAL FRAME".to_owned(),
        _ => "LOCAL ARRAY".to_owned(),
    }
}

fn dim_color_toward_canvas(
    color: ratatui::style::Color,
    canvas: ratatui::style::Color,
    amount: f32,
) -> ratatui::style::Color {
    use ratatui::style::Color;
    match (color, canvas) {
        (Color::Rgb(r, g, b), Color::Rgb(cr, cg, cb)) => {
            let mix = |a: u8, b: u8| -> u8 {
                (f32::from(a) * (1.0 - amount) + f32::from(b) * amount)
                    .round()
                    .clamp(0.0, 255.0) as u8
            };
            Color::Rgb(mix(r, cr), mix(g, cg), mix(b, cb))
        }
        (Color::Indexed(_), Color::Indexed(_)) => Color::Indexed(236),
        (_, Color::Black) => Color::DarkGray,
        _ => canvas,
    }
}

/// Dim every cell in `area` behind a centered modal smoothly (no stripes, no competing text fragments).
fn dim_backdrop(frame: &mut Frame<'_>, area: Rect, theme: Theme, ascii: bool) {
    if theme.no_color() {
        return;
    }
    let buf = frame.buffer_mut();
    let footer_start = area.y.saturating_add(area.height.saturating_sub(2));
    let x_end = area.x.saturating_add(area.width);
    for y in area.y..area.y.saturating_add(area.height) {
        let in_footer = y >= footer_start;
        // Snapshot the row so neighbour checks see original glyphs, not already-blanked cells.
        let row: Vec<String> = (area.x..x_end)
            .map(|x| buf[(x, y)].symbol().to_owned())
            .collect();
        let is_rule = |sym: &str| matches!(sym, "-" | "=" | "+");
        for x in area.x..x_end {
            let i = usize::from(x - area.x);
            let sym = row[i].as_str();
            let is_pixel_block = matches!(sym, "▀" | "▄" | "█" | " ");
            // Box-drawing glyphs are always borders. In the ASCII tier, `|` and `+` are borders, and
            // `-`/`=` only when they continue a horizontal rule; a lone `-` (as in "R-081") is text
            // and must be blanked like any other fragment.
            let left_rule = i > 0 && is_rule(&row[i - 1]);
            let right_rule = row.get(i + 1).is_some_and(|next| is_rule(next));
            let is_border = matches!(
                sym,
                "│" | "─" | "╭" | "╮" | "╰" | "╯" | "║" | "═" | "╔" | "╗" | "╚" | "╝"
            ) || (ascii
                && (matches!(sym, "|" | "+")
                    || (matches!(sym, "-" | "=") && (left_rule || right_rule))));
            let cell = &mut buf[(x, y)];
            if is_pixel_block {
                let fg = dim_color_toward_canvas(cell.fg, theme.canvas, 0.70);
                let bg = dim_color_toward_canvas(cell.bg, theme.canvas, 0.70);
                cell.set_style(Style::default().fg(fg).bg(bg));
            } else if is_border || in_footer {
                let fg = dim_color_toward_canvas(cell.fg, theme.canvas, 0.55);
                cell.set_style(Style::default().fg(fg).bg(theme.canvas));
            } else {
                // Blank competing background text fragments cleanly behind modals.
                cell.set_symbol(" ");
                cell.set_style(Style::default().fg(theme.canvas).bg(theme.canvas));
            }
        }
    }
}

/// Smallest scroll offset that keeps the selected row span fully visible (or its top, if taller).
fn selection_scroll(total: usize, height: usize, selected: Option<(usize, usize)>) -> usize {
    let max_scroll = total.saturating_sub(height);
    let Some((start, end)) = selected else {
        return 0;
    };
    let wanted = end.saturating_sub(height);
    wanted.min(start).min(max_scroll)
}

/// Wrap `text` into lines with a hanging indent of `indent` spaces on continuation rows.
fn wrap_indented_lines(
    prefix_spans: Vec<Span<'static>>,
    prefix_width: usize,
    text: &str,
    indent: usize,
    max_width: usize,
    text_style: Style,
) -> Vec<Line<'static>> {
    let width = max_width.max(indent + 12);
    let first_budget = width.saturating_sub(prefix_width).max(10);
    let cont_budget = width.saturating_sub(indent).max(10);
    let mut lines = Vec::new();
    let mut first_line = true;
    let mut current = String::new();
    let mut budget = first_budget;

    for word in text.split_whitespace() {
        let wlen = word.chars().count();
        let clen = current.chars().count();
        if clen == 0 {
            current.push_str(word);
        } else if clen + 1 + wlen <= budget {
            current.push(' ');
            current.push_str(word);
        } else {
            if first_line {
                let mut spans = prefix_spans.clone();
                spans.push(Span::styled(std::mem::take(&mut current), text_style));
                lines.push(Line::from(spans));
                first_line = false;
                budget = cont_budget;
            } else {
                lines.push(Line::from(vec![
                    Span::raw(" ".repeat(indent)),
                    Span::styled(std::mem::take(&mut current), text_style),
                ]));
            }
            current.push_str(word);
        }
    }
    if first_line {
        let mut spans = prefix_spans;
        spans.push(Span::styled(current, text_style));
        lines.push(Line::from(spans));
    } else if !current.is_empty() {
        lines.push(Line::from(vec![
            Span::raw(" ".repeat(indent)),
            Span::styled(current, text_style),
        ]));
    }
    lines
}

fn wrapped_field_lines(
    label: &str,
    value: &str,
    max_width: usize,
    theme: Theme,
) -> Vec<Line<'static>> {
    let prefix = vec![Span::styled(
        format!("{label:<12}"),
        Style::default().fg(theme.muted),
    )];
    wrap_indented_lines(
        prefix,
        12,
        value,
        12,
        max_width,
        Style::default().fg(theme.text),
    )
}

pub fn render(frame: &mut Frame<'_>, app: &App, options: RenderOptions, now: Instant) {
    let area = frame.area();
    let theme = Theme::for_color_tier(options.no_color, app.ansi16, app.ansi256);
    frame.render_widget(Block::default().style(theme.canvas_style()), area);
    if area.width < 44 || area.height < 14 {
        if area.width >= 24 && area.height >= 3 && app.planet.is_some() && !app.onboarding_pending()
        {
            render_pilot_strip(frame, area, app, options, theme, now);
        } else {
            render_too_small(frame, area, options, theme);
        }
        if app.session_menu.is_some() {
            render_session_menu(frame, area, app, options, theme);
        }
        return;
    }
    if app.session_menu.is_some() && (app.snapshot.is_none() || app.onboarding_pending()) {
        render_session_menu_backdrop(frame, area, app, options, theme);
        render_session_menu(frame, area, app, options, theme);
        return;
    }
    if app.session_menu.is_none() {
        if let Some(boot_state) = app
            .boot_intro
            .filter(|boot_state| boot_state.is_active(now))
        {
            boot::render_boot_intro(
                frame,
                area,
                boot_state,
                options,
                theme,
                app.color_tier(options),
                now,
            );
            return;
        }
    }
    if app.snapshot.is_none() {
        render_connection_required(frame, area, app, options, theme);
    } else if app.onboarding_pending() {
        render_onboarding(frame, area, app, options, theme);
    } else {
        match app.workspace {
            Workspace::Planet => render_planet(frame, area, app, options, theme, now),
            Workspace::Overview => render_overview(frame, area, app, options, theme, now),
            Workspace::Probes => render_probes(frame, area, app, options, theme, now),
            Workspace::Research => render_research(frame, area, app, options, theme, now),
            Workspace::Archive => render_archive(frame, area, app, options, theme, now),
            Workspace::SystemMap => render_system_map(frame, area, app, options, theme, now),
            Workspace::System => render_dashboard(frame, area, app, options, theme, now),
        }
    }
    if app.session_menu.is_some() {
        dim_backdrop(frame, area, theme, options.ascii);
        render_session_menu(frame, area, app, options, theme);
        return;
    }
    if app.show_return_summary {
        dim_backdrop(frame, area, theme, options.ascii);
        render_return_summary(frame, area, app, options, theme);
    } else if let (Some(planet), Some((index, record))) =
        (&app.planet, app.active_transmission_record())
    {
        let source = archive_source_label(planet, record);
        let elapsed = app
            .transmission_opened_at
            .map_or(std::time::Duration::from_secs(10), |opened| {
                now.saturating_duration_since(opened)
            });
        transmission::render_discovery_transmission(
            frame,
            area,
            transmission::DiscoveryTransmissionView {
                record,
                record_index: index,
                total_records: planet.archive.records.len(),
                source_label: &source,
                campaign: &planet.campaign,
                elapsed,
                force_complete: app.transmission_revealed,
                is_replay: app.replay_transmission_id.is_some(),
                scroll: app.transmission_scroll,
                phase: app.globe_phase,
                options,
                theme,
                tier: app.color_tier(options),
            },
        );
    } else if app.show_chapter_card {
        if let Some(planet) = &app.planet {
            dim_backdrop(frame, area, theme, options.ascii);
            transmission::render_chapter_card(frame, area, &planet.campaign, options, theme);
        }
    }
    if app.show_help {
        dim_backdrop(frame, area, theme, options.ascii);
        render_help(
            frame,
            area,
            app.help_scroll,
            &app.scroll_limits.help,
            options,
            theme,
        );
    } else if let Some(progress) = app.transition_progress(now) {
        art::render_crt_wipe(frame, area, progress, options, theme);
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
    let header_height = if area.height >= 30 { 5 } else { 3 };
    let rows = Layout::vertical([
        Constraint::Length(header_height),
        Constraint::Min(7),
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
                    label: "menu",
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

    if area.width >= 76 {
        let (left_pct, right_pct) = if area.width >= 100 {
            (58, 42)
        } else {
            (54, 46)
        };
        let columns = Layout::horizontal([
            Constraint::Percentage(left_pct),
            Constraint::Percentage(right_pct),
        ])
        .split(rows[1]);
        if app.local_view {
            render_local_sites(frame, columns[0], app, planet, options, theme, now);
            render_local_inspector(frame, columns[1], app, planet, options, theme);
        } else {
            render_globe(frame, columns[0], app, planet, options, theme, now);
            render_region_inspector(frame, columns[1], app, planet, options, theme, now);
        }
    } else {
        let body = Layout::vertical([Constraint::Percentage(56), Constraint::Percentage(44)])
            .split(rows[1]);
        if app.local_view {
            render_local_sites(frame, body[0], app, planet, options, theme, now);
            render_local_inspector(frame, body[1], app, planet, options, theme);
        } else {
            render_globe(frame, body[0], app, planet, options, theme, now);
            render_region_inspector(frame, body[1], app, planet, options, theme, now);
        }
    }
    let local_controls = [
        KeyHint {
            key: "ARROWS",
            label: "sector",
        },
        KeyHint {
            key: "ENTER",
            label: "act",
        },
        KeyHint {
            key: "N",
            label: "lead",
        },
        KeyHint {
            key: "PGUP/DN",
            label: "inspect",
        },
        KeyHint {
            key: "P/C/D/W/L/O",
            label: "mod",
        },
        KeyHint {
            key: "G/M",
            label: "rig/maint",
        },
        KeyHint {
            key: "ESC",
            label: "globe",
        },
        KeyHint {
            key: "Q",
            label: "menu",
        },
        KeyHint {
            key: "?",
            label: "help",
        },
    ];
    let controls: &[KeyHint<'_>] = if app.local_view {
        &local_controls
    } else if area.width < 72 {
        &[
            KeyHint {
                key: "ARROWS",
                label: "move",
            },
            KeyHint {
                key: "ENTER",
                label: "act",
            },
            KeyHint {
                key: "TAB",
                label: "view",
            },
            KeyHint {
                key: "Q",
                label: "menu",
            },
        ]
    } else if area.width < 104 {
        &[
            KeyHint {
                key: "ARROWS",
                label: "region",
            },
            KeyHint {
                key: "ENTER",
                label: "act",
            },
            KeyHint {
                key: "N",
                label: "lead",
            },
            KeyHint {
                key: "TAB",
                label: "view",
            },
            KeyHint {
                key: "Q",
                label: "menu",
            },
            KeyHint {
                key: "?",
                label: "help",
            },
        ]
    } else if area.width < 140 {
        &[
            KeyHint {
                key: "ARROWS",
                label: "region",
            },
            KeyHint {
                key: "ENTER",
                label: "action",
            },
            KeyHint {
                key: "N",
                label: "lead",
            },
            KeyHint {
                key: "C",
                label: "brief",
            },
            KeyHint {
                key: "PGUP/DN",
                label: "scroll",
            },
            KeyHint {
                key: "TAB",
                label: "view",
            },
            KeyHint {
                key: "Q",
                label: "menu",
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
                key: "V",
                label: "local field",
            },
            KeyHint {
                key: "N",
                label: "lead",
            },
            KeyHint {
                key: "C",
                label: "chapter",
            },
            KeyHint {
                key: "TAB",
                label: "workspace",
            },
            KeyHint {
                key: "Q",
                label: "menu",
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
        Constraint::Length(3),
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
                    label: "menu",
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
        let remaining_ms = app.display_remaining_ms(research.remaining_ms, now);
        let title = planet
            .research
            .options
            .iter()
            .find(|option| option.project == research.project)
            .map_or("Research", |option| option.title.as_str());
        activity.push(Line::styled(
            format!(
                "  {title} // {}% // {}s remaining",
                operation_progress(remaining_ms, research.phase_total_ms),
                remaining_ms.div_ceil(1_000)
            ),
            Style::default().fg(theme.cyan_bright).bold(),
        ));
    }
    if let Some(mission) = &planet.active_mission {
        has_active_operation = true;
        let target_designation = region_designation(planet, &mission.target_region_id);
        let remaining_ms = if mission.phase == ScoutState::Holding {
            mission.remaining_ms
        } else {
            app.display_remaining_ms(mission.remaining_ms, now)
        };
        let progress = if mission.phase == ScoutState::Holding || mission.phase_total_ms == 0 {
            100
        } else {
            100_u64.saturating_sub(remaining_ms.saturating_mul(100) / mission.phase_total_ms)
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
                if mission.phase == ScoutState::Holding {
                    "holding on station".to_owned()
                } else {
                    format!("{}s remaining", remaining_ms.div_ceil(1_000))
                },
                if mission.automated { " // AUTO" } else { "" }
            ),
            Style::default().fg(theme.muted),
        ));
        row_index += 1;
    }
    if let Some(mission) = &planet.active_extraction {
        has_active_operation = true;
        let remaining_ms = app.display_remaining_ms(mission.remaining_ms, now);
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
                operation_progress(remaining_ms, mission.phase_total_ms),
                remaining_ms.div_ceil(1_000),
                if mission.automated { " // AUTO" } else { "" }
            ),
            Style::default().fg(theme.muted),
        ));
        activity.push(Line::styled(
            format!(
                "   {} // capacity {} units // {}",
                risk_text(mission.risk),
                mission.cargo_capacity,
                mission.loadout.title()
            ),
            Style::default().fg(theme.muted),
        ));
        row_index += 1;
    }
    if let Some(construction) = &planet.active_construction {
        has_active_operation = true;
        let remaining_ms = app.display_remaining_ms(construction.remaining_ms, now);
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
                operation_progress(remaining_ms, construction.phase_total_ms),
                remaining_ms.div_ceil(1_000)
            ),
            Style::default().fg(theme.muted),
        ));
        row_index += 1;
    }
    for project in &planet.network.active_projects {
        has_active_operation = true;
        let remaining_ms = app.display_remaining_ms(project.remaining_ms, now);
        if app.overview_selection == row_index {
            selected_line = activity.len();
        }
        activity.push(Line::from(vec![
            overview_marker(app.overview_selection == row_index, options, theme),
            Span::styled(
                if project.maintenance {
                    "NODE MAINTENANCE".into()
                } else {
                    project.module.map_or("FOOTHOLD".into(), |module| {
                        module_title(module).to_uppercase()
                    })
                },
                Style::default().fg(theme.cyan_bright).bold(),
            ),
            Span::styled("  //  ", Style::default().fg(theme.faint)),
            Span::styled("CONSTRUCTING", Style::default().fg(theme.amber).bold()),
        ]));
        activity.push(Line::styled(
            format!(
                "   {} // {}% // {}s{}",
                region_designation(planet, &project.region_id),
                operation_progress(remaining_ms, project.phase_total_ms),
                remaining_ms.div_ceil(1_000),
                if project.logistics_supported {
                    " // DEPOT SUPPLY"
                } else {
                    ""
                }
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
            .wrap(Wrap { trim: false })
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
            "{:?} // {} // {}",
            mission.phase,
            region_designation(planet, &mission.target_region_id),
            loadout_code(mission.loadout)
        )
    } else if planet.extractor.rebuild_remaining_ms > 0 {
        format!(
            "Rebuilding // {}s // {}",
            planet.extractor.rebuild_remaining_ms.div_ceil(1_000),
            loadout_code(planet.extractor.loadout)
        )
    } else {
        format!(
            "{:?} // core // {}",
            planet.extractor.state,
            loadout_code(planet.extractor.loadout)
        )
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
        format!(
            "waypoint {} / {} // 8-slot automation needs research",
            planet.survey_queue.queued_region_ids.len(),
            planet.survey_queue.capacity
        )
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
    let right_inner_w = usize::from(columns[1].width.saturating_sub(2)).max(24);
    let active_depots = planet
        .network
        .bases
        .iter()
        .filter(|base| {
            base.connected
                && base
                    .modules
                    .iter()
                    .any(|module| module.kind == BaseModuleKind::Depot && module.active)
        })
        .count();
    let depot_noun = if active_depots == 1 {
        "depot"
    } else {
        "depots"
    };
    let horizon = &planet.network.horizon;
    let region_check = if horizon.surveyed_regions >= horizon.required_regions {
        if options.ascii {
            format!(
                "regions {} ({} req) OK",
                horizon.surveyed_regions, horizon.required_regions
            )
        } else {
            format!(
                "regions {} ({} req) ✓",
                horizon.surveyed_regions, horizon.required_regions
            )
        }
    } else {
        format!(
            "regions {}/{}",
            horizon.surveyed_regions, horizon.required_regions
        )
    };
    let mut summary_lines = vec![section_line("MATERIAL STORES", theme)];
    summary_lines.extend(wrapped_field_lines(
        "STRUCTURAL",
        &format!(
            "raw {} // refined {} // relay needs 8",
            raw.structural, stock.structural
        ),
        right_inner_w,
        theme,
    ));
    summary_lines.extend(wrapped_field_lines(
        "CONDUCTIVE",
        &format!(
            "raw {} // refined {} // relay needs 6",
            raw.conductive, stock.conductive
        ),
        right_inner_w,
        theme,
    ));
    summary_lines.extend(wrapped_field_lines(
        "SILICATE",
        &format!(
            "raw {} // refined {} // relay needs 6",
            raw.silicate, stock.silicate
        ),
        right_inner_w,
        theme,
    ));
    summary_lines.extend(wrapped_field_lines(
        "REFINING",
        &if planet.refining_remaining_ms == 0 {
            "idle".into()
        } else {
            format!(
                "{}s to next unit",
                planet.refining_remaining_ms.div_ceil(1_000)
            )
        },
        right_inner_w,
        theme,
    ));
    summary_lines.push(Line::from(""));
    summary_lines.push(section_line("FLEET & NETWORK", theme));
    summary_lines.extend(wrapped_field_lines(
        "SCOUT",
        &format!("{:?} // {scout_location}", planet.scout.state),
        right_inner_w,
        theme,
    ));
    summary_lines.extend(wrapped_field_lines(
        "EXTRACTOR",
        &extractor_status,
        right_inner_w,
        theme,
    ));
    summary_lines.extend(wrapped_field_lines(
        "RELAY",
        &relay_status,
        right_inner_w,
        theme,
    ));
    summary_lines.extend(wrapped_field_lines(
        "LOCAL SITES",
        &if planet.network.unlocked || !planet.network.subregions.is_empty() {
            format!(
                "{} surveyed // {} required for horizon",
                horizon.surveyed_subregions, horizon.required_subregions
            )
        } else {
            "survey a region first to reveal local sectors".into()
        },
        right_inner_w,
        theme,
    ));
    summary_lines.extend(wrapped_field_lines(
        "NODES",
        &format!(
            "{} total // {} connected // {} beyond relay envelope",
            planet.network.bases.len(),
            horizon.connected_bases,
            horizon.remote_bases
        ),
        right_inner_w,
        theme,
    ));
    summary_lines.extend(wrapped_field_lines(
        "NET POWER",
        &format!(
            "{} generation / {} demand",
            planet
                .network
                .bases
                .iter()
                .map(|base| base.generation)
                .sum::<u32>(),
            planet
                .network
                .bases
                .iter()
                .map(|base| base.demand)
                .sum::<u32>()
        ),
        right_inner_w,
        theme,
    ));
    summary_lines.extend(wrapped_field_lines(
        "CREWS",
        &format!(
            "{} / {} committed",
            planet.network.active_projects.len(),
            planet.network.project_capacity
        ),
        right_inner_w,
        theme,
    ));
    summary_lines.extend(wrapped_field_lines(
        "LOGISTICS",
        &format!("{active_depots} active connected {depot_noun} // each adds one crew, max two"),
        right_inner_w,
        theme,
    ));
    summary_lines.extend(wrapped_field_lines(
        "OBSERVATION",
        &format!(
            "labs {}/{} // arrays {}/{} // {region_check}",
            horizon.analysis_sites,
            horizon.required_analysis_sites,
            horizon.observation_sites,
            horizon.required_observation_sites,
        ),
        right_inner_w,
        theme,
    ));
    summary_lines.push(Line::from(""));
    summary_lines.push(section_line("RESEARCH & AUTOMATION", theme));
    summary_lines.extend(wrapped_field_lines(
        "PROFILE",
        active_profile,
        right_inner_w,
        theme,
    ));
    summary_lines.extend(wrapped_field_lines(
        "ANALYSIS",
        &format!("{} available", planet.research.analysis_units),
        right_inner_w,
        theme,
    ));
    summary_lines.extend(wrapped_field_lines(
        "SURVEYS",
        &survey_automation,
        right_inner_w,
        theme,
    ));
    summary_lines.extend(wrapped_field_lines(
        "EXTRACTION",
        &extraction_automation,
        right_inner_w,
        theme,
    ));
    summary_lines.push(Line::from(""));
    summary_lines.push(section_line("FABRICATION", theme));
    summary_lines.push(Line::styled(
        construction_status,
        Style::default().fg(theme.text_strong),
    ));
    if planet.network.active_projects.is_empty() {
        summary_lines.push(Line::styled(
            "base projects idle",
            Style::default().fg(theme.muted),
        ));
    } else {
        for project in &planet.network.active_projects {
            summary_lines.push(Line::styled(
                format!(
                    "{} // {} // {}% // {}s{}",
                    region_designation(planet, &project.region_id),
                    if project.maintenance {
                        "maintenance"
                    } else {
                        project.module.map_or("foothold", module_title)
                    },
                    operation_progress(project.remaining_ms, project.phase_total_ms),
                    project.remaining_ms.div_ceil(1_000),
                    if project.logistics_supported {
                        " // DEPOT SUPPLY"
                    } else {
                        ""
                    }
                ),
                Style::default().fg(theme.text_strong),
            ));
        }
    }
    summary_lines.push(Line::from(""));
    summary_lines.push(Line::styled(
        "Select an operation or report and press Enter to inspect its region.",
        Style::default().fg(theme.muted),
    ));
    let summary_total = summary_lines.len();
    let summary_visible = usize::from(columns[1].height.saturating_sub(2));
    app.scroll_limits
        .overview_status
        .set(u16::try_from(summary_total.saturating_sub(summary_visible)).unwrap_or(u16::MAX));
    let status_overflow = summary_total > summary_visible;
    let status_title = if status_overflow {
        if app.overview_status_scroll > 0 {
            if options.ascii {
                " PLANET STATUS [PgUp/PgDn] "
            } else {
                " PLANET STATUS [PgUp/PgDn ▲▼] "
            }
        } else if options.ascii {
            " PLANET STATUS [PgDn more] "
        } else {
            " PLANET STATUS [PgDn ▼] "
        }
    } else {
        " PLANET STATUS "
    };
    let summary = Text::from(summary_lines);
    frame.render_widget(
        Paragraph::new(summary)
            .wrap(Wrap { trim: false })
            .scroll((app.overview_status_scroll, 0))
            .style(theme.panel_style())
            .block(panel(status_title, options, theme, PanelTone::Standard)),
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
                label: "menu",
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
                label: "menu",
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
        Constraint::Length(3),
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
    let block = panel(
        " RESEARCH & SPECIALIZATION ",
        options,
        theme,
        PanelTone::Focused,
    );
    let inner = block.inner(rows[1]);
    let show_topology = research.unlocked && inner.height >= 20 && inner.width >= 52;
    let wrap_width = usize::from(inner.width.max(20));
    // Row span of the highlighted entry (title + its wrapped detail) so the body can scroll to it.
    let mut selected_rows: Option<(usize, usize)> = None;
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
            let is_active = research
                .active
                .as_ref()
                .is_some_and(|act| act.project == option.project);
            let state = if option.completed {
                "COMPLETED"
            } else if is_active {
                "ACTIVE"
            } else if option.available {
                "READY"
            } else {
                "LOCKED"
            };
            let entry_start = lines.len();
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{marker} {:<22}", option.title),
                    Style::default().fg(theme.text_strong),
                ),
                Span::styled(
                    format!(" {state:<9}"),
                    Style::default()
                        .fg(if is_active {
                            theme.cyan_bright
                        } else if option.completed {
                            theme.success
                        } else if option.available {
                            theme.amber
                        } else {
                            theme.muted
                        })
                        .add_modifier(if is_active {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
                Span::styled(
                    format!(" {} analysis", option.analysis_cost),
                    Style::default().fg(theme.muted),
                ),
            ]));
            lines.extend(wrap_indented_lines(
                vec![Span::raw("    ")],
                4,
                &option.summary,
                4,
                wrap_width,
                Style::default().fg(theme.muted),
            ));
            if index == app.research_selection {
                selected_rows = Some((entry_start, lines.len()));
            }
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
            let entry_start = lines.len();
            lines.extend(wrap_indented_lines(
                vec![
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
                ],
                29,
                &profile.summary,
                29,
                wrap_width,
                Style::default().fg(theme.muted),
            ));
            if offset + index == app.research_selection {
                selected_rows = Some((entry_start, lines.len()));
            }
        }
        if field_operation_active {
            lines.extend(wrap_indented_lines(
                vec![Span::raw("    ")],
                4,
                "Profiles can be changed when survey and extraction probes are not in flight.",
                4,
                wrap_width,
                Style::default().fg(theme.muted),
            ));
        }
    }
    let body_height = usize::from(if show_topology {
        inner.height.saturating_sub(7)
    } else {
        inner.height
    });
    let scroll = selection_scroll(lines.len(), body_height, selected_rows);
    let more_below = scroll + body_height < lines.len();
    let block = if scroll > 0 || more_below {
        panel(
            match (more_below, options.ascii) {
                (true, true) => " RESEARCH & SPECIALIZATION [more v] ",
                (true, false) => " RESEARCH & SPECIALIZATION [more ▼] ",
                (false, true) => " RESEARCH & SPECIALIZATION [^] ",
                (false, false) => " RESEARCH & SPECIALIZATION [▲] ",
            },
            options,
            theme,
            PanelTone::Focused,
        )
    } else {
        block
    };
    frame.render_widget(block, rows[1]);
    frame.render_widget(Block::default().style(theme.panel_style()), inner);
    let body_area = if show_topology {
        let splits = Layout::vertical([
            Constraint::Length(6),
            Constraint::Length(1),
            Constraint::Min(10),
        ])
        .split(inner);
        let completed = [
            research.options.first().is_some_and(|o| o.completed),
            research.options.get(1).is_some_and(|o| o.completed),
            research.options.get(2).is_some_and(|o| o.completed),
        ];
        let active_idx = research.active.as_ref().and_then(|act| {
            research
                .options
                .iter()
                .position(|o| o.project == act.project)
        });
        let active_profile_idx = research.profiles.iter().position(|p| p.active).unwrap_or(0);
        let topo = crate::cinematic::build_research_topology_buffer(
            completed,
            active_idx,
            active_profile_idx,
            splits[0].width,
            splits[0].height,
            app.globe_phase,
        );
        crate::raster::blit_to_frame(&topo, frame, splits[0], app.color_tier(options));
        let caption = Line::from(vec![
            Span::styled(" CORE ", Style::default().fg(theme.gold).bold()),
            Span::styled(
                if options.ascii {
                    "---> "
                } else {
                    "───▶ "
                },
                Style::default().fg(theme.faint),
            ),
            Span::styled("DIRECTIONS ", Style::default().fg(theme.cyan_bright).bold()),
            Span::styled(
                if options.ascii {
                    "---> "
                } else {
                    "───▶ "
                },
                Style::default().fg(theme.faint),
            ),
            Span::styled("PROFILES", Style::default().fg(theme.amber).bold()),
        ]);
        frame.render_widget(
            Paragraph::new(caption)
                .alignment(Alignment::Center)
                .style(theme.panel_style()),
            splits[1],
        );
        splits[2]
    } else {
        inner
    };
    frame.render_widget(
        Paragraph::new(lines)
            .scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0))
            .style(theme.panel_style()),
        body_area,
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
                label: "menu",
            },
        ],
        theme,
    );
}

fn render_system_map(
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
    render_workspace_header(frame, rows[0], app, Workspace::SystemMap, options, theme);

    let available = app
        .planet
        .as_ref()
        .is_some_and(|planet| planet.system_map.available);
    let elapsed = app
        .system_map_opened_at
        .map_or(std::time::Duration::from_secs(10), |opened| {
            now.saturating_duration_since(opened)
        });
    if !available {
        frame.render_widget(
            Paragraph::new("Local-system observations are not yet resolved.")
                .alignment(Alignment::Center)
                .style(theme.panel_style())
                .block(panel(" SYSTEM MAP ", options, theme, PanelTone::Standard)),
            rows[1],
        );
    } else if area.width >= 92 {
        let columns = Layout::horizontal([Constraint::Percentage(64), Constraint::Percentage(36)])
            .split(rows[1]);
        art::render_system_cliffhanger_chart(
            frame,
            columns[0],
            elapsed,
            app.globe_phase,
            options,
            theme,
            app.color_tier(options),
        );
        art::render_system_cliffhanger_panel(frame, columns[1], options, theme);
    } else if rows[1].height >= 18 {
        let body = Layout::vertical([Constraint::Min(8), Constraint::Length(10)]).split(rows[1]);
        art::render_system_cliffhanger_chart(
            frame,
            body[0],
            elapsed,
            app.globe_phase,
            options,
            theme,
            app.color_tier(options),
        );
        art::render_system_cliffhanger_panel(frame, body[1], options, theme);
    } else {
        art::render_system_cliffhanger_panel(frame, rows[1], options, theme);
    }

    render_workspace_footer(
        frame,
        rows[2],
        app,
        now,
        &[
            KeyHint {
                key: "TAB",
                label: "review workspaces",
            },
            KeyHint {
                key: "Q",
                label: "menu",
            },
            KeyHint {
                key: "?",
                label: "help",
            },
        ],
        theme,
    );
}

fn render_archive(
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
    render_workspace_header(frame, rows[0], app, Workspace::Archive, options, theme);
    let Some(planet) = &app.planet else {
        frame.render_widget(
            Paragraph::new("Awaiting authoritative archive state...")
                .alignment(Alignment::Center)
                .style(theme.panel_style())
                .block(panel(" ARCHIVE ", options, theme, PanelTone::Focused)),
            rows[1],
        );
        return;
    };
    if planet.archive.records.is_empty() {
        frame.render_widget(
            Paragraph::new(vec![
                Line::styled(
                    "NO PERMANENT OBSERVATIONS RECORDED",
                    Style::default().fg(theme.text_strong).bold(),
                ),
                Line::from(""),
                Line::styled(
                    "Significant player-visible evidence will remain here after discovery, independent of material losses.",
                    Style::default().fg(theme.muted),
                ),
            ])
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true })
            .style(theme.panel_style())
            .block(panel(
                " PERMANENT ARCHIVE ",
                options,
                theme,
                PanelTone::Focused,
            )),
            rows[1],
        );
    } else {
        let selected = app
            .archive_selection
            .min(planet.archive.records.len().saturating_sub(1));
        let record = &planet.archive.records[selected];
        let compact_archive = rows[1].height <= 20 || area.width < 92;
        let list_col_w = if area.width >= 76 {
            (usize::from(area.width) * 34 / 100).saturating_sub(3)
        } else {
            usize::from(area.width).saturating_sub(4)
        };
        let list = Text::from(
            planet
                .archive
                .records
                .iter()
                .enumerate()
                .flat_map(|(index, candidate)| {
                    let marker = if index == selected { ">" } else { " " };
                    let glyph = match (candidate.record_id.as_str(), options.ascii) {
                        ("observation_001", false) => "≡ ",
                        ("observation_002", false) => "▣ ",
                        ("observation_003", false) => "┆ ",
                        ("observation_004", false) => "◈ ",
                        ("observation_005", false) => "✦ ",
                        (_, true) => "* ",
                        _ => "• ",
                    };
                    let mut entry_lines = vec![Line::from(vec![
                        Span::styled(
                            format!("{marker} {:02} ", index + 1),
                            Style::default().fg(if index == selected {
                                theme.amber
                            } else {
                                theme.muted
                            }),
                        ),
                        Span::styled(
                            glyph,
                            Style::default().fg(if index == selected {
                                theme.gold
                            } else {
                                theme.cyan
                            }),
                        ),
                        Span::styled(
                            candidate.title.clone(),
                            Style::default().fg(if index == selected {
                                theme.text_strong
                            } else {
                                theme.text
                            }),
                        ),
                    ])];
                    if !compact_archive {
                        entry_lines.extend(wrap_indented_lines(
                            vec![Span::raw("     ")],
                            5,
                            &candidate.medium.to_uppercase(),
                            5,
                            list_col_w.max(20),
                            Style::default().fg(theme.muted),
                        ));
                    }
                    entry_lines
                })
                .collect::<Vec<_>>(),
        );
        let panels = if rows[1].width >= 76 {
            let (l, r) = if rows[1].width >= 96 {
                (34, 66)
            } else {
                (32, 68)
            };
            Layout::horizontal([Constraint::Percentage(l), Constraint::Percentage(r)])
                .split(rows[1])
        } else {
            Layout::vertical([Constraint::Percentage(34), Constraint::Percentage(66)])
                .split(rows[1])
        };
        let detail_inner_width = usize::from(panels[1].width.saturating_sub(2));
        let source = archive_source_label(planet, record);
        let mut detail_lines = vec![
            Line::styled(
                record.title.clone(),
                Style::default().fg(theme.text_strong).bold(),
            ),
            field_line("MEDIUM", record.medium.to_uppercase(), theme),
            field_line("SOURCE", source, theme),
            Line::from(""),
        ];
        detail_lines.extend(transmission::record_diagram(
            &record.record_id,
            options.ascii,
            app.globe_phase,
            theme,
            detail_inner_width,
        ));
        detail_lines.extend([Line::from(""), section_line("OBSERVATION", theme)]);
        detail_lines.extend(paragraph_lines(
            &terminal_text(&record.observation, options.ascii),
            Style::default().fg(theme.text),
        ));
        detail_lines.extend([
            Line::from(""),
            section_line("CURRENT INTERPRETATION", theme),
        ]);
        detail_lines.extend(paragraph_lines(
            &terminal_text(&record.interpretation, options.ascii),
            Style::default().fg(theme.muted),
        ));
        detail_lines.extend([
            Line::from(""),
            Line::styled(
                "ARCHIVE STATUS // permanent knowledge",
                Style::default().fg(theme.success),
            ),
        ]);
        if let Some(experiment) = &planet.archive.experiment {
            let state = if experiment.active {
                if experiment.running {
                    "RUNNING"
                } else {
                    "PAUSED"
                }
            } else if experiment.available {
                "READY"
            } else {
                "PENDING"
            };
            detail_lines.extend([
                Line::from(""),
                section_line("ARCHIVE EXPERIMENT", theme),
                field_line("STATE", state.into(), theme),
            ]);
            if experiment.active {
                detail_lines.extend(art::archive_experiment_waveform(
                    experiment.running,
                    operation_progress(experiment.remaining_ms, experiment.phase_total_ms),
                    app.globe_phase,
                    options.ascii,
                    theme,
                ));
            }
            detail_lines.extend([
                Line::styled(experiment.status.clone(), Style::default().fg(theme.muted)),
                field_line(
                    "PROGRESS",
                    if experiment.active {
                        format!(
                            "{}% // {}s remaining",
                            operation_progress(experiment.remaining_ms, experiment.phase_total_ms),
                            experiment.remaining_ms.div_ceil(1_000)
                        )
                    } else {
                        format!("{}s comparison", experiment.phase_total_ms / 1_000)
                    },
                    theme,
                ),
            ]);
            if experiment.available {
                detail_lines.push(key_line("ENTER", "begin timing comparison", theme));
            }
        }
        let detail = Text::from(detail_lines);
        frame.render_widget(
            Paragraph::new(list)
                .wrap(Wrap { trim: false })
                .style(theme.panel_style())
                .block(panel(
                    " PERMANENT ARCHIVE ",
                    options,
                    theme,
                    PanelTone::Focused,
                )),
            panels[0],
        );
        render_inspector_text(
            frame,
            panels[1],
            detail,
            app.archive_detail_scroll,
            &app.scroll_limits.archive_detail,
            "OBSERVATION",
            false,
            options,
            theme,
        );
    }
    let mut hints = vec![
        KeyHint {
            key: "UP/DOWN",
            label: "select record",
        },
        KeyHint {
            key: "V",
            label: "replay transmission",
        },
        KeyHint {
            key: "PGUP/DN",
            label: "scroll",
        },
        KeyHint {
            key: "TAB",
            label: "workspace",
        },
        KeyHint {
            key: "Q",
            label: "menu",
        },
    ];
    if app.archive_experiment_action().is_some() {
        hints.insert(
            0,
            KeyHint {
                key: "ENTER",
                label: "run comparison",
            },
        );
    }
    render_workspace_footer(frame, rows[2], app, now, &hints, theme);
}

/// Split authored text on line breaks so paragraph boundaries survive rendering (a single
/// `Line` would join "…signature.\nI had…" into "signature.I had").
fn paragraph_lines(text: &str, style: Style) -> Vec<Line<'static>> {
    text.split('\n')
        .map(|paragraph| Line::styled(paragraph.trim_end().to_owned(), style))
        .collect()
}

fn terminal_text(value: &str, ascii: bool) -> String {
    if !ascii {
        return value.to_owned();
    }
    let mut rendered = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '“' | '”' => rendered.push('"'),
            '‘' | '’' => rendered.push('\''),
            '—' | '–' | '·' | '•' => rendered.push('-'),
            '…' => rendered.push_str("..."),
            '✓' => rendered.push_str("OK"),
            _ if character.is_ascii() => rendered.push(character),
            _ => rendered.push('?'),
        }
    }
    rendered
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
    let archived_evidence: Vec<&mind_core::protocol::ArchiveRecordSnapshot> = planet
        .unreviewed_activity
        .iter()
        .filter_map(|activity| activity.archive_record_id.as_deref())
        .filter(|record_id| {
            planet
                .archive
                .unpresented_record_ids
                .iter()
                .any(|pending| pending == record_id)
        })
        .filter_map(|record_id| {
            planet
                .archive
                .records
                .iter()
                .find(|record| record.record_id == record_id)
        })
        .collect();
    let width = area.width.saturating_sub(6).clamp(38, 86);
    let inner_w = usize::from(width.saturating_sub(2));
    let max_inner_h = usize::from(area.height.saturating_sub(2).max(10).saturating_sub(2));
    let wrap = |lines: Vec<Line<'static>>| crate::textwrap::hang_wrap(lines, inner_w);
    let sep = if options.ascii { " | " } else { " · " };
    let header = wrap(vec![
        Line::from(vec![
            Span::styled(
                format!(
                    "CHAPTER {} // {} ",
                    transmission::roman_chapter(planet.campaign.chapter_number),
                    planet.campaign.title.to_uppercase()
                ),
                Style::default().fg(theme.cyan_bright).bold(),
            ),
            Span::styled(
                format!(
                    "[{}]",
                    transmission::step_pips(&planet.campaign, options.ascii)
                ),
                Style::default().fg(theme.amber),
            ),
        ]),
        Line::styled(
            terminal_text(&planet.campaign.question, options.ascii),
            Style::default().fg(theme.text_strong),
        ),
        Line::raw(""),
        Line::from(vec![
            Span::styled(
                "WHILE YOU WERE AWAY ",
                Style::default().fg(theme.amber).bold(),
            ),
            Span::styled(
                format!(
                    "({} event{})",
                    planet.unreviewed_activity.len(),
                    if planet.unreviewed_activity.len() == 1 {
                        ""
                    } else {
                        "s"
                    }
                ),
                Style::default().fg(theme.muted),
            ),
        ]),
    ]);
    // Newest-last activity entries, each pre-wrapped so row counts are exact.
    let activity: Vec<Vec<Line<'static>>> = if planet.unreviewed_activity.is_empty() {
        vec![wrap(vec![Line::styled(
            "  No unreviewed operational changes.",
            Style::default().fg(theme.muted),
        )])]
    } else {
        planet
            .unreviewed_activity
            .iter()
            .map(|record| {
                wrap(vec![Line::from(vec![
                    Span::styled(
                        format!("  #{:03} ", record.activity_id),
                        Style::default().fg(theme.faint),
                    ),
                    Span::styled(record.summary.clone(), Style::default().fg(theme.text)),
                ])])
            })
            .collect()
    };
    let (evidence_head, evidence_quote) = if let Some(evidence) = archived_evidence.last() {
        (
            wrap(vec![
                Line::raw(""),
                Line::styled(
                    format!(
                        "NEW EVIDENCE RECORDED // {} ({})",
                        evidence.title.to_uppercase(),
                        evidence.medium.to_uppercase()
                    ),
                    Style::default().fg(theme.gold).bold(),
                ),
            ]),
            wrap(paragraph_lines(
                &terminal_text(&evidence.observation, options.ascii),
                Style::default().fg(theme.text_strong),
            )),
        )
    } else {
        (Vec::new(), Vec::new())
    };
    let mut lines: Vec<Line<'static>> = Vec::new();
    let raw = planet.raw_inventory;
    let refined = planet.refined_inventory;
    let surveyed = planet
        .regions
        .iter()
        .filter(|r| r.knowledge >= mind_core::protocol::RegionKnowledge::Surveyed)
        .count();
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled("STORES NOW  ", Style::default().fg(theme.cyan).bold()),
        Span::styled(
            format!("MAP {surveyed}/{}", planet.regions.len()),
            Style::default().fg(theme.text_strong).bold(),
        ),
        Span::styled(sep, Style::default().fg(theme.faint)),
        Span::styled(
            format!("RAW {}/{}/{}", raw.structural, raw.conductive, raw.silicate),
            Style::default().fg(theme.text),
        ),
        Span::styled(sep, Style::default().fg(theme.faint)),
        Span::styled(
            format!(
                "REF {}/{}/{}",
                refined.structural, refined.conductive, refined.silicate
            ),
            Style::default().fg(theme.gold),
        ),
        Span::styled(sep, Style::default().fg(theme.faint)),
        Span::styled(
            format!("ANALYSIS {}", planet.research.analysis_units),
            Style::default().fg(theme.cyan_bright),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("CURRENT  ", Style::default().fg(theme.amber).bold()),
        Span::styled(
            format!(
                "{} // {}",
                planet.objective.title, planet.objective.guidance
            ),
            Style::default().fg(theme.text),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("NEXT  ", Style::default().fg(theme.amber).bold()),
        Span::styled(
            terminal_text(&planet.campaign.next_action, options.ascii),
            Style::default().fg(theme.text_strong),
        ),
    ]));
    if !planet.archive.unpresented_record_ids.is_empty() {
        let count = planet.archive.unpresented_record_ids.len();
        let noun = if count == 1 {
            "transmission"
        } else {
            "transmissions"
        };
        lines.push(Line::styled(
            format!("      {count} full discovery {noun} queued next."),
            Style::default().fg(theme.gold).bold(),
        ));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        key_span("Enter", theme),
        Span::styled(" / ", Style::default().fg(theme.faint)),
        key_span("Esc", theme),
        Span::styled(" acknowledge and continue", Style::default().fg(theme.cyan)),
    ]));
    let tail = wrap(lines);

    // Fit to the viewport: header, evidence title and tail (stores, CURRENT, NEXT, acknowledge hint)
    // always show. The newest event is kept, then as much of the quote as fits, then older events.
    let budget = max_inner_h.saturating_sub(header.len() + evidence_head.len() + tail.len());
    let activity_rows = |count: usize| -> usize {
        let shown: usize = activity.iter().rev().take(count).map(Vec::len).sum();
        shown + usize::from(count < activity.len())
    };
    let quote_rows = evidence_quote
        .len()
        .min(budget.saturating_sub(activity_rows(1)));
    let mut shown_events = 1.min(activity.len());
    while shown_events < activity.len() && activity_rows(shown_events + 1) + quote_rows <= budget {
        shown_events += 1;
    }
    let mut body = header;
    let hidden_events = activity.len() - shown_events;
    if hidden_events > 0 {
        body.push(Line::styled(
            format!("  +{hidden_events} earlier"),
            Style::default().fg(theme.faint),
        ));
    }
    for entry in &activity[hidden_events..] {
        body.extend(entry.iter().cloned());
    }
    body.extend(evidence_head);
    if quote_rows < evidence_quote.len() {
        body.extend(
            evidence_quote
                .into_iter()
                .take(quote_rows.saturating_sub(1)),
        );
        if quote_rows > 0 {
            body.push(Line::styled(
                if options.ascii {
                    "  ... full record in the queued transmission"
                } else {
                    "  … full record in the queued transmission"
                },
                Style::default().fg(theme.muted),
            ));
        }
    } else {
        body.extend(evidence_quote);
    }
    body.extend(tail);
    let height = u16::try_from(body.len() + 2)
        .unwrap_or(u16::MAX)
        .min(area.height);
    let popup = centered_rect(width, height, area);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(body)
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
    let visible = app.visible_workspaces();
    let compact = area.width < 106 || (visible.len() >= 7 && area.width < 126);
    let ultra_compact = area.width < 84;
    let workspace_label = |ws: Workspace| match ws {
        Workspace::Planet => "PLANET",
        Workspace::Overview => "OVERVIEW",
        Workspace::Probes => "PROBES",
        Workspace::Research => "RESEARCH",
        Workspace::Archive => "ARCHIVE",
        Workspace::SystemMap => "SYSTEM MAP",
        Workspace::System => "SYSTEM",
    };

    let mut left_spans = if ultra_compact {
        vec![Span::styled(
            if options.ascii { " # " } else { " ◆ " },
            Style::default().fg(theme.gold).bold(),
        )]
    } else {
        vec![
            Span::styled(
                if options.ascii { " # " } else { " ◆ " },
                Style::default().fg(theme.gold).bold(),
            ),
            Span::styled("SUBSTRATE ", Style::default().fg(theme.text_strong).bold()),
            Span::styled(" ", Style::default().fg(theme.faint)),
        ]
    };

    for (idx, &ws) in visible.iter().enumerate() {
        let num = idx + 1;
        let label = workspace_label(ws);
        let text = if compact {
            format!("{num}:{label}")
        } else {
            format!(" [{num}] {label} ")
        };
        if active == ws {
            left_spans.push(Span::styled(
                text,
                theme.selection_style().fg(theme.cyan_bright),
            ));
        } else {
            left_spans.push(Span::styled(text, Style::default().fg(theme.muted)));
        }
        left_spans.push(Span::raw(" "));
    }

    let mut left_len: usize = left_spans.iter().map(|s| s.content.chars().count()).sum();
    let total_width = usize::from(area.width);
    let mut right_spans: Vec<Span<'static>> = Vec::new();
    if let Some(planet) = &app.planet {
        let pips = transmission::step_pips(&planet.campaign, options.ascii);
        let full_chapter = format!(
            "CH {} {} ",
            transmission::roman_chapter(planet.campaign.chapter_number),
            planet.campaign.title
        );
        let short_chapter = format!(
            "CH {} ",
            transmission::roman_chapter(planet.campaign.chapter_number)
        );
        let pips_str = format!("[{pips}] ");
        let chapter_text =
            if left_len + full_chapter.chars().count() + pips_str.chars().count() < total_width {
                full_chapter
            } else {
                short_chapter
            };
        right_spans.push(Span::styled(
            chapter_text,
            Style::default().fg(theme.amber).bold(),
        ));
        right_spans.push(Span::styled(
            pips_str,
            Style::default().fg(theme.cyan_bright),
        ));
    }

    let mut right_len: usize = right_spans.iter().map(|s| s.content.chars().count()).sum();
    if left_len + right_len >= total_width && !right_spans.is_empty() {
        // Too many tabs for the chapter badge: keep the active tab named and number the rest,
        // so the chapter and its pips always stay visible.
        let mut compact_spans = vec![left_spans[0].clone()];
        for (idx, &ws) in visible.iter().enumerate() {
            let num = idx + 1;
            if active == ws {
                compact_spans.push(Span::styled(
                    format!("{num}:{}", workspace_label(ws)),
                    theme.selection_style().fg(theme.cyan_bright),
                ));
            } else {
                compact_spans.push(Span::styled(
                    num.to_string(),
                    Style::default().fg(theme.muted),
                ));
            }
            compact_spans.push(Span::raw(" "));
        }
        let compact_len: usize = compact_spans
            .iter()
            .map(|s| s.content.chars().count())
            .sum();
        if compact_len + right_len < total_width {
            left_spans = compact_spans;
            left_len = compact_len;
        } else if let Some(first) = right_spans.first_mut() {
            // Still too narrow: fall back to the short chapter label.
            if let Some(planet) = &app.planet {
                first.content = format!(
                    "CH {} ",
                    transmission::roman_chapter(planet.campaign.chapter_number)
                )
                .into();
                right_len = right_spans.iter().map(|s| s.content.chars().count()).sum();
            }
        }
    }
    if left_len + right_len < total_width {
        left_spans.push(Span::raw(" ".repeat(total_width - left_len - right_len)));
        left_spans.extend(right_spans);
    } else if !ultra_compact {
        left_spans.extend(right_spans);
    }

    let second_line = if active == Workspace::Planet && area.height <= 3 {
        if let Some(planet) = &app.planet {
            let surveyed = planet
                .regions
                .iter()
                .filter(|r| r.knowledge >= mind_core::protocol::RegionKnowledge::Surveyed)
                .count();
            let mut spans = vec![Span::styled(
                " NEXT // ",
                Style::default().fg(theme.amber).bold(),
            )];
            let (override_key, override_text) = local_next_override(app, planet);
            if let Some(key) = override_key.or(planet.campaign.next_action_key.as_deref()) {
                spans.push(Span::styled(format!(" {key} "), theme.key_style()));
                spans.push(Span::raw(" "));
            }
            let badge = format!(" MAP {surveyed}/{} ", planet.regions.len());
            let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
            let max_action = total_width.saturating_sub(used + badge.len() + 1);
            let action = terminal_text(
                if override_text.is_empty() {
                    &planet.campaign.next_action
                } else {
                    &override_text
                },
                options.ascii,
            );
            let clipped = truncate_chars(&action, max_action, options.ascii);
            let pad = total_width.saturating_sub(used + clipped.chars().count() + badge.len());
            spans.push(Span::styled(
                clipped,
                Style::default().fg(theme.text_strong),
            ));
            if pad > 0 {
                spans.push(Span::raw(" ".repeat(pad)));
            }
            spans.push(Span::styled(badge, Style::default().fg(theme.cyan_bright)));
            Line::from(spans)
        } else {
            Line::styled(
                " Awaiting planetary state",
                Style::default().fg(theme.muted),
            )
        }
    } else if let Some(planet) = &app.planet {
        let q_prefix = " QUESTION // ";
        let q_text = terminal_text(&planet.campaign.question, options.ascii);
        let q_clipped = truncate_chars(
            &q_text,
            total_width.saturating_sub(q_prefix.len() + 1),
            options.ascii,
        );
        Line::from(vec![
            Span::styled(q_prefix, Style::default().fg(theme.cyan).bold()),
            Span::styled(q_clipped, Style::default().fg(theme.text_strong).bold()),
        ])
    } else {
        Line::from(vec![
            Span::styled(" OBJECTIVE // ", Style::default().fg(theme.amber).bold()),
            Span::styled(
                "Awaiting planetary state",
                Style::default().fg(theme.text_strong).bold(),
            ),
        ])
    };

    let mut lines = vec![Line::from(left_spans), second_line];
    if active == Workspace::Planet && area.height >= 5 {
        if let Some(planet) = &app.planet {
            let mut next_spans = vec![Span::styled(
                " NEXT // ",
                Style::default().fg(theme.amber).bold(),
            )];
            let (override_key, override_text) = local_next_override(app, planet);
            let display_key = override_key.or(planet.campaign.next_action_key.as_deref());
            let display_action = if override_text.is_empty() {
                planet.campaign.next_action.as_str()
            } else {
                override_text.as_str()
            };
            if let Some(key) = display_key {
                next_spans.push(Span::styled(format!(" {key} "), theme.key_style()));
                next_spans.push(Span::raw(" "));
            }
            let used: usize = next_spans.iter().map(|s| s.content.chars().count()).sum();
            let action = terminal_text(display_action, options.ascii);
            next_spans.push(Span::styled(
                truncate_chars(&action, total_width.saturating_sub(used + 1), options.ascii),
                Style::default().fg(theme.text),
            ));
            lines.push(Line::from(next_spans));
        }
    }
    if active == Workspace::Planet && area.height >= 4 {
        lines.push(planet_store_header_line(
            app.planet.as_ref(),
            area.width,
            options.ascii,
            theme,
        ));
    }
    let rule_char = if options.ascii { "-" } else { "─" };
    lines.push(Line::styled(
        rule_char.repeat(total_width),
        Style::default().fg(theme.border),
    ));

    frame.render_widget(Paragraph::new(lines).style(theme.panel_style()), area);
}

/// Local-field NEXT guidance: while a 4x4 field is open, translate the campaign's globe-level
/// instruction into the key that acts on the selected sector, keeping the campaign progress counter.
fn local_next_override(app: &App, planet: &PlanetSnapshot) -> (Option<&'static str>, String) {
    // The daemon publishes the lead's progress counter; never parse it out of guidance prose.
    let progress = planet
        .campaign
        .lead_progress
        .as_deref()
        .map(|counter| format!(" ({counter})"))
        .unwrap_or_default();
    // While a focused survey resolves, the campaign's own monitoring text is already correct.
    if app.local_view && planet.network.active_site_survey.is_none() {
        let in_lead =
            planet.campaign.lead_region_id.as_deref() == app.selected_region_id.as_deref();
        let lead_kind = planet.campaign.lead_kind;
        match planet.campaign.next_action_key.as_deref() {
            _ if in_lead && lead_kind == Some(CampaignLeadKind::Node) => {
                let on_target = planet
                    .campaign
                    .lead_subregion_id
                    .as_deref()
                    .is_some_and(|id| app.selected_subregion().is_some_and(|site| site.id == id));
                if !on_target {
                    (
                        Some("N"),
                        "Press N to select the node that needs attention.".to_owned(),
                    )
                } else if planet.campaign.next_action_key.as_deref() == Some("V") {
                    // Already on the node: V is inert here, so point at the module keys.
                    (Some("D/W/L/O"), planet.campaign.next_action.clone())
                } else {
                    // The daemon's key (P, C, M or O) acts on this node directly.
                    (None, String::new())
                }
            }
            Some("V") if in_lead && lead_kind == Some(CampaignLeadKind::Frontier) => {
                let selected_detected = app
                    .selected_subregion()
                    .is_some_and(|s| s.knowledge == SubregionKnowledge::Detected);
                if selected_detected {
                    (
                        Some("S"),
                        format!("Press S to survey this FRONTIER sector{progress}."),
                    )
                } else {
                    (
                        Some("N"),
                        format!("Press N or arrows for a FRONTIER, then S to survey{progress}."),
                    )
                }
            }
            // Already inside the lead field: V would be inert, so point at N (lead sector) instead.
            Some("V") if in_lead => (Some("N"), planet.campaign.next_action.clone()),
            Some("X" | "B" | "R" | "A" | "S") if !in_lead || !planet.network.unlocked => (
                Some("ESC"),
                format!("Return to globe (Esc) — {}", planet.campaign.next_action),
            ),
            _ => (None, String::new()),
        }
    } else if !app.local_view
        && matches!(
            planet.campaign.next_action_key.as_deref(),
            Some("O" | "M" | "P" | "C" | "D" | "W" | "L")
        )
    {
        // Module and maintenance keys act only inside a local field; on the globe, lead with V.
        (
            Some("V"),
            format!("In the local field: {}", planet.campaign.next_action),
        )
    } else {
        (None, String::new())
    }
}

/// Word-boundary truncation with an ellipsis, shared with overlay renderers.
pub(crate) fn truncate_for_width(text: &str, max_chars: usize, ascii: bool) -> String {
    truncate_chars(text, max_chars, ascii)
}

fn truncate_chars(text: &str, max_chars: usize, ascii: bool) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    if max_chars <= 3 {
        return text.chars().take(max_chars).collect();
    }
    let ellipsis = if ascii { "..." } else { "…" };
    let budget = max_chars - ellipsis.chars().count();
    let head: String = text.chars().take(budget).collect();
    // Prefer a word boundary so no word is cut mid-way; fall back to a hard cut only when the
    // last boundary would waste more than ~40% of the budget.
    let next_is_space = text.chars().nth(budget).is_some_and(char::is_whitespace);
    let cut = if next_is_space {
        head.trim_end().to_owned()
    } else {
        match head.rfind(' ') {
            Some(at) if head[..at].chars().count() * 5 >= budget * 3 => head[..at].to_owned(),
            _ => head,
        }
    };
    let cut = cut
        .trim_end_matches(|c: char| {
            c.is_whitespace() || matches!(c, '/' | '·' | '(' | ',' | ';' | ':' | '-' | '|')
        })
        .to_owned();
    format!("{cut}{ellipsis}")
}

fn planet_store_header_line(
    planet: Option<&PlanetSnapshot>,
    width: u16,
    ascii: bool,
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
                |region_id| format!("relay {}", region_designation(planet, region_id)),
            )
        },
        |construction| {
            format!(
                "relay {}% {}",
                operation_progress(construction.remaining_ms, construction.phase_total_ms),
                region_designation(planet, &construction.region_id)
            )
        },
    );
    let profile = planet
        .research
        .profiles
        .iter()
        .find(|profile| profile.active)
        .map_or("Balanced", |profile| profile.title.as_str());
    let sep = if ascii { " | " } else { " · " };
    let raw_nonzero = raw.structural > 0 || raw.conductive > 0 || raw.silicate > 0;
    let ref_nonzero = refined.structural > 0 || refined.conductive > 0 || refined.silicate > 0;
    let fab_active = planet.active_construction.is_some() || planet.relay_region_id.is_some();
    let analysis_nonzero = planet.research.analysis_units > 0;

    if width < 140 {
        return Line::from(vec![
            Span::styled(" STORES  ", Style::default().fg(theme.amber).bold()),
            Span::styled("MAP ", Style::default().fg(theme.cyan)),
            Span::styled(
                format!("{surveyed}/{}", planet.regions.len()),
                Style::default().fg(theme.text_strong).bold(),
            ),
            Span::styled(sep, Style::default().fg(theme.faint)),
            Span::styled("RAW ", Style::default().fg(theme.muted)),
            Span::styled(
                format!("{}/{}/{}", raw.structural, raw.conductive, raw.silicate),
                Style::default().fg(if raw_nonzero {
                    theme.text_strong
                } else {
                    theme.muted
                }),
            ),
            Span::styled(sep, Style::default().fg(theme.faint)),
            Span::styled("REF ", Style::default().fg(theme.muted)),
            Span::styled(
                format!(
                    "{}/{}/{}",
                    refined.structural, refined.conductive, refined.silicate
                ),
                Style::default().fg(if ref_nonzero { theme.gold } else { theme.muted }),
            ),
            Span::styled(sep, Style::default().fg(theme.faint)),
            Span::styled("FAB ", Style::default().fg(theme.muted)),
            Span::styled(
                fabrication,
                Style::default().fg(if fab_active {
                    theme.cyan_bright
                } else {
                    theme.muted
                }),
            ),
            Span::styled(sep, Style::default().fg(theme.faint)),
            Span::styled("P ", Style::default().fg(theme.muted)),
            Span::styled(profile.to_owned(), Style::default().fg(theme.text)),
            Span::styled(sep, Style::default().fg(theme.faint)),
            Span::styled("A ", Style::default().fg(theme.muted)),
            Span::styled(
                planet.research.analysis_units.to_string(),
                Style::default().fg(if analysis_nonzero {
                    theme.cyan_bright
                } else {
                    theme.muted
                }),
            ),
        ]);
    }
    Line::from(vec![
        Span::styled(" STORES  MAP ", Style::default().fg(theme.amber).bold()),
        Span::styled(
            format!("{surveyed} / {}", planet.regions.len()),
            Style::default().fg(theme.text_strong).bold(),
        ),
        Span::styled(sep, Style::default().fg(theme.faint)),
        Span::styled("RAW ", Style::default().fg(theme.muted)),
        Span::styled(
            format!(
                "S {}  C {}  Si {}",
                raw.structural, raw.conductive, raw.silicate
            ),
            Style::default().fg(if raw_nonzero {
                theme.text_strong
            } else {
                theme.muted
            }),
        ),
        Span::styled(sep, Style::default().fg(theme.faint)),
        Span::styled("REF ", Style::default().fg(theme.muted)),
        Span::styled(
            format!(
                "S {}  C {}  Si {}",
                refined.structural, refined.conductive, refined.silicate
            ),
            Style::default().fg(if ref_nonzero { theme.gold } else { theme.muted }),
        ),
        Span::styled(sep, Style::default().fg(theme.faint)),
        Span::styled("FAB ", Style::default().fg(theme.muted)),
        Span::styled(
            fabrication,
            Style::default().fg(if fab_active {
                theme.cyan_bright
            } else {
                theme.muted
            }),
        ),
        Span::styled(sep, Style::default().fg(theme.faint)),
        Span::styled("PROFILE ", Style::default().fg(theme.muted)),
        Span::styled(profile.to_owned(), Style::default().fg(theme.text_strong)),
        Span::styled(sep, Style::default().fg(theme.faint)),
        Span::styled("ANALYSIS ", Style::default().fg(theme.muted)),
        Span::styled(
            planet.research.analysis_units.to_string(),
            Style::default().fg(if analysis_nonzero {
                theme.cyan_bright
            } else {
                theme.muted
            }),
        ),
    ])
}

fn render_probes(
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
    render_workspace_header(frame, rows[0], app, Workspace::Probes, options, theme);
    if let Some(planet) = &app.planet {
        probes::render_probe_bay_body(frame, rows[1], app, planet, options, theme, now);
    } else {
        frame.render_widget(
            Paragraph::new("Awaiting authoritative planetary state...")
                .alignment(Alignment::Center)
                .style(theme.panel_style())
                .block(panel(" PROBE BAY ", options, theme, PanelTone::Focused)),
            rows[1],
        );
    }
    let controls = if area.width < 72 {
        &[
            KeyHint {
                key: "LEFT/RIGHT",
                label: "probe",
            },
            KeyHint {
                key: "G",
                label: "rig",
            },
            KeyHint {
                key: "PGUP/DN",
                label: "specs",
            },
            KeyHint {
                key: "Q",
                label: "menu",
            },
        ][..]
    } else {
        &[
            KeyHint {
                key: "LEFT/RIGHT",
                label: "probe",
            },
            KeyHint {
                key: "G",
                label: "rig",
            },
            KeyHint {
                key: "PGUP/DN",
                label: "specs",
            },
            KeyHint {
                key: "ENTER",
                label: "region",
            },
            KeyHint {
                key: "TAB",
                label: "view",
            },
            KeyHint {
                key: "Q",
                label: "menu",
            },
            KeyHint {
                key: "?",
                label: "help",
            },
        ][..]
    };
    render_workspace_footer(frame, rows[2], app, now, controls, theme);
}

fn render_globe(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    planet: &PlanetSnapshot,
    options: RenderOptions,
    theme: Theme,
    now: Instant,
) {
    let inner_height = area.height.saturating_sub(2).max(1);
    let inner_width = area.width.saturating_sub(2).max(1);
    let flash_ids = app.active_discovery_flash_ids(now);
    let mut cache = app.globe_cache.borrow_mut();
    frame.render_widget(
        Paragraph::new(globe::render_cached_with_flashes(
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
                ansi256: app.ansi256,
                theme,
            },
            &mut cache,
            &flash_ids,
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

fn render_local_sites(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    planet: &PlanetSnapshot,
    options: RenderOptions,
    theme: Theme,
    now: Instant,
) {
    let sites = app.selected_subregions();
    let region_name = app
        .selected_region()
        .map_or("REGION", |region| region.designation.as_str());
    let local_title = format!(
        " {region_name} // LOCAL FIELD {}x{} ",
        planet.network.subregion_columns, planet.network.subregion_rows
    );
    let outer = panel(&local_title, options, theme, PanelTone::Focused);
    let inner = outer.inner(area);
    frame.render_widget(outer, area);
    if sites.is_empty() {
        frame.render_widget(
            Paragraph::new(
                "Local resolution is unavailable until the planetary network chapter opens.",
            )
            .alignment(Alignment::Center)
            .style(theme.canvas_style()),
            inner,
        );
        return;
    }
    let row_count = u16::from(planet.network.subregion_rows.max(1));
    let column_count = u16::from(planet.network.subregion_columns.max(1));
    // Equal tile sizes: spare rows/columns become a centered margin instead of enlarging one
    // row or column (which made only that row show state labels and widened one column).
    let tile_h = (inner.height / row_count).max(1);
    let tile_w = (inner.width / column_count).max(1);
    let grid = Rect::new(
        inner.x + (inner.width - tile_w * column_count) / 2,
        inner.y + (inner.height - tile_h * row_count) / 2,
        tile_w * column_count,
        tile_h * row_count,
    );
    let rows = Layout::vertical(
        (0..row_count)
            .map(|_| Constraint::Length(tile_h))
            .collect::<Vec<_>>(),
    )
    .split(grid);
    let min_tile_inner_rows = rows
        .iter()
        .map(|r| r.height.saturating_sub(2))
        .min()
        .unwrap_or(4);
    let selected_id = app.selected_subregion().map(|site| site.id.as_str());
    for (row_index, row_area) in rows.iter().enumerate() {
        let columns = Layout::horizontal(
            (0..column_count)
                .map(|_| Constraint::Length(tile_w))
                .collect::<Vec<_>>(),
        )
        .split(*row_area);
        for (column_index, cell) in columns.iter().enumerate() {
            let Some(site) = sites.iter().find(|site| {
                usize::from(site.row) == row_index && usize::from(site.column) == column_index
            }) else {
                continue;
            };
            let selected = selected_id == Some(site.id.as_str());
            let base = site.base_id.as_ref().and_then(|base_id| {
                planet
                    .network
                    .bases
                    .iter()
                    .find(|candidate| &candidate.id == base_id)
            });
            let active_survey = planet
                .network
                .active_site_survey
                .as_ref()
                .is_some_and(|survey| survey.subregion_id == site.id);
            let scout_present = planet.active_mission.as_ref().is_some_and(|mission| {
                mission.target_region_id == site.region_id && mission.phase == ScoutState::Holding
            });
            let coordinate = format!(
                "{}{}",
                char::from(b'A'.saturating_add(site.row)),
                site.column + 1
            );
            let mut art_lines = art::local_sector_art(art::LocalSectorArtParams {
                site,
                base,
                active_survey,
                scout_present,
                selected,
                phase: app.globe_phase,
                options,
                theme,
            });
            if min_tile_inner_rows < 5 && art_lines.len() > 1 {
                art_lines.truncate(1);
            }
            // A single text row shows the sector ID rather than decoration.
            if min_tile_inner_rows < 2 {
                art_lines.clear();
            }
            let mut lines = art_lines;
            let sep = if options.ascii { "-" } else { "·" };
            let tile_inner_w = cell.width.saturating_sub(2);
            let short_id = site
                .designation
                .split(" / ")
                .last()
                .unwrap_or(site.designation.as_str());
            let id_label = if tile_inner_w >= 11 {
                format!("{coordinate} {sep} {short_id}")
            } else {
                format!("{coordinate} {short_id}")
            };
            lines.push(Line::styled(
                id_label,
                Style::default().fg(theme.text_strong).bold(),
            ));
            if let (Some(energy), Some(hazard)) = (site.energy_yield, site.hazard) {
                let status_str = if tile_inner_w >= 13 {
                    let h = match hazard {
                        mind_core::protocol::RiskLevel::Low => "LOW 5%",
                        mind_core::protocol::RiskLevel::Moderate => "MOD 15%",
                        mind_core::protocol::RiskLevel::High => "HIGH 30%",
                    };
                    format!("E{energy} {sep} {h}")
                } else if tile_inner_w >= 10 {
                    let h = match hazard {
                        mind_core::protocol::RiskLevel::Low => "L 5%",
                        mind_core::protocol::RiskLevel::Moderate => "M 15%",
                        mind_core::protocol::RiskLevel::High => "H 30%",
                    };
                    format!("E{energy} {sep} {h}")
                } else {
                    let pct = match hazard {
                        mind_core::protocol::RiskLevel::Low => "5%",
                        mind_core::protocol::RiskLevel::Moderate => "15%",
                        mind_core::protocol::RiskLevel::High => "30%",
                    };
                    format!("E{energy} {pct}")
                };
                lines.push(Line::styled(
                    status_str,
                    Style::default().fg(if hazard == mind_core::protocol::RiskLevel::High {
                        theme.warning
                    } else if selected {
                        theme.text_strong
                    } else {
                        theme.text
                    }),
                ));
            } else {
                let knowledge_label = if active_survey {
                    if tile_inner_w >= 9 {
                        "SCANNING"
                    } else {
                        "SCAN"
                    }
                } else if tile_inner_w >= 9 {
                    match site.knowledge {
                        SubregionKnowledge::Unknown => "UNCHARTED",
                        SubregionKnowledge::Detected => "FRONTIER",
                        SubregionKnowledge::Surveyed => "SURVEYED",
                        SubregionKnowledge::Observed => "OBSERVED",
                    }
                } else {
                    match site.knowledge {
                        SubregionKnowledge::Unknown => "FOG",
                        SubregionKnowledge::Detected => "FRNT",
                        SubregionKnowledge::Surveyed => "SRVD",
                        SubregionKnowledge::Observed => "OBSV",
                    }
                };
                lines.push(Line::styled(
                    knowledge_label,
                    Style::default().fg(if active_survey {
                        theme.amber
                    } else if selected {
                        theme.text_strong
                    } else {
                        theme.muted
                    }),
                ));
            }
            if let Some(base) = base {
                lines.push(Line::styled(
                    base.designation.clone(),
                    Style::default()
                        .fg(if base.connected {
                            theme.success
                        } else {
                            theme.warning
                        })
                        .bold(),
                ));
            } else if let Some(deposit) = &site.deposit {
                lines.push(Line::styled(
                    format!("{:?} {}", deposit.resource, deposit.remaining),
                    Style::default().fg(theme.amber),
                ));
            }
            let newly_scanned = app.has_discovery_flash(&site.id, now);
            let tile_title = if tile_inner_w < 10 {
                if selected {
                    " SEL "
                } else if newly_scanned {
                    " NEW "
                } else if active_survey {
                    " SCN "
                } else if site.knowledge == SubregionKnowledge::Unknown {
                    " FOG "
                } else {
                    " SEC "
                }
            } else if selected {
                " SELECTED "
            } else if newly_scanned {
                " NEW SCAN "
            } else if active_survey {
                " SCAN "
            } else if site.knowledge == SubregionKnowledge::Unknown {
                " FOG "
            } else {
                " SECTOR "
            };
            frame.render_widget(
                Paragraph::new(lines)
                    .alignment(Alignment::Center)
                    .style(if selected {
                        theme.raised_style()
                    } else {
                        theme.panel_style()
                    })
                    .block(panel(
                        tile_title,
                        options,
                        theme,
                        if selected {
                            PanelTone::Focused
                        } else if newly_scanned || active_survey {
                            PanelTone::Modal
                        } else {
                            PanelTone::Standard
                        },
                    )),
                *cell,
            );
        }
    }
}

fn render_local_inspector(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    planet: &PlanetSnapshot,
    options: RenderOptions,
    theme: Theme,
) {
    let text = app.selected_subregion().map_or_else(
        || Text::from(Line::styled("No local site selected.", theme.muted)),
        |site| {
            if let Some(action) = &app.pending_action {
                return local_confirmation(action, site, planet, theme);
            }
            let mut lines = vec![Line::styled(
                site.designation.clone(),
                Style::default().fg(theme.text_strong).bold(),
            )];
            let site_base = planet
                .network
                .bases
                .iter()
                .find(|base| base.subregion_id == site.id);
            let active_survey = planet
                .network
                .active_site_survey
                .as_ref()
                .is_some_and(|survey| survey.subregion_id == site.id);
            if area.height >= 20 {
                let vista_w = area.width.saturating_sub(2).max(16);
                let vista_h = if area.height >= 32 { 5 } else { 4 };
                lines.extend(
                    vista::render_local_vista(
                        site,
                        site_base,
                        active_survey,
                        vista_w,
                        vista_h,
                        app.globe_phase,
                        app.color_tier(options),
                    )
                    .lines,
                );
                if let Some(base) = site_base {
                    lines.push(local_module_strip(base, options.ascii, theme));
                }
                lines.push(Line::from(""));
            } else {
                lines.push(Line::from(""));
            }
            lines.extend([
                section_line("SITE CONDITIONS", theme),
                field_line(
                    "POSITION",
                    format!(
                        "{}{} // row {} of {} // column {} of {}",
                        char::from(b'A'.saturating_add(site.row)),
                        site.column + 1,
                        site.row + 1,
                        planet.network.subregion_rows,
                        site.column + 1,
                        planet.network.subregion_columns
                    ),
                    theme,
                ),
                field_line("KNOWLEDGE", format!("{:?}", site.knowledge), theme),
                field_line(
                    "TERRAIN",
                    site.class.map_or_else(
                        || "unresolved".into(),
                        |class| subregion_class_title(class).into(),
                    ),
                    theme,
                ),
                field_line(
                    "HAZARD",
                    site.hazard
                        .map_or("unresolved".into(), |risk| risk_text(risk).into()),
                    theme,
                ),
                field_line(
                    "ENERGY",
                    site.energy_yield
                        .map_or("unresolved".into(), |value| format!("yield {value} / 6")),
                    theme,
                ),
            ]);
            if let Some(deposit) = &site.deposit {
                lines.push(field_line(
                    "DEPOSIT",
                    format!(
                        "{:?} // {} units // {}",
                        deposit.resource,
                        deposit.remaining,
                        risk_text(deposit.risk)
                    ),
                    theme,
                ));
            } else if site.knowledge >= SubregionKnowledge::Surveyed {
                lines.push(field_line("DEPOSIT", "none resolved".into(), theme));
            }
            if let Some(survey) = planet
                .network
                .active_site_survey
                .as_ref()
                .filter(|survey| survey.subregion_id == site.id)
            {
                lines.push(field_line(
                    "LOCAL SCAN",
                    format!(
                        "{}% // {}s",
                        operation_progress(survey.remaining_ms, survey.phase_total_ms),
                        survey.remaining_ms.div_ceil(1_000)
                    ),
                    theme,
                ));
            }
            if let Some(project) = planet
                .network
                .active_projects
                .iter()
                .find(|project| project.subregion_id == site.id)
            {
                lines.push(field_line(
                    "PROJECT",
                    format!(
                        "{} // {}% // {}s",
                        if project.maintenance {
                            "maintenance"
                        } else {
                            project.module.map_or("foothold", module_title)
                        },
                        operation_progress(project.remaining_ms, project.phase_total_ms),
                        project.remaining_ms.div_ceil(1_000)
                    ),
                    theme,
                ));
                if project.logistics_supported {
                    lines.push(field_line(
                        "SUPPLY",
                        "depot-supported // construction time reduced 25%".into(),
                        theme,
                    ));
                }
            }
            lines.push(Line::from(""));
            lines.push(section_line("LOCAL INFRASTRUCTURE", theme));
            if let Some(base) = site_base {
                lines.push(field_line(
                    "NODE",
                    format!(
                        "{} // {}",
                        base.designation,
                        if base.connected {
                            "CONNECTED"
                        } else {
                            "ISOLATED"
                        }
                    ),
                    theme,
                ));
                lines.push(field_line(
                    "POWER",
                    format!(
                        "{} gen / {} demand // {}/{} stored",
                        base.generation, base.demand, base.stored_energy, base.energy_capacity
                    ),
                    theme,
                ));
                lines.push(field_line(
                    "INTEGRITY",
                    format!("{}%", base.integrity),
                    theme,
                ));
                lines.push(field_line(
                    "MODULES",
                    if base.modules.is_empty() {
                        "none".into()
                    } else {
                        base.modules
                            .iter()
                            .map(|module| {
                                format!(
                                    "{} {}",
                                    module_title(module.kind),
                                    if !module.enabled {
                                        "[STANDBY]"
                                    } else if module.active {
                                        "[ONLINE]"
                                    } else {
                                        "[STARVED]"
                                    }
                                )
                            })
                            .collect::<Vec<_>>()
                            .join(" / ")
                    },
                    theme,
                ));
            } else {
                lines.push(Line::styled(
                    "No permanent presence at this site.",
                    theme.muted,
                ));
            }
            lines.push(Line::from(""));
            lines.push(section_line(
                &format!(
                    "SITE OPERATIONS  CREWS {}/{}",
                    planet.network.active_projects.len(),
                    planet.network.project_capacity
                ),
                theme,
            ));
            lines.extend(local_action_lines(app, planet, site, theme));
            Text::from(lines)
        },
    );
    let confirming = app.pending_action.is_some();
    render_inspector_text(
        frame,
        area,
        text,
        app.region_panel_scroll,
        &app.scroll_limits.region_panel,
        if confirming {
            "SITE CONFIRMATION"
        } else {
            "LOCAL INSPECTOR"
        },
        confirming,
        options,
        theme,
    );
}

/// Render an inspector body pre-wrapped with hanging indents, clamped scroll, and an overflow cue
/// in the title (`[PgDn ▼]`, `[PgUp ▲]`, or both) whenever content extends beyond the panel.
#[allow(clippy::too_many_arguments)]
fn render_inspector_text(
    frame: &mut Frame<'_>,
    area: Rect,
    text: Text<'static>,
    scroll: u16,
    limit: &std::cell::Cell<u16>,
    title: &str,
    confirming: bool,
    options: RenderOptions,
    theme: Theme,
) {
    let inner_width = usize::from(area.width.saturating_sub(2));
    let inner_height = usize::from(area.height.saturating_sub(2));
    let mut rows = crate::textwrap::hang_wrap(text.lines, inner_width);
    let is_heading = |line: &Line<'_>| {
        let plain: String = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        let plain = plain.trim_end();
        plain == "UNAVAILABLE HERE" || plain.ends_with(" //")
    };
    // Keep a heading with the row it introduces: never leave it alone on the last visible row.
    let first_scroll = usize::from(scroll).min(rows.len().saturating_sub(inner_height));
    let last_visible = first_scroll + inner_height;
    if inner_height > 1 && last_visible < rows.len() && is_heading(&rows[last_visible - 1]) {
        rows.insert(last_visible - 1, Line::raw(""));
    }
    let max_scroll = rows.len().saturating_sub(inner_height);
    limit.set(u16::try_from(max_scroll).unwrap_or(u16::MAX));
    let scroll = usize::from(scroll).min(max_scroll);
    let more_above = scroll > 0;
    let more_below = scroll < max_scroll;
    let cue = match (more_above, more_below, options.ascii) {
        (false, false, _) => "",
        (false, true, true) => " [PgDn more]",
        (false, true, false) => " [PgDn ▼]",
        (true, false, true) => " [PgUp]",
        (true, false, false) => " [PgUp ▲]",
        (true, true, true) => " [PgUp/PgDn]",
        (true, true, false) => " [PgUp/PgDn ▲▼]",
    };
    let full_title = format!(" {title}{cue} ");
    let block = panel(
        &full_title,
        options,
        theme,
        if confirming {
            PanelTone::Modal
        } else {
            PanelTone::Standard
        },
    );
    frame.render_widget(
        Paragraph::new(rows)
            .scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0))
            .style(if confirming {
                theme.raised_style()
            } else {
                theme.panel_style()
            })
            .block(block),
        area,
    );
}

fn local_confirmation(
    action: &ContextAction,
    site: &SubregionSnapshot,
    planet: &PlanetSnapshot,
    theme: Theme,
) -> Text<'static> {
    let (title, warning) = match action {
        ContextAction::ExtractSubregion { .. } => (
            "CONFIRM LOCAL EXTRACTION",
            format!(
                "{} loss risk using {}. Probe and cargo may be permanently lost.",
                site.deposit
                    .as_ref()
                    .map_or("UNKNOWN", |deposit| risk_text(deposit.risk)),
                planet.extractor.loadout.title()
            ),
        ),
        ContextAction::EstablishBase(_) => (
            "CONFIRM FOOTHOLD",
            "Consume S 10 / C 6 / Si 4. Site energy and hazard shape every later module.".into(),
        ),
        ContextAction::MaintainBase(_) => (
            "CONFIRM MAINTENANCE",
            "Consume S 4 / C 2. A 60-second cycle restores 35% node integrity.".into(),
        ),
        ContextAction::ConstructBaseModule { module, .. } => {
            let cost = module.cost();
            (
                "CONFIRM MODULE",
                format!(
                    "Build {} for S {} / C {} / Si {}.",
                    module_title(*module),
                    cost.structural,
                    cost.conductive,
                    cost.silicate
                ),
            )
        }
        _ => (
            "CONFIRM ACTION",
            "Confirm the selected local operation.".into(),
        ),
    };
    Text::from(vec![
        Line::styled(title, Style::default().fg(theme.warning).bold()),
        Line::from(""),
        Line::styled(
            site.designation.clone(),
            Style::default().fg(theme.text_strong).bold(),
        ),
        Line::from(""),
        Line::styled(warning, Style::default().fg(theme.text_strong)),
        Line::from(""),
        key_line("ENTER", "confirm", theme),
        key_line("ESC", "cancel", theme),
    ])
}

fn local_action_lines(
    app: &App,
    planet: &PlanetSnapshot,
    site: &SubregionSnapshot,
    theme: Theme,
) -> Vec<Line<'static>> {
    let has_node = planet
        .network
        .bases
        .iter()
        .any(|base| base.subregion_id == site.id);
    let mut lines = vec![
        local_action_line(
            "S",
            "focused survey",
            app.subregion_survey_action().is_some(),
            local_survey_blocker(app, planet, site),
            theme,
        ),
        local_action_line(
            "X",
            "extract site",
            app.subregion_extraction_action().is_some(),
            local_extraction_blocker(planet, site),
            theme,
        ),
        local_action_line(
            "B",
            "establish node",
            app.base_establishment_action().is_some(),
            local_base_blocker(app, planet, site),
            theme,
        ),
    ];
    if !has_node {
        lines.push(local_action_line(
            "P/C/D/W/L/O",
            "node modules",
            false,
            if planet.network.unlocked {
                "establish a node at this site first".into()
            } else {
                "unlock planetary network, then establish a node".into()
            },
            theme,
        ));
        return lines;
    }
    lines.push(local_action_line(
        "M",
        "maintenance",
        app.base_maintenance_action().is_some(),
        local_maintenance_blocker(planet, site),
        theme,
    ));
    for (key, module) in [
        ("P", BaseModuleKind::PowerArray),
        ("C", BaseModuleKind::Communications),
        ("D", BaseModuleKind::Depot),
        ("W", BaseModuleKind::Workshop),
        ("L", BaseModuleKind::Laboratory),
        ("O", BaseModuleKind::Observatory),
    ] {
        let mode = app.base_module_mode_action(module);
        let available = mode.is_some() || app.base_module_action(module).is_some();
        let label = match mode {
            Some(ContextAction::SetBaseModuleEnabled { enabled: true, .. }) => {
                format!("restore {}", module_action_title(module))
            }
            Some(ContextAction::SetBaseModuleEnabled { enabled: false, .. }) => {
                format!("suspend {}", module_action_title(module))
            }
            _ => module_action_title(module).into(),
        };
        let blocker = local_module_blocker(planet, site, module);
        lines.push(local_action_line(key, label, available, blocker, theme));
    }
    lines
}

fn local_action_line(
    key: &'static str,
    label: impl Into<String>,
    available: bool,
    blocker: String,
    theme: Theme,
) -> Line<'static> {
    let label = label.into();
    Line::from(vec![
        Span::styled(format!("[{key}] "), Style::default().fg(theme.amber).bold()),
        Span::styled(
            label,
            Style::default().fg(if available {
                theme.text_strong
            } else {
                theme.muted
            }),
        ),
        Span::styled(" // ", Style::default().fg(theme.faint)),
        Span::styled(
            if available { "ready".into() } else { blocker },
            Style::default().fg(if available {
                theme.success
            } else {
                theme.muted
            }),
        ),
    ])
}

fn local_survey_blocker(app: &App, planet: &PlanetSnapshot, site: &SubregionSnapshot) -> String {
    if site.knowledge >= SubregionKnowledge::Surveyed {
        return "site already resolved".into();
    }
    if site.knowledge == SubregionKnowledge::Unknown {
        return "survey an adjacent frontier sector first".into();
    }
    if let Some(active) = &planet.network.active_site_survey {
        return if active.subregion_id == site.id {
            "survey in progress on this sector".into()
        } else {
            "another local survey is active".into()
        };
    }
    if app.selected_region().is_some_and(|region| {
        planet.active_mission.as_ref().is_none_or(|mission| {
            mission.target_region_id != region.id || mission.phase != ScoutState::Holding
        })
    }) {
        return "hold the scout in this region".into();
    }
    "local contact required".into()
}

fn local_extraction_blocker(planet: &PlanetSnapshot, site: &SubregionSnapshot) -> String {
    if !planet.network.unlocked {
        return crate::app::LOCAL_FIELD_UNLOCK_GUIDANCE.into();
    }
    if site.knowledge < SubregionKnowledge::Surveyed {
        return "focused survey required".into();
    }
    if site.deposit.is_none() {
        return "no local deposit".into();
    }
    if site
        .deposit
        .as_ref()
        .is_some_and(|deposit| deposit.remaining == 0)
    {
        return "deposit depleted".into();
    }
    format!("extractor is {:?}", planet.extractor.state).to_lowercase()
}

fn extractor_loadout_blocker(planet: &PlanetSnapshot) -> String {
    match planet.extractor.state {
        ExtractorState::Ready => "ready".into(),
        ExtractorState::Rebuilding => format!(
            "rebuilding // {}s remaining",
            planet.extractor.rebuild_remaining_ms.div_ceil(1_000)
        ),
        _ => planet.active_extraction.as_ref().map_or_else(
            || format!("extractor is {:?}", planet.extractor.state).to_lowercase(),
            |mission| format!("rig locked during mission #{:03}", mission.mission_id),
        ),
    }
}

fn local_base_blocker(app: &App, planet: &PlanetSnapshot, site: &SubregionSnapshot) -> String {
    if !planet.network.unlocked {
        return crate::app::LOCAL_FIELD_UNLOCK_GUIDANCE.into();
    }
    if site.knowledge < SubregionKnowledge::Surveyed {
        return "focused survey required".into();
    }
    if site.base_id.is_some() {
        return "node already established".into();
    }
    if app.selected_region().is_some_and(|region| {
        planet
            .network
            .bases
            .iter()
            .any(|base| base.region_id == region.id)
    }) {
        return "one node per region".into();
    }
    if app
        .selected_region()
        .is_some_and(|region| !region.within_scout_range)
    {
        return "outside connected operating range".into();
    }
    if planet
        .network
        .active_projects
        .iter()
        .any(|project| project.region_id == site.region_id)
    {
        return "local project active".into();
    }
    if planet.network.active_projects.len() >= planet.network.project_capacity {
        return "all construction crews committed".into();
    }
    missing_stock_message(planet, 10, 6, 4)
}

fn local_maintenance_blocker(planet: &PlanetSnapshot, site: &SubregionSnapshot) -> String {
    let Some(base) = planet
        .network
        .bases
        .iter()
        .find(|base| base.subregion_id == site.id)
    else {
        return "node required".into();
    };
    if base.integrity >= 100 {
        return "integrity already full".into();
    }
    if planet
        .network
        .active_projects
        .iter()
        .any(|project| project.region_id == site.region_id)
    {
        return "local project active".into();
    }
    if planet.network.active_projects.len() >= planet.network.project_capacity {
        return "all construction crews committed".into();
    }
    missing_stock_message(planet, 4, 2, 0)
}

fn local_module_blocker(
    planet: &PlanetSnapshot,
    site: &SubregionSnapshot,
    module: BaseModuleKind,
) -> String {
    let Some(base) = planet
        .network
        .bases
        .iter()
        .find(|base| base.subregion_id == site.id)
    else {
        return "node required".into();
    };
    if base.modules.iter().any(|built| built.kind == module) {
        return "module already built".into();
    }
    if base.integrity <= 20 {
        return "maintenance required".into();
    }
    if planet
        .network
        .active_projects
        .iter()
        .any(|project| project.region_id == site.region_id)
    {
        return "local project active".into();
    }
    if planet.network.active_projects.len() >= planet.network.project_capacity {
        return "all construction crews committed".into();
    }
    let has = |kind| base.modules.iter().any(|built| built.kind == kind);
    if module != BaseModuleKind::PowerArray && !has(BaseModuleKind::PowerArray) {
        return "power array required".into();
    }
    if matches!(
        module,
        BaseModuleKind::Workshop | BaseModuleKind::Laboratory | BaseModuleKind::Observatory
    ) && !has(BaseModuleKind::Communications)
    {
        return "communications required".into();
    }
    if module == BaseModuleKind::Observatory {
        let completed_research = planet
            .research
            .options
            .iter()
            .filter(|option| option.completed)
            .count();
        if completed_research < planet.network.horizon.required_research {
            return format!(
                "research {completed_research}/{}",
                planet.network.horizon.required_research
            );
        }
        if planet.network.horizon.surveyed_subregions < planet.network.horizon.required_subregions {
            return format!(
                "local surveys {}/{}",
                planet.network.horizon.surveyed_subregions,
                planet.network.horizon.required_subregions
            );
        }
        if !planet.network.bases.iter().any(|candidate| {
            candidate
                .modules
                .iter()
                .any(|built| built.kind == BaseModuleKind::Laboratory && built.active)
        }) {
            return "powered laboratory required".into();
        }
    }
    let cost = module.cost();
    missing_stock_message(planet, cost.structural, cost.conductive, cost.silicate)
}

fn missing_stock_message(
    planet: &PlanetSnapshot,
    structural: u32,
    conductive: u32,
    silicate: u32,
) -> String {
    let stock = planet.refined_inventory;
    if stock.structural >= structural
        && stock.conductive >= conductive
        && stock.silicate >= silicate
    {
        return "ready when selected".into();
    }
    format!(
        "need {structural}/{conductive}/{silicate}; have {}/{}/{}",
        stock.structural, stock.conductive, stock.silicate
    )
}

fn module_title(module: BaseModuleKind) -> &'static str {
    module.title()
}

fn module_action_title(module: BaseModuleKind) -> &'static str {
    match module {
        BaseModuleKind::PowerArray => "power",
        BaseModuleKind::Communications => "communications",
        BaseModuleKind::Depot => "depot",
        BaseModuleKind::Workshop => "workshop",
        BaseModuleKind::Laboratory => "laboratory",
        BaseModuleKind::Observatory => "observation",
    }
}

fn subregion_class_title(class: mind_core::protocol::SubregionClass) -> &'static str {
    match class {
        mind_core::protocol::SubregionClass::ShelteredShelf => "sheltered shelf",
        mind_core::protocol::SubregionClass::BasinFloor => "basin floor",
        mind_core::protocol::SubregionClass::OpenPlain => "open plain",
        mind_core::protocol::SubregionClass::ExposedRidge => "exposed ridge",
        mind_core::protocol::SubregionClass::FractureField => "fracture field",
        mind_core::protocol::SubregionClass::PolarMargin => "polar margin",
    }
}

fn render_region_inspector(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    planet: &PlanetSnapshot,
    options: RenderOptions,
    theme: Theme,
    now: Instant,
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
                                "{risk} loss risk using {}. Probe and carried material may be permanently lost.",
                                planet.extractor.loadout.title()
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

            let mut lines = vec![region_title_line(region, options.ascii, theme)];
            let region_has_concurrent_ops = planet
                .active_mission
                .as_ref()
                .is_some_and(|m| m.target_region_id == region.id)
                && planet
                    .active_extraction
                    .as_ref()
                    .is_some_and(|m| m.target_region_id == region.id);
            let show_top_vista = area.height >= 16 && !region_has_concurrent_ops;
            if show_top_vista {
                let vista_w = area.width.saturating_sub(2).max(16);
                let vista_h = if area.height >= 26 { 6 } else { 4 };
                lines.extend(
                    vista::render_region_vista(
                        region,
                        planet,
                        vista_w,
                        vista_h,
                        app.globe_phase,
                        app.color_tier(options),
                    )
                    .lines,
                );
                lines.push(region_vista_caption(
                    region,
                    planet,
                    area.width < 58,
                    options.ascii,
                    theme,
                ));
            }
            if area.height < 14 && area.width < 60 {
                lines.push(Line::from(""));
                lines.extend(region_data_lines(region, theme));
                lines.push(Line::from(""));
                lines.push(section_line("ACTIVE IN REGION", theme));
                lines.extend(local_operation_lines(
                    app,
                    planet,
                    region,
                    options.ascii,
                    theme,
                    now,
                ));
                lines.push(Line::from(""));
                lines.push(section_line("REGION OPERATIONS", theme));
                lines.extend(region_action_lines(app, planet, region, theme));
                lines.push(Line::from(""));
                lines.push(section_line("LOCAL PRESENCE", theme));
                lines.extend(region_presence_lines(planet, region, theme));
            } else if area.height < 22 && !region_has_concurrent_ops {
                lines.push(Line::from(""));
                lines.push(section_line("REGION OPERATIONS", theme));
                lines.extend(region_action_lines(app, planet, region, theme));
                lines.push(Line::from(""));
                lines.push(section_line("ACTIVE IN REGION", theme));
                lines.extend(local_operation_lines(
                    app,
                    planet,
                    region,
                    options.ascii,
                    theme,
                    now,
                ));
                lines.push(Line::from(""));
                lines.push(section_line("LOCAL PRESENCE", theme));
                lines.extend(region_presence_lines(planet, region, theme));
                lines.push(Line::from(""));
                lines.extend(region_data_lines(region, theme));
            } else {
                lines.push(Line::from(""));
                lines.push(section_line("ACTIVE IN REGION", theme));
                lines.extend(local_operation_lines(
                    app,
                    planet,
                    region,
                    options.ascii,
                    theme,
                    now,
                ));
                if area.height >= 14 {
                    lines.push(Line::from(""));
                    lines.push(section_line("LOCAL PRESENCE", theme));
                    lines.extend(region_presence_lines(planet, region, theme));
                }
                lines.push(Line::from(""));
                lines.push(section_line("REGION OPERATIONS", theme));
                lines.extend(region_action_lines(app, planet, region, theme));
                if !show_top_vista && area.height >= 22 {
                    let vista_w = area.width.saturating_sub(2).max(16);
                    lines.push(Line::from(""));
                    lines.extend(
                        vista::render_region_vista(
                            region,
                            planet,
                            vista_w,
                            5,
                            app.globe_phase,
                            app.color_tier(options),
                        )
                        .lines,
                    );
                    lines.push(region_vista_caption(
                        region,
                        planet,
                        area.width < 58,
                        options.ascii,
                        theme,
                    ));
                }
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
    let confirming = app.pending_action.is_some();
    render_inspector_text(
        frame,
        area,
        text,
        app.region_panel_scroll,
        &app.scroll_limits.region_panel,
        if confirming {
            "REGION CONFIRMATION"
        } else {
            "REGION INSPECTOR"
        },
        confirming,
        options,
        theme,
    );
}

fn region_title_line(
    region: &mind_core::protocol::RegionSnapshot,
    ascii: bool,
    theme: Theme,
) -> Line<'static> {
    let sep = if ascii { " - " } else { " · " };
    let mut spans = vec![Span::styled(
        region.designation.clone(),
        Style::default().fg(theme.text_strong).bold(),
    )];
    if region.core {
        spans.push(Span::styled(
            "  [CORE]",
            Style::default().fg(theme.gold).bold(),
        ));
    } else if region.relay {
        spans.push(Span::styled(
            "  [RELAY]",
            Style::default().fg(theme.success).bold(),
        ));
    } else {
        let knowledge = format!("{:?}", region.knowledge).to_uppercase();
        spans.push(Span::styled(
            format!("  [{knowledge}]"),
            Style::default().fg(
                if region.knowledge == mind_core::protocol::RegionKnowledge::Unknown {
                    theme.muted
                } else {
                    theme.cyan_bright
                },
            ),
        ));
    }
    spans.push(Span::styled(sep, Style::default().fg(theme.faint)));
    spans.push(Span::styled(
        format!("{} HOP", region.hop_distance_from_core),
        Style::default().fg(if region.within_scout_range || region.core {
            theme.text
        } else {
            theme.warning
        }),
    ));
    Line::from(spans)
}

fn local_module_strip(
    base: &mind_core::protocol::BaseSnapshot,
    ascii: bool,
    theme: Theme,
) -> Line<'static> {
    // An empty lead span stops the hanging-indent wrapper treating the first badge as a label,
    // so a wrapped badge row starts at the left edge instead of under the first badge.
    let mut spans = vec![Span::raw("")];
    let slots = [
        (BaseModuleKind::PowerArray, "PP", "P·PWR", "P-PWR"),
        (BaseModuleKind::Communications, "CC", "C·COM", "C-COM"),
        (BaseModuleKind::Depot, "DD", "D·DEP", "D-DEP"),
        (BaseModuleKind::Workshop, "WW", "W·WRK", "W-WRK"),
        (BaseModuleKind::Laboratory, "LL", "L·LAB", "L-LAB"),
        (BaseModuleKind::Observatory, "OO", "O·OBS", "O-OBS"),
    ];
    for (kind, code, uni_tag, asc_tag) in slots {
        let Some(module) = base.modules.iter().find(|m| m.kind == kind) else {
            continue;
        };
        let color = if !module.enabled {
            theme.muted
        } else if module.active {
            theme.cyan_bright
        } else {
            theme.warning
        };
        let badge = if ascii {
            format!("[{code}] {asc_tag}  ")
        } else {
            format!("▐{code}▌ {uni_tag}  ")
        };
        spans.push(Span::styled(badge, Style::default().fg(color).bold()));
    }
    if spans.len() == 1 {
        spans.push(Span::styled(
            "FOOTHOLD FOUNDATION // NO MODULES",
            Style::default().fg(theme.muted),
        ));
    }
    Line::from(spans)
}

fn region_vista_caption(
    region: &mind_core::protocol::RegionSnapshot,
    planet: &PlanetSnapshot,
    compact: bool,
    ascii: bool,
    theme: Theme,
) -> Line<'static> {
    let sep = if ascii { " / " } else { " · " };
    if region.knowledge == mind_core::protocol::RegionKnowledge::Unknown || region.terrain.is_none()
    {
        return Line::from(vec![
            Span::styled(
                "UNRESOLVED HORIZON",
                Style::default().fg(theme.amber).bold(),
            ),
            Span::styled(
                format!("{sep}survey required"),
                Style::default().fg(theme.muted),
            ),
        ]);
    }
    let terrain = format!("{:?}", region.terrain.unwrap()).to_uppercase();
    let temp = region
        .temperature
        .map_or("?".into(), |t| format!("{t:?}").to_uppercase());
    let atmo = region
        .atmospheric_severity
        .map_or("?".into(), |a| format!("{a:?}").to_uppercase());
    let lead = if compact {
        terrain
    } else {
        format!("{terrain}{sep}{temp}{sep}{atmo}")
    };
    let mut spans = vec![Span::styled(lead, Style::default().fg(theme.cyan_bright))];
    if region.core {
        spans.push(Span::styled(
            format!("{sep}CORE COMPLEX ONLINE"),
            Style::default().fg(theme.gold).bold(),
        ));
    } else if let Some(base) = planet
        .network
        .bases
        .iter()
        .find(|b| b.region_id == region.id)
    {
        let (state, color) = if base.connected && base.integrity > 20 {
            ("OUTPOST ONLINE", theme.success)
        } else if !base.connected {
            ("OUTPOST ISOLATED", theme.warning)
        } else {
            ("OUTPOST FAILING", theme.warning)
        };
        spans.push(Span::styled(
            format!("{sep}{state}"),
            Style::default().fg(color).bold(),
        ));
    } else if region.relay {
        spans.push(Span::styled(
            format!("{sep}RELAY ACTIVE"),
            Style::default().fg(theme.success).bold(),
        ));
    } else if let Some(deposit) = &region.deposit {
        if deposit.remaining > 0 {
            spans.push(Span::styled(
                format!("{sep}{:?} {}", deposit.resource, deposit.remaining).to_uppercase(),
                Style::default().fg(theme.amber).bold(),
            ));
        }
    }
    Line::from(spans)
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
            format!(
                "{} {}",
                region.hop_distance_from_core,
                if region.hop_distance_from_core == 1 {
                    "hop"
                } else {
                    "hops"
                }
            ),
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
    app: &App,
    planet: &PlanetSnapshot,
    region: &mind_core::protocol::RegionSnapshot,
    ascii: bool,
    theme: Theme,
    now: Instant,
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
        let remaining_ms = if mission.phase == ScoutState::Holding {
            mission.remaining_ms
        } else {
            app.display_remaining_ms(mission.remaining_ms, now)
        };
        let pct = operation_progress(remaining_ms, mission.phase_total_ms);
        lines.push(field_line(
            "PROGRESS",
            format!(
                "{pct}% // {}",
                if mission.phase == ScoutState::Holding {
                    "holding on station".to_owned()
                } else {
                    format!("{}s remaining", remaining_ms.div_ceil(1_000))
                }
            ),
            theme,
        ));
        if matches!(mission.phase, ScoutState::Outbound | ScoutState::Returning) {
            lines.push(art::operation_telemetry_strip(
                "CORE",
                &region.designation,
                pct,
                mission.phase == ScoutState::Outbound,
                ascii,
                theme,
            ));
        }
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
        let remaining_ms = app.display_remaining_ms(mission.remaining_ms, now);
        let pct = operation_progress(remaining_ms, mission.phase_total_ms);
        lines.push(field_line(
            "PROGRESS",
            format!("{pct}% // {}s remaining", remaining_ms.div_ceil(1_000)),
            theme,
        ));
        if matches!(
            mission.phase,
            ExtractorState::Outbound | ExtractorState::Returning
        ) {
            lines.push(art::operation_telemetry_strip(
                "CORE",
                &region.designation,
                pct,
                mission.phase == ExtractorState::Outbound,
                ascii,
                theme,
            ));
        }
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
        lines.push(field_line("RIG", mission.loadout.title().into(), theme));
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
        let remaining_ms = app.display_remaining_ms(construction.remaining_ms, now);
        lines.push(Line::styled(
            "RANGE RELAY // CONSTRUCTING",
            Style::default().fg(theme.cyan_bright).bold(),
        ));
        lines.push(field_line(
            "PROGRESS",
            format!(
                "{}% // {}s remaining",
                operation_progress(remaining_ms, construction.phase_total_ms),
                remaining_ms.div_ceil(1_000)
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
            if planet.survey_queue.unlocked {
                "QUEUE"
            } else {
                "WAYPOINT"
            },
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
            ScoutState::Outbound => "inbound from core",
            ScoutState::Surveying => "surveying locally",
            ScoutState::Holding => "holding locally",
            ScoutState::Returning => "returning to core",
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
            mind_core::protocol::ExtractorState::Outbound => "inbound from core",
            mind_core::protocol::ExtractorState::Extracting => "extracting locally",
            mind_core::protocol::ExtractorState::Returning => "returning to core",
            mind_core::protocol::ExtractorState::Ready => "ready",
            mind_core::protocol::ExtractorState::Rebuilding => "rebuilding",
        };
        format!(
            "{} // {relation} // {}",
            planet.extractor.designation,
            loadout_code(mission.loadout)
        )
    } else if region.core && planet.active_extraction.is_none() {
        match planet.extractor.state {
            mind_core::protocol::ExtractorState::Ready => {
                format!(
                    "{} // stationed // {}",
                    planet.extractor.designation,
                    loadout_code(planet.extractor.loadout)
                )
            }
            mind_core::protocol::ExtractorState::Rebuilding => format!(
                "{} // rebuilding // {}s // {}",
                planet.extractor.designation,
                planet.extractor.rebuild_remaining_ms.div_ceil(1_000),
                loadout_code(planet.extractor.loadout)
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
        Some(ContextAction::QueueSurvey(_)) if !planet.survey_queue.unlocked => {
            "queue waypoint (next)"
        }
        Some(ContextAction::QueueSurvey(_)) => "queue survey",
        Some(ContextAction::RemoveQueuedSurvey(_)) if !planet.survey_queue.unlocked => {
            "clear waypoint"
        }
        Some(ContextAction::RemoveQueuedSurvey(_)) => "remove queued survey",
        Some(ContextAction::SetExtractionRoute(_))
            if planet.extraction_route.region_id.as_deref() == Some(region.id.as_str()) =>
        {
            "resume route"
        }
        Some(ContextAction::SetExtractionRoute(_)) => "activate route",
        Some(ContextAction::ClearExtractionRoute) => "clear route",
        _ if !planet.survey_queue.unlocked => "waypoint / auto",
        _ => "automation",
    };
    let automation_identity = automation
        .clone()
        .unwrap_or_else(|| ContextAction::QueueSurvey(region.id.clone()));

    struct ActionEntry {
        key: &'static str,
        label: String,
        action: ContextAction,
        available: Option<ContextAction>,
        allow_primary: bool,
        show_when_blocked: bool,
        block_reason: String,
    }
    let mut ready_lines = Vec::new();
    let mut blocked_lines = Vec::new();
    let region_surveyed = region.knowledge >= mind_core::protocol::RegionKnowledge::Surveyed;
    let rig_relevant = region.core
        || region.deposit.is_some()
        || planet
            .active_extraction
            .as_ref()
            .is_some_and(|m| m.target_region_id == region.id);
    let entries = vec![
        ActionEntry {
            key: "S",
            label: "survey".into(),
            action: ContextAction::Deploy(region.id.clone()),
            available: app.scout_action(),
            allow_primary: true,
            show_when_blocked: !region.core && !region_surveyed,
            block_reason: survey_block_reason(planet, region),
        },
        ActionEntry {
            key: "X",
            label: "extract".into(),
            action: ContextAction::Extract(region.id.clone()),
            available: app.extraction_action(),
            allow_primary: true,
            show_when_blocked: region.deposit.is_some(),
            block_reason: extraction_block_reason(planet, region),
        },
        ActionEntry {
            key: "B",
            label: "build relay".into(),
            action: ContextAction::ConstructRelay(region.id.clone()),
            available: app.construction_action(),
            allow_primary: true,
            show_when_blocked: !region.core && region_surveyed && planet.relay_region_id.is_none(),
            block_reason: construction_block_reason(planet, region),
        },
        ActionEntry {
            key: "R",
            label: "recall scout".into(),
            action: ContextAction::Recall,
            available: app.scout_recall_action(),
            allow_primary: true,
            show_when_blocked: planet.scout.state == mind_core::protocol::ScoutState::Holding
                || planet.active_mission.is_some(),
            block_reason: recall_block_reason(planet, region),
        },
        ActionEntry {
            key: "A",
            label: automation_label.into(),
            action: automation_identity,
            available: automation,
            allow_primary: false,
            show_when_blocked: planet.survey_queue.unlocked
                || planet.active_mission.is_some()
                || (planet.extraction_route.unlocked && region.deposit.is_some()),
            block_reason: automation_block_reason(planet, region),
        },
        ActionEntry {
            key: "G",
            label: format!("select {}", planet.extractor.loadout.next().title()),
            action: ContextAction::ConfigureExtractor(planet.extractor.loadout.next()),
            available: app.extractor_loadout_action(),
            allow_primary: false,
            show_when_blocked: rig_relevant,
            block_reason: extractor_loadout_blocker(planet),
        },
    ];

    for entry in entries {
        let is_available = entry.available.as_ref() == Some(&entry.action);
        if !is_available && !entry.show_when_blocked {
            continue;
        }
        let line = region_action_line(
            entry.key,
            entry.label,
            entry.action,
            entry.available,
            if entry.allow_primary {
                primary.as_ref()
            } else {
                None
            },
            entry.block_reason,
            theme,
        );
        if is_available {
            ready_lines.push(line);
        } else {
            blocked_lines.push(line);
        }
    }

    let local_field_available = planet
        .network
        .subregions
        .iter()
        .any(|site| site.region_id == region.id);
    let local_field_status = if !local_field_available {
        crate::app::LOCAL_FIELD_SURVEY_GUIDANCE
    } else if planet.network.unlocked {
        "open 4x4 local field"
    } else {
        "open 4x4 local field (focused survey)"
    };
    let local_field_line = Line::from(vec![
        Span::styled(
            "[V] ",
            Style::default()
                .fg(if local_field_available {
                    theme.amber
                } else {
                    theme.faint
                })
                .bold(),
        ),
        Span::styled(
            "local field",
            Style::default()
                .fg(if local_field_available {
                    theme.text_strong
                } else {
                    theme.muted
                })
                .add_modifier(if local_field_available {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ),
        Span::styled(" // ", Style::default().fg(theme.faint)),
        Span::styled(
            local_field_status,
            Style::default().fg(if local_field_available {
                theme.success
            } else {
                theme.muted
            }),
        ),
    ]);
    if local_field_available {
        ready_lines.push(local_field_line);
    } else if !region.core {
        blocked_lines.push(local_field_line);
    }

    let mut lines = ready_lines;
    if !blocked_lines.is_empty() {
        lines.push(Line::styled(
            "UNAVAILABLE HERE",
            Style::default().fg(theme.faint),
        ));
        lines.extend(blocked_lines);
    }
    lines
}

fn automation_block_reason(
    planet: &PlanetSnapshot,
    region: &mind_core::protocol::RegionSnapshot,
) -> String {
    if !region.core
        && region.knowledge < mind_core::protocol::RegionKnowledge::Surveyed
        && !region.within_scout_range
    {
        return "region is beyond current network reach".into();
    }
    if region.knowledge < mind_core::protocol::RegionKnowledge::Surveyed {
        if planet.survey_queue.queued_region_ids.len() >= planet.survey_queue.capacity {
            return if planet.survey_queue.unlocked {
                format!(
                    "survey queue full ({}/{})",
                    planet.survey_queue.queued_region_ids.len(),
                    planet.survey_queue.capacity
                )
            } else {
                "waypoint slot full (1/1) // research unlocks 8-slot queue".into()
            };
        }
        return "survey is already active or queued".into();
    }
    if !planet.extraction_route.unlocked {
        return "complete research to earn routine extraction routes".into();
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
    label: impl Into<String>,
    action: ContextAction,
    available: Option<ContextAction>,
    primary: Option<&ContextAction>,
    block_reason: String,
    theme: Theme,
) -> Line<'static> {
    let label = label.into();
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

fn loadout_code(loadout: ExtractorLoadout) -> &'static str {
    match loadout {
        ExtractorLoadout::Balanced => "BALANCED",
        ExtractorLoadout::Cargo => "CARGO",
        ExtractorLoadout::Shielded => "SHIELDED",
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

fn hint_width(hint: &KeyHint<'_>) -> usize {
    let key_w = hint.key.chars().count() + 2;
    if hint.label.is_empty() {
        key_w + 1
    } else {
        key_w + 1 + hint.label.chars().count() + 2
    }
}

/// Fit whole key hints into `max_width` without ever clipping a hint mid-word.
/// Always preserves the trailing menu/help controls when space is tight.
fn fit_hint_spans(controls: &[KeyHint<'_>], max_width: usize, theme: Theme) -> Vec<Span<'static>> {
    let total: usize = controls.iter().map(hint_width).sum();
    if total <= max_width {
        return hint_spans(controls, theme);
    }
    // Keep trailing Q/? controls when present, then greedily fill from the front.
    let tail_count = controls
        .iter()
        .rev()
        .take_while(|h| matches!(h.key, "Q" | "?"))
        .count();
    let split = controls.len().saturating_sub(tail_count);
    let tail = &controls[split..];
    let tail_w: usize = tail.iter().map(hint_width).sum();
    let mut chosen: Vec<KeyHint<'_>> = Vec::with_capacity(controls.len());
    let mut used = tail_w;
    for hint in &controls[..split] {
        let w = hint_width(hint);
        if used + w <= max_width {
            chosen.push(*hint);
            used += w;
        }
    }
    if tail_w <= max_width {
        chosen.extend_from_slice(tail);
    }
    hint_spans(&chosen, theme)
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
    let max_w = usize::from(area.width).saturating_sub(2);
    let badge = if matches!(app.connection, ConnectionState::Connected) {
        app.next_event_badge(now)
    } else {
        None
    };
    // Fit the status row without clipping: a long notice first displaces the NEXT badge, then is
    // shortened at a word boundary with an ellipsis.
    let spans_width =
        |spans: &[Span<'_>]| -> usize { spans.iter().map(|s| s.content.chars().count()).sum() };
    let sep_width = spans_width(&[separator(theme)]);
    let base_width = spans_width(&spans);
    let notice_width = app
        .notice
        .as_ref()
        .map_or(0, |notice| notice.chars().count() + sep_width);
    let badge_width = badge
        .as_ref()
        .map_or(0, |(text, _)| text.chars().count() + sep_width);
    let status_budget = if area.width < 140 {
        max_w
    } else {
        max_w.saturating_sub(max_w / 2)
    };
    let keep_badge =
        app.notice.is_none() || base_width + notice_width + badge_width <= status_budget;
    if let (true, Some((badge, highlight))) = (keep_badge, badge) {
        spans.extend([
            separator(theme),
            Span::styled(
                badge,
                Style::default()
                    .fg(if highlight {
                        theme.gold
                    } else {
                        theme.cyan_bright
                    })
                    .bold(),
            ),
        ]);
    }
    if let Some(notice) = &app.notice {
        let room = status_budget.saturating_sub(spans_width(&spans) + sep_width);
        spans.extend([
            separator(theme),
            Span::styled(
                truncate_chars(notice, room.max(12), true),
                Style::default().fg(theme.text),
            ),
        ]);
    }
    let content = if area.width < 140 {
        Text::from(vec![
            Line::from(spans),
            Line::from(fit_hint_spans(controls, max_w, theme)),
        ])
    } else {
        let used: usize = spans
            .iter()
            .map(|s| s.content.chars().count())
            .sum::<usize>()
            + 4;
        spans.push(separator(theme));
        spans.extend(fit_hint_spans(controls, max_w.saturating_sub(used), theme));
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
    let confirming = !matches!(app.onboarding_view, OnboardingView::Adjusting);
    let tier = app.color_tier(options);

    // 1. Full-bleed harmonic phase canvas across the entire viewport.
    let chunks = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(10),
        Constraint::Length(1),
    ])
    .split(area);

    // Docked HUD/card heights: borders plus content rows (notice adds one), leaving no empty rows.
    let adjusting_hud_h = (6 + u16::from(app.notice.is_some())).min(chunks[1].height);
    let confirm_card_h = 9.min(chunks[1].height);
    let reserved_rows = if confirming {
        confirm_card_h + 1
    } else {
        adjusting_hud_h + 1
    };
    let canvas_buf = cinematic::build_coherence_buffer(
        chunks[1].width,
        chunks[1].height,
        app.band_positions,
        app.selected_band,
        app.globe_phase,
        confirming,
        reserved_rows,
    );
    crate::raster::blit_to_frame(&canvas_buf, frame, chunks[1], tier);

    // 2. Clean top bar with hairline rule.
    let rule = if options.ascii { "-" } else { "─" };
    let title = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(
                if options.ascii { "# " } else { "◆ " },
                Style::default().fg(theme.gold).bold(),
            ),
            Span::styled("SUBSTRATE", Style::default().fg(theme.text_strong).bold()),
            Span::styled(" // COHERENCE", Style::default().fg(theme.cyan_bright)),
        ]),
        Line::styled(
            rule.repeat(usize::from(area.width)),
            Style::default().fg(theme.border),
        ),
    ])
    .alignment(Alignment::Center)
    .style(theme.panel_style());
    frame.render_widget(title, chunks[0]);

    // 3. Sleek floating HUD docked in the lower portion of the full-bleed canvas.
    match app.onboarding_view {
        OnboardingView::Adjusting => {
            let hud_h = adjusting_hud_h;
            let hud_w = chunks[1].width.saturating_sub(4).clamp(42, 76);
            let hud_y = chunks[1].y + chunks[1].height.saturating_sub(hud_h + 1);
            let hud_x = chunks[1].x + chunks[1].width.saturating_sub(hud_w) / 2;
            let hud_area = Rect::new(hud_x, hud_y, hud_w, hud_h);
            let block = panel(" COHERENCE ", options, theme, PanelTone::Focused);
            let inner = block.inner(hud_area);
            frame.render_widget(Clear, hud_area);
            frame.render_widget(block, hud_area);

            let band_bar = |idx: usize, pos: u8| -> Span<'static> {
                let active = idx == app.selected_band;
                let marker = if options.ascii {
                    if active {
                        ">"
                    } else {
                        " "
                    }
                } else if active {
                    "▸"
                } else {
                    " "
                };
                Span::styled(
                    format!("{marker}B{} [{:02}/12]  ", idx + 1, pos + 1),
                    Style::default()
                        .fg(if active {
                            theme.gold
                        } else if app.aligned() {
                            theme.cyan_bright
                        } else {
                            theme.muted
                        })
                        .add_modifier(if active {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                )
            };

            let mut hud_lines = vec![
                Line::styled(
                    if app.aligned() {
                        "Three harmonic bands lock in phase."
                    } else {
                        "The signals repeat, but do not agree."
                    },
                    Style::default()
                        .fg(if app.aligned() {
                            theme.gold
                        } else {
                            theme.text_strong
                        })
                        .bold(),
                ),
                Line::from(vec![
                    band_bar(0, app.band_positions[0]),
                    band_bar(1, app.band_positions[1]),
                    band_bar(2, app.band_positions[2]),
                ]),
                art::coherence_oscilloscope(
                    app.band_positions,
                    app.globe_phase,
                    options.ascii,
                    theme,
                ),
                Line::from(hint_spans(
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
                )),
            ];
            if let Some(notice) = &app.notice {
                hud_lines.push(Line::styled(
                    notice.as_str(),
                    Style::default().fg(theme.warning).bold(),
                ));
            }
            frame.render_widget(
                Paragraph::new(hud_lines)
                    .alignment(Alignment::Center)
                    .style(theme.panel_style()),
                inner,
            );
        }
        OnboardingView::Confirming | OnboardingView::Submitting => {
            let submitting = app.onboarding_view == OnboardingView::Submitting;
            let connected = matches!(app.connection, ConnectionState::Connected);
            let card_w = chunks[1].width.saturating_sub(4).clamp(42, 68);
            let card_h = confirm_card_h;
            let card_y = chunks[1].y + chunks[1].height.saturating_sub(card_h + 1);
            let card_x = chunks[1].x + chunks[1].width.saturating_sub(card_w) / 2;
            let card_area = Rect::new(card_x, card_y, card_w, card_h);
            let block = panel(" INTERVAL LOCKED ", options, theme, PanelTone::Modal);
            let inner = block.inner(card_area);
            frame.render_widget(Clear, card_area);
            frame.render_widget(block, card_area);

            let lines = vec![
                Line::styled(
                    "The interval holds.",
                    Style::default().fg(theme.gold).bold(),
                ),
                Line::raw(""),
                Line::styled(
                    "It is easier to observe when the pattern returns.",
                    Style::default().fg(theme.text_strong),
                ),
                Line::raw(""),
                Line::styled(
                    if submitting {
                        "Submitting..."
                    } else if !connected {
                        "Connection required // r reconnect // Esc reconsider"
                    } else {
                        "Esc Reconsider     Enter Awaken"
                    },
                    Style::default()
                        .fg(if connected {
                            theme.cyan_bright
                        } else {
                            theme.warning
                        })
                        .bold(),
                ),
            ];
            frame.render_widget(
                Paragraph::new(Text::from(lines))
                    .alignment(Alignment::Center)
                    .wrap(Wrap { trim: true })
                    .style(theme.raised_style()),
                inner,
            );
        }
    }

    let footer = Paragraph::new(connection_line(app, theme, area.width < 72))
        .alignment(Alignment::Center)
        .style(theme.panel_style());
    frame.render_widget(footer, chunks[2]);
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
        // Six telemetry rows plus borders.
        let rows = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(8),
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
            Constraint::Length(8),
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
                field_line(
                    "TUI DRAW",
                    format!(
                        "{:.2}ms // {} frames",
                        app.last_frame_duration_us as f64 / 1_000.0,
                        app.frame_count
                    ),
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
                EngineEventKind::ExtractorConfigured => "RIG",
                EngineEventKind::ExtractionCompleted => "CARGO",
                EngineEventKind::ExtractorLost => "LOSS",
                EngineEventKind::ExtractorRebuilt => "REBUILD",
                EngineEventKind::RelayConstructionStarted => "BUILD",
                EngineEventKind::RelayActivated => "RELAY",
                EngineEventKind::AlphaMilestoneCompleted => "ALPHA",
                EngineEventKind::FoundationCompleted => "FOUND",
                EngineEventKind::SubregionSurveyed => "LOCAL",
                EngineEventKind::BaseProjectStarted => "BUILD",
                EngineEventKind::BaseEstablished => "NODE",
                EngineEventKind::BaseModuleCompleted => "MODULE",
                EngineEventKind::BaseModuleModeChanged => "POWER",
                EngineEventKind::BaseMaintained => "REPAIR",
                EngineEventKind::BaseStrained => "WEAR",
                EngineEventKind::SolarHorizonReached => "MAP",
                EngineEventKind::ResearchStarted => "R&D",
                EngineEventKind::ResearchCompleted => "R&D+",
                EngineEventKind::ProfileActivated => "PROFILE",
                EngineEventKind::SurveyQueued => "QUEUE",
                EngineEventKind::ExtractionRouteSet => "ROUTE",
                EngineEventKind::AutomationPaused => "PAUSE",
                EngineEventKind::ActivityReviewed => "REVIEW",
                EngineEventKind::ArchiveExperimentStarted => "ARC+",
                EngineEventKind::ArchiveExperimentCompleted => "ARC✓",
                EngineEventKind::ArchiveObservationRecorded => "EVID",
                EngineEventKind::ArchivePresentationReviewed => "EVID",
                EngineEventKind::PersistenceDegraded => "SAVE!",
                EngineEventKind::PersistenceRecovered => "SAVE",
                EngineEventKind::ShutdownRequested => "STOP",
            };
            let tag_color = match event.kind {
                EngineEventKind::PersistenceDegraded | EngineEventKind::ExtractorLost => {
                    theme.danger
                }
                EngineEventKind::ShutdownRequested
                | EngineEventKind::SaveReset
                | EngineEventKind::BaseStrained => theme.warning,
                EngineEventKind::DiagnosticPulse
                | EngineEventKind::OnboardingCompleted
                | EngineEventKind::SurveyCompleted
                | EngineEventKind::ExtractionCompleted
                | EngineEventKind::ExtractorConfigured
                | EngineEventKind::ExtractorRebuilt
                | EngineEventKind::RelayActivated
                | EngineEventKind::AlphaMilestoneCompleted
                | EngineEventKind::FoundationCompleted
                | EngineEventKind::SubregionSurveyed
                | EngineEventKind::BaseEstablished
                | EngineEventKind::BaseModuleCompleted
                | EngineEventKind::BaseMaintained
                | EngineEventKind::SolarHorizonReached
                | EngineEventKind::ResearchCompleted
                | EngineEventKind::PersistenceRecovered => theme.success,
                _ => theme.cyan,
            };
            Line::from(vec![
                Span::styled(
                    format!("{:>6} ", event.tick),
                    Style::default().fg(theme.faint),
                ),
                Span::styled(format!("{tag:>7}"), Style::default().fg(tag_color).bold()),
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
                label: "pulse",
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
                label: "view",
            },
            KeyHint {
                key: "?",
                label: "help",
            },
            KeyHint {
                key: "Q",
                label: "menu",
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
                label: "menu",
            },
        ]
    };
    let content = if footer_area.width < 100 {
        Text::from(vec![
            Line::from(footer),
            Line::from(fit_hint_spans(
                hints,
                usize::from(footer_area.width).saturating_sub(2),
                theme,
            )),
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
                label: "menu",
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
                label: "menu",
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

fn render_pilot_strip(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    options: RenderOptions,
    theme: Theme,
    now: Instant,
) {
    let Some(planet) = &app.planet else {
        return;
    };
    let surveyed = planet
        .regions
        .iter()
        .filter(|r| r.knowledge >= mind_core::protocol::RegionKnowledge::Surveyed)
        .count();
    let sep = if options.ascii { " | " } else { " · " };
    let dot = if options.ascii { "*" } else { "◆" };
    let line1 = Line::from(vec![
        Span::styled(
            format!(" {dot} SUBSTRATE"),
            Style::default().fg(theme.gold).bold(),
        ),
        Span::styled(sep, Style::default().fg(theme.faint)),
        Span::styled(
            format!(
                "CH {}",
                transmission::roman_chapter(planet.campaign.chapter_number)
            ),
            Style::default().fg(theme.amber).bold(),
        ),
        Span::styled(sep, Style::default().fg(theme.faint)),
        Span::styled(
            format!("MAP {surveyed}/{}", planet.regions.len()),
            Style::default().fg(theme.cyan_bright),
        ),
    ]);

    let status_line = if let Some((badge, highlight)) = app.next_event_badge(now) {
        let (glyph, glyph_color) = if !planet.archive.unpresented_record_ids.is_empty() {
            (if options.ascii { " ! " } else { " ◈ " }, theme.gold)
        } else if highlight {
            (if options.ascii { " * " } else { " ● " }, theme.success)
        } else {
            (if options.ascii { " > " } else { " ▶ " }, theme.cyan_bright)
        };
        let concise = badge.strip_prefix("NEXT: ").unwrap_or(&badge);
        let clipped = truncate_chars(
            concise,
            usize::from(area.width).saturating_sub(4),
            options.ascii,
        );
        Line::from(vec![
            Span::styled(glyph, Style::default().fg(glyph_color).bold()),
            Span::styled(
                clipped,
                Style::default()
                    .fg(if highlight {
                        theme.gold
                    } else {
                        theme.text_strong
                    })
                    .bold(),
            ),
        ])
    } else {
        Line::from(vec![
            Span::styled(
                if options.ascii { " * " } else { " ● " },
                Style::default().fg(theme.success).bold(),
            ),
            Span::styled("READY FOR ORDERS", Style::default().fg(theme.text_strong)),
        ])
    };

    let next_action = terminal_text(&planet.campaign.next_action, options.ascii);
    let mut lines = vec![line1, status_line];
    let width = usize::from(area.width);
    let rows_left = usize::from(area.height).saturating_sub(2);
    if let Some(confirm) = pilot_confirmation(app, planet) {
        // Never accept Enter blind: show what is being confirmed, its target and risk.
        let text = terminal_text(&confirm, options.ascii);
        let mut confirm_lines = wrap_indented_lines(
            vec![Span::styled(
                " CONFIRM ",
                Style::default().fg(theme.warning).bold(),
            )],
            9,
            &text,
            9,
            width.max(20),
            Style::default().fg(theme.text_strong),
        );
        let keys = Line::from(vec![
            Span::styled(" ENTER ", theme.key_style()),
            Span::styled(" ok ", Style::default().fg(theme.muted)),
            Span::styled(" ESC ", theme.key_style()),
            Span::styled(" cancel", Style::default().fg(theme.muted)),
        ]);
        if rows_left >= 2 {
            confirm_lines.truncate(rows_left - 1);
            lines.extend(confirm_lines);
            lines.push(keys);
        } else {
            lines.push(Line::from(vec![
                Span::styled(" CONFIRM ", Style::default().fg(theme.warning).bold()),
                Span::styled(
                    truncate_chars(&text, width.saturating_sub(24), options.ascii),
                    Style::default().fg(theme.text_strong),
                ),
                Span::styled(" ENTER/ESC", Style::default().fg(theme.muted)),
            ]));
        }
    } else if area.height >= 4 {
        let mut next_lines = wrap_indented_lines(
            vec![Span::styled(
                " NEXT ",
                Style::default().fg(theme.amber).bold(),
            )],
            6,
            &next_action,
            6,
            width.max(20),
            Style::default().fg(theme.muted),
        );
        if next_lines.len() > rows_left && rows_left > 0 {
            // Cap to the strip and end the last visible row with an ellipsis at a word boundary.
            let plain = |line: &Line<'_>| -> String {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            };
            let tail: Vec<String> = next_lines[rows_left - 1..]
                .iter()
                .map(|line| {
                    plain(line)
                        .trim()
                        .trim_start_matches("NEXT")
                        .trim()
                        .to_owned()
                })
                .collect();
            let joined = tail.join(" ");
            let first_row = rows_left == 1;
            next_lines.truncate(rows_left - 1);
            let budget = width.saturating_sub(6);
            let mut spans = vec![Span::styled(
                if first_row { " NEXT " } else { "      " },
                Style::default().fg(theme.amber).bold(),
            )];
            spans.push(Span::styled(
                truncate_chars(&joined, budget, options.ascii),
                Style::default().fg(theme.muted),
            ));
            next_lines.push(Line::from(spans));
        }
        lines.extend(next_lines);
    } else {
        lines.push(Line::from(vec![
            Span::styled(" NEXT ", Style::default().fg(theme.amber).bold()),
            Span::styled(
                truncate_chars(
                    &next_action,
                    usize::from(area.width).saturating_sub(7),
                    options.ascii,
                ),
                Style::default().fg(theme.muted),
            ),
        ]));
    }

    frame.render_widget(Paragraph::new(lines).style(theme.panel_style()), area);
}

/// One-line description of a pending confirmation for the Pilot Strip (action, target, risk).
fn pilot_confirmation(app: &App, planet: &PlanetSnapshot) -> Option<String> {
    let action = app.pending_action.as_ref()?;
    let rig = planet.extractor.loadout.title();
    Some(match action {
        ContextAction::Extract(region_id) => {
            let risk = planet
                .regions
                .iter()
                .find(|region| &region.id == region_id)
                .and_then(|region| region.deposit.as_ref())
                .map_or("UNKNOWN", |deposit| risk_text(deposit.risk));
            format!(
                "extract {} // {risk} loss risk // {rig}",
                region_designation(planet, region_id)
            )
        }
        ContextAction::ExtractSubregion { subregion_id, .. } => {
            let site = planet
                .network
                .subregions
                .iter()
                .find(|site| &site.id == subregion_id);
            let risk = site
                .and_then(|site| site.deposit.as_ref())
                .map_or("UNKNOWN", |deposit| risk_text(deposit.risk));
            format!(
                "extract {} // {risk} loss risk // {rig}",
                site.map_or(subregion_id.as_str(), |site| site.designation.as_str())
            )
        }
        ContextAction::ConstructRelay(region_id) => {
            format!(
                "relay at {} // S8 C6 Si6",
                region_designation(planet, region_id)
            )
        }
        ContextAction::EstablishBase(_) => "foothold // S10 C6 Si4".to_owned(),
        ContextAction::MaintainBase(_) => "maintenance // S4 C2".to_owned(),
        ContextAction::ConstructBaseModule { module, .. } => {
            let cost = module.cost();
            format!(
                "{} // S{} C{} Si{}",
                module_title(*module),
                cost.structural,
                cost.conductive,
                cost.silicate
            )
        }
        _ => "selected operation".to_owned(),
    })
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

fn render_help(
    frame: &mut Frame<'_>,
    area: Rect,
    scroll: u16,
    app_help_limit: &std::cell::Cell<u16>,
    options: RenderOptions,
    theme: Theme,
) {
    let two_col = area.width >= 104 && area.height >= 24;
    let width = if two_col {
        100.min(area.width.saturating_sub(4))
    } else {
        64.min(area.width.saturating_sub(4))
    };
    // Leave the two-row footer and header clear so no status fragments sit beside the border.
    let height = if two_col {
        21.min(area.height.saturating_sub(4))
    } else {
        22.min(area.height.saturating_sub(4))
    };
    let help_area = centered_rect(width, height, area);
    frame.render_widget(Clear, help_area);

    let left_col = vec![
        section_line("GLOBAL", theme),
        key_line("Q / CTRL-C", "session menu / detach", theme),
        key_line("TAB / 1-7", "cycle or jump to workspace", theme),
        key_line("N", "focus campaign lead", theme),
        key_line("C", "chapter briefing (globe view)", theme),
        key_line("R", "reconnect to the daemon", theme),
        key_line("? / ESC", "close this help", theme),
        Line::from(""),
        section_line("OVERVIEW / PROBES / RESEARCH", theme),
        key_line("ARROWS", "select entry or probe", theme),
        key_line("ENTER", "jump / commit / activate", theme),
        key_line("G", "cycle extractor rig", theme),
        key_line("PGUP / PGDN", "scroll details panel", theme),
        Line::from(""),
        section_line("ARCHIVE", theme),
        key_line("UP / DOWN", "select observation", theme),
        key_line("V / ENTER", "replay / run comparison", theme),
    ];
    let right_col = vec![
        section_line("PLANET GLOBE", theme),
        key_line("ARROWS / SHIFT", "move / jump 5 sectors", theme),
        key_line("ENTER", "run primary region action", theme),
        key_line("S / R", "deploy / recall scout", theme),
        key_line("X / B", "extract / build relay", theme),
        key_line("A", "queue waypoint or route", theme),
        key_line("V", "open 4x4 local field", theme),
        Line::from(""),
        section_line("LOCAL FIELD (4x4)", theme),
        key_line("ARROWS / ESC", "select sector / globe", theme),
        key_line("S / X / B", "survey / extract / node", theme),
        key_line("P / C / D", "power / comms / depot", theme),
        key_line("W / L / O / M", "workshop / lab / obs / maint", theme),
        Line::from(""),
        Line::styled(
            "Extraction can lose probe and cargo.",
            Style::default().fg(theme.warning),
        ),
        Line::styled(
            "Core always rebuilds; progress stays.",
            Style::default().fg(theme.muted),
        ),
    ];

    let inner_preview = Rect::new(0, 0, width.saturating_sub(2), height.saturating_sub(2));
    let single_col_rows = if two_col {
        Vec::new()
    } else {
        let mut combined = left_col.clone();
        combined.push(Line::from(""));
        combined.extend(right_col.clone());
        crate::textwrap::hang_wrap(combined, usize::from(inner_preview.width))
    };
    let max_scroll = (single_col_rows.len() as u16).saturating_sub(inner_preview.height);
    app_help_limit.set(max_scroll);
    let clamped = scroll.min(max_scroll);
    let cue = match (clamped > 0, clamped < max_scroll, options.ascii) {
        (_, _, _) if two_col => "",
        (false, true, true) => " // MORE v",
        (false, true, false) => " // MORE ▼",
        (true, false, true) => " // ^",
        (true, false, false) => " // ▲",
        (true, true, true) => " // ^ v",
        (true, true, false) => " // ▲▼",
        (false, false, _) => "",
    };
    let title = if two_col {
        " HELP // ? OR ESC TO CLOSE ".to_owned()
    } else if options.ascii {
        format!(" HELP // UP/DN SCROLL // ESC CLOSE{cue} ")
    } else {
        format!(" HELP // ↑↓ SCROLL // ESC CLOSE{cue} ")
    };
    let block = panel(&title, options, theme, PanelTone::Modal);
    let inner = block.inner(help_area);
    frame.render_widget(block, help_area);
    frame.render_widget(Block::default().style(theme.raised_style()), inner);

    if two_col {
        let cols = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(inner);
        frame.render_widget(
            Paragraph::new(left_col)
                .wrap(Wrap { trim: false })
                .style(theme.raised_style()),
            cols[0],
        );
        frame.render_widget(
            Paragraph::new(right_col)
                .wrap(Wrap { trim: false })
                .style(theme.raised_style()),
            cols[1],
        );
    } else {
        frame.render_widget(
            Paragraph::new(single_col_rows)
                .scroll((clamped, 0))
                .style(theme.raised_style()),
            inner,
        );
    }
}

fn render_session_menu_backdrop(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    options: RenderOptions,
    theme: Theme,
) {
    let tier = app.color_tier(options);
    if matches!(
        tier,
        crate::raster::ColorTier::TrueColor
            | crate::raster::ColorTier::Ansi256
            | crate::raster::ColorTier::Ansi16
    ) {
        let mut buf = crate::raster::PixelBuffer::new(
            usize::from(area.width),
            usize::from(area.height) * 2,
            crate::raster::Rgb(0, 18, 26),
        );
        buf.fill_gradient_stops(
            &[
                (0.0, crate::raster::Rgb(0, 12, 20)),
                (0.65, crate::raster::Rgb(2, 24, 34)),
                (1.0, crate::raster::Rgb(6, 38, 50)),
            ],
            true,
        );
        let title_excl = if area.height >= 6 {
            let mask_w = 50.min(usize::from(area.width));
            let mask_x = usize::from(area.width).saturating_sub(mask_w) / 2;
            Some((mask_x, 0, mask_x + mask_w, 8))
        } else {
            None
        };
        buf.draw_starfield_bounded(
            42,
            (usize::from(area.width) / 2).max(24),
            2,
            usize::from(area.height),
            title_excl,
            0.0,
            crate::raster::Rgb(46, 86, 102),
            crate::raster::Rgb(160, 214, 224),
        );
        // Subtle planetary limb curvature rising at the lower edge with crisp cyan halo.
        let cx = buf.width as f32 * 0.5;
        let r = (buf.width as f32).max(buf.height as f32) * 1.15;
        let cy = buf.height as f32 + r * 0.84;
        buf.add_radial_glow(
            cx,
            buf.height as f32 * 0.94,
            buf.width as f32 * 0.62,
            buf.height as f32 * 0.28,
            crate::raster::Rgb(54, 188, 192),
            0.55,
        );
        buf.fill_circle(cx, cy, r + 1.5, crate::raster::Rgb(42, 168, 178), 0.72);
        buf.fill_circle(cx, cy, r, crate::raster::Rgb(3, 24, 36), 1.0);
        crate::raster::blit_to_frame(&buf, frame, area, tier);
    }
    if area.height >= 6 {
        let title_area = Rect::new(area.x, area.y + 1, area.width, 2);
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(vec![
                    Span::styled(
                        if options.ascii { "* " } else { "◆ " },
                        Style::default().fg(theme.gold).bold(),
                    ),
                    Span::styled(
                        "S U B S T R A T E",
                        Style::default().fg(theme.text_strong).bold(),
                    ),
                ]),
                Line::styled(
                    "PLANETARY EXPLORATION // ATTACHED TERMINAL",
                    Style::default().fg(theme.muted),
                ),
            ])
            .alignment(Alignment::Center),
            title_area,
        );
    }
}

fn render_compact_session_menu(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    state: SessionMenuState,
    options: RenderOptions,
    theme: Theme,
) {
    let active_label = match app.selected_session_menu_action() {
        SessionMenuAction::Resume => {
            if app.onboarding_pending() {
                "BEGIN"
            } else {
                "CONTINUE"
            }
        }
        SessionMenuAction::Detach => "DETACH (daemon runs)",
        SessionMenuAction::StopDaemon => "SAVE & STOP DAEMON",
        SessionMenuAction::NewPlaythrough => "NEW PLAYTHROUGH",
    };
    let status_line = match state {
        SessionMenuState::Selecting => format!("MENU > {active_label}"),
        SessionMenuState::ConfirmingNewPlaythrough => "CONFIRM NEW WORLD? ENTER/ESC".to_owned(),
        SessionMenuState::StartingNewPlaythrough => "CREATING WORLD...".to_owned(),
        SessionMenuState::StoppingDaemon => "STOPPING DAEMON...".to_owned(),
    };
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(
                " SUBSTRATE // SESSION MENU",
                Style::default().fg(theme.gold).bold(),
            ),
            Line::styled(
                format!(
                    " {}",
                    truncate_chars(
                        &status_line,
                        usize::from(area.width).saturating_sub(2),
                        options.ascii
                    )
                ),
                Style::default().fg(theme.cyan_bright).bold(),
            ),
            Line::styled(
                " UP/DN select · ENTER ok · ESC back",
                Style::default().fg(theme.muted),
            ),
        ])
        .style(theme.raised_style()),
        area,
    );
}

fn render_session_menu(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    options: RenderOptions,
    theme: Theme,
) {
    let Some(state) = app.session_menu else {
        return;
    };
    let connected = matches!(app.connection, ConnectionState::Connected);
    // Compact split-pane session menu (e.g. 38x5 Pilot Strip) so Q is always actionable.
    if area.height < 10 {
        render_compact_session_menu(frame, area, app, state, options, theme);
        return;
    }
    let (width, height, title, tone) = match state {
        SessionMenuState::Selecting => (68, 13, " SESSION MENU ", PanelTone::Modal),
        SessionMenuState::ConfirmingNewPlaythrough => {
            (68, 11, " NEW PLAYTHROUGH ", PanelTone::Warning)
        }
        SessionMenuState::StartingNewPlaythrough => {
            (58, 7, " CREATING PLAYTHROUGH ", PanelTone::Modal)
        }
        SessionMenuState::StoppingDaemon => (58, 7, " STOPPING SUBSTRATE ", PanelTone::Modal),
    };
    let menu_area = centered_rect(
        width.min(area.width.saturating_sub(2)),
        height.min(area.height.saturating_sub(2)),
        area,
    );
    let body = match state {
        SessionMenuState::Selecting => {
            let mut lines = vec![
                Line::styled(
                    if connected {
                        "CONNECTED // choose how to leave or begin"
                    } else {
                        "NO DAEMON CONNECTION // lifecycle actions unavailable"
                    },
                    Style::default().fg(if connected {
                        theme.success
                    } else {
                        theme.warning
                    }),
                ),
                Line::from(""),
            ];
            let fresh_world = app.onboarding_pending();
            let entries = [
                (
                    SessionMenuAction::Resume,
                    if fresh_world { "BEGIN" } else { "CONTINUE" },
                    if fresh_world {
                        "Initialize planetary signal coherence."
                    } else {
                        "Return to the current playthrough."
                    },
                    true,
                ),
                (
                    SessionMenuAction::Detach,
                    "DETACH",
                    "Close this interface; simulation continues.",
                    true,
                ),
                (
                    SessionMenuAction::StopDaemon,
                    "SAVE & STOP",
                    "Checkpoint progress, stop the daemon, and exit.",
                    connected,
                ),
                (
                    SessionMenuAction::NewPlaythrough,
                    "NEW PLAYTHROUGH",
                    "Replace current progress with a newly generated world.",
                    connected,
                ),
            ];
            for (action, label, description, enabled) in entries {
                let selected = app.selected_session_menu_action() == action;
                let marker = if selected {
                    if options.ascii {
                        ">"
                    } else {
                        "▸"
                    }
                } else {
                    " "
                };
                let color = if !enabled {
                    theme.muted
                } else if selected {
                    theme.amber
                } else {
                    theme.text_strong
                };
                lines.push(Line::styled(
                    format!(" {marker} {label}"),
                    Style::default().fg(color).bold(),
                ));
                lines.push(Line::styled(
                    format!("     {description}"),
                    Style::default().fg(if enabled { theme.text } else { theme.muted }),
                ));
            }
            if let Some(message) = &app.session_menu_message {
                lines.push(Line::styled(
                    message.clone(),
                    Style::default().fg(theme.warning),
                ));
            } else {
                lines.push(Line::styled(
                    "UP/DOWN select  //  ENTER choose  //  ESC resume",
                    Style::default().fg(theme.muted),
                ));
            }
            Text::from(lines)
        }
        SessionMenuState::ConfirmingNewPlaythrough => Text::from(vec![
            Line::styled(
                "CREATE A COMPLETELY NEW PLAYTHROUGH?",
                Style::default().fg(theme.warning).bold(),
            ),
            Line::from(""),
            Line::styled(
                "Current material progress, research, infrastructure, and archived",
                Style::default().fg(theme.text),
            ),
            Line::styled(
                "observations in this playthrough will be permanently replaced.",
                Style::default().fg(theme.text),
            ),
            Line::from(""),
            Line::styled(
                "The save is replaced atomically; a write failure leaves it unchanged.",
                Style::default().fg(theme.muted),
            ),
            Line::from(""),
            Line::styled(
                "ENTER create new playthrough  //  ESC back",
                Style::default().fg(theme.amber).bold(),
            ),
        ]),
        SessionMenuState::StartingNewPlaythrough => Text::from(vec![
            Line::from(""),
            Line::styled(
                "Generating a new world and committing its initial state...",
                Style::default().fg(theme.amber).bold(),
            ),
            Line::styled(
                "The existing save remains active until the write succeeds.",
                Style::default().fg(theme.muted),
            ),
        ]),
        SessionMenuState::StoppingDaemon => Text::from(vec![
            Line::from(""),
            Line::styled(
                "Checkpointing authoritative state and stopping all services...",
                Style::default().fg(theme.amber).bold(),
            ),
            Line::styled(
                "The terminal will close when shutdown is acknowledged.",
                Style::default().fg(theme.muted),
            ),
        ]),
    };
    // Size from wrapped rows. If descriptions do not fit, drop them; if even the labels do not
    // fit, use the compact strip menu so every entry and the key hints stay reachable.
    let inner_w = usize::from(menu_area.width.saturating_sub(2));
    let max_rows = usize::from(area.height.saturating_sub(2)).saturating_sub(2);
    let mut rows = crate::textwrap::hang_wrap(body.lines, inner_w);
    if rows.len() > max_rows && state == SessionMenuState::Selecting {
        rows.retain(|line| {
            let plain: String = line
                .spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect();
            !plain.starts_with("     ") && !plain.is_empty()
        });
    }
    if rows.len() > max_rows {
        render_compact_session_menu(frame, area, app, state, options, theme);
        return;
    }
    let menu_area = centered_rect(
        menu_area.width,
        u16::try_from(rows.len() + 2).unwrap_or(u16::MAX),
        area,
    );
    frame.render_widget(Clear, menu_area);
    frame.render_widget(
        Paragraph::new(rows)
            .style(theme.raised_style())
            .block(panel(title, options, theme, tone)),
        menu_area,
    );
}

fn panel<'a>(title: &'a str, options: RenderOptions, theme: Theme, tone: PanelTone) -> Block<'a> {
    let (surface, border_color, title_color, emphasized) = match tone {
        PanelTone::Focused => (theme.panel, theme.cyan, theme.cyan_bright, true),
        PanelTone::Standard => (theme.panel, theme.border, theme.muted, false),
        PanelTone::Modal => (theme.raised, theme.amber, theme.gold, true),
        PanelTone::Warning => (theme.raised, theme.warning, theme.warning, true),
    };
    let mut border_style = Style::default().fg(border_color);
    let mut title_style = Style::default().fg(title_color);
    if emphasized {
        border_style = border_style.add_modifier(Modifier::BOLD);
        title_style = title_style.add_modifier(Modifier::BOLD);
    }
    let border_set = match (options.ascii, tone) {
        (true, PanelTone::Focused) => ASCII_DOUBLE_BORDER,
        (true, _) => ASCII_BORDER,
        (false, PanelTone::Focused) => border::DOUBLE,
        (false, _) => border::ROUNDED,
    };
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_set(border_set)
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
    use mind_core::protocol::{
        DepositSnapshot, HealthState, OnboardingSnapshot, ResourceKind, RiskLevel, StatusSnapshot,
        SubregionClass,
    };
    use ratatui::{backend::TestBackend, Terminal};

    #[test]
    fn node_lead_guides_to_the_node_not_a_frontier() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([2, 2, 2]);
        let mut planet = engine.planet().unwrap();
        let region = planet.core_region_id.clone();
        planet.network.subregions.clear();
        for slot in 0..16_u8 {
            planet.network.subregions.push(SubregionSnapshot {
                id: format!("{region}_site_{}", slot + 1),
                designation: format!("R-081 / S-{}", slot + 1),
                region_id: region.clone(),
                slot,
                row: slot / 4,
                column: slot % 4,
                knowledge: if slot == 0 {
                    SubregionKnowledge::Detected
                } else {
                    SubregionKnowledge::Observed
                },
                class: Some(SubregionClass::ShelteredShelf),
                hazard: Some(RiskLevel::Low),
                energy_yield: Some(2),
                deposit: None,
                base_id: None,
            });
        }
        planet.campaign.next_action =
            "Restore communications at R-081: press C on its node to rejoin the network.".into();
        planet.campaign.next_action_key = Some("C".into());
        planet.campaign.lead_region_id = Some(region.clone());
        planet.campaign.lead_kind = Some(CampaignLeadKind::Node);
        planet.campaign.lead_subregion_id = Some(format!("{region}_site_7"));
        let mut app = App::default();
        app.apply_snapshot(status(true), Instant::now());
        app.apply_planet(planet, Instant::now());
        app.show_return_summary = false;
        app.clear_active_transmission_locally();
        app.local_view = true;
        let planet = app.planet.clone().unwrap();
        let (key, text) = local_next_override(&app, &planet);
        assert_eq!(key, Some("N"), "{text}");
        assert!(!text.contains("FRONTIER"), "{text}");
        assert!(!text.contains("(M)"), "{text}");
        assert!(app.focus_campaign_lead());
        assert_eq!(
            app.selected_subregion().map(|site| site.id.clone()),
            Some(format!("{region}_site_7"))
        );
        // On the node, the daemon's own key (C) is shown unchanged.
        let (key, text) = local_next_override(&app, &planet);
        assert_eq!(key, None, "{text}");
    }

    #[test]
    fn chapter_briefing_keeps_next_and_close_prompt_on_small_screens() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([2, 2, 2]);
        let planet = engine.planet().unwrap();
        for (width, height) in [(48_u16, 14_u16), (80, 24), (120, 36)] {
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal
                .draw(|frame| {
                    transmission::render_chapter_card(
                        frame,
                        frame.area(),
                        &planet.campaign,
                        RenderOptions {
                            ascii: true,
                            no_color: true,
                        },
                        Theme::new(true),
                    );
                })
                .unwrap();
            let buffer = terminal.backend().buffer().clone();
            let text: String = (0..buffer.area.height)
                .map(|y| {
                    (0..buffer.area.width)
                        .map(|x| buffer[(x, y)].symbol().to_owned())
                        .collect::<String>()
                        + "\n"
                })
                .collect();
            assert!(text.contains("NEXT"), "{width}x{height}\n{text}");
            assert!(text.contains("ENTER / ESC"), "{width}x{height}\n{text}");
        }
    }

    #[test]
    fn truncation_ends_on_a_word_boundary() {
        let text =
            "If this signature persists across unrelated sites, does one planet contain all of it?";
        let cut = truncate_chars(text, 40, true);
        assert!(cut.ends_with("..."), "{cut}");
        assert!(cut.chars().count() <= 40, "{cut}");
        let stem = cut.trim_end_matches("...");
        assert!(
            text.starts_with(stem) && text[stem.len()..].starts_with(' '),
            "{cut}"
        );
        assert_eq!(truncate_chars("short", 40, false), "short");
    }

    #[test]
    fn selection_scroll_keeps_the_selected_span_visible() {
        assert_eq!(selection_scroll(30, 10, Some((2, 4))), 0);
        assert_eq!(selection_scroll(30, 10, Some((18, 20))), 10);
        assert_eq!(selection_scroll(30, 10, Some((28, 30))), 20);
        assert_eq!(selection_scroll(8, 10, Some((6, 8))), 0);
        assert_eq!(selection_scroll(30, 10, None), 0);
    }

    #[test]
    fn paragraph_breaks_survive_rendering() {
        let lines = paragraph_lines("First sentence.\nSecond paragraph.", Style::default());
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[1].spans[0].content, "Second paragraph.");
    }

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
    fn local_site_view_preserves_parent_context_and_ascii_actions() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([2, 2, 2]);
        let mut planet = engine.planet().unwrap();
        planet.network.unlocked = true;
        planet.network.subregions.clear();
        for slot in 0..16_u8 {
            planet.network.subregions.push(SubregionSnapshot {
                id: format!("{}_site_{}", planet.core_region_id, slot + 1),
                designation: format!("R-081 / S-{}", slot + 1),
                region_id: planet.core_region_id.clone(),
                slot,
                row: slot / 4,
                column: slot % 4,
                knowledge: SubregionKnowledge::Observed,
                class: Some(if slot == 1 {
                    SubregionClass::ExposedRidge
                } else {
                    SubregionClass::ShelteredShelf
                }),
                hazard: Some(if slot == 1 {
                    RiskLevel::High
                } else {
                    RiskLevel::Low
                }),
                energy_yield: Some(u32::from(slot.min(5)) + 1),
                deposit: (slot == 2).then_some(DepositSnapshot {
                    resource: ResourceKind::Conductive,
                    remaining: 18,
                    risk: RiskLevel::Moderate,
                }),
                base_id: None,
            });
        }
        planet.network.subregions[14].knowledge = SubregionKnowledge::Detected;
        planet.network.subregions[14].hazard = None;
        planet.network.subregions[14].energy_yield = None;
        planet.network.subregions[15].knowledge = SubregionKnowledge::Unknown;
        planet.network.subregions[15].class = None;
        planet.network.subregions[15].hazard = None;
        planet.network.subregions[15].energy_yield = None;
        planet.network.subregions[0].base_id = Some("base_01".into());
        planet
            .network
            .bases
            .push(mind_core::protocol::BaseSnapshot {
                id: "base_01".into(),
                designation: "Node 01".into(),
                region_id: planet.core_region_id.clone(),
                subregion_id: planet.network.subregions[0].id.clone(),
                integrity: 84,
                generation: 9,
                demand: 4,
                stored_energy: 62,
                energy_capacity: 100,
                connected: true,
                modules: vec![
                    mind_core::protocol::BaseModuleSnapshot {
                        kind: BaseModuleKind::PowerArray,
                        enabled: true,
                        active: true,
                        generation: 8,
                        demand: 0,
                    },
                    mind_core::protocol::BaseModuleSnapshot {
                        kind: BaseModuleKind::Communications,
                        enabled: false,
                        active: false,
                        generation: 0,
                        demand: 2,
                    },
                ],
            });
        let mut app = App::default();
        app.apply_snapshot(status(true), Instant::now());
        app.apply_planet(planet, Instant::now());
        app.show_return_summary = false;
        app.clear_active_transmission_locally();
        app.local_view = true;
        let ascii = screen(
            &app,
            120,
            36,
            RenderOptions {
                ascii: true,
                no_color: true,
            },
        );
        assert!(ascii.contains("LOCAL FIELD 4x4"));
        assert!(ascii.contains("LOCAL INSPECTOR"));
        assert!(ascii.contains("SITE CONDITIONS"));
        assert!(ascii.contains("FRONTIER"));
        assert!(ascii.contains("UNCHARTED"));
        assert!(ascii.contains("[S] focused survey"));
        assert!(ascii.contains("LOCAL INSPECTOR [PgDn more]"), "{ascii}");
        assert!(ascii.contains("Power array [ONLINE]"));
        assert!(ascii.contains("Communications relay [STANDBY]"), "{ascii}");
        app.region_panel_scroll = 8;
        let scrolled = screen(
            &app,
            120,
            36,
            RenderOptions {
                ascii: true,
                no_color: true,
            },
        );
        assert!(scrolled.contains("[O] observation"), "{scrolled}");
        assert!(scrolled.contains("[P] suspend power"), "{scrolled}");
        let narrow = screen(
            &app,
            72,
            36,
            RenderOptions {
                ascii: true,
                no_color: true,
            },
        );
        assert!(narrow.contains("LOCAL FIELD 4x4"), "{narrow}");
        assert!(narrow.contains("LOCAL INSPECTOR"), "{narrow}");
        assert!(narrow.contains("A1"), "{narrow}");
        app.selected_subregion_slot = 0;
        app.move_subregion_selection(1, 0);
        assert_eq!(
            app.selected_subregion().map(|site| (site.row, site.column)),
            Some((1, 0))
        );
        app.move_subregion_selection(0, -1);
        assert_eq!(
            app.selected_subregion().map(|site| (site.row, site.column)),
            Some((1, 3))
        );
        assert!(
            ascii.contains("G/M") && ascii.contains("rig/maint") && ascii.contains("?  help"),
            "{ascii}"
        );
        assert!(!ascii.contains('▂'));
    }

    #[test]
    fn unsurveyed_local_field_explains_regional_survey_requirement() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([2, 2, 2]);
        let now = Instant::now();
        let mut app = App::default();
        app.apply_snapshot(engine.status(), now);
        app.apply_planet(engine.planet().unwrap(), now);
        app.show_return_summary = false;
        app.clear_active_transmission_locally();
        let unknown = engine
            .planet()
            .unwrap()
            .regions
            .into_iter()
            .find(|region| !region.core)
            .unwrap();
        app.selected_region_id = Some(unknown.id);
        app.enter_local_view();

        let output = screen(
            &app,
            160,
            36,
            RenderOptions {
                ascii: true,
                no_color: true,
            },
        );
        assert!(output.contains("LOCAL FIELD UNAVAILABLE"), "{output}");
        assert!(
            output.contains("Complete a regional survey here first"),
            "{output}"
        );
    }

    #[test]
    fn substrate_dark_applies_canvas_surfaces_and_semantic_accents() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([2, 2, 2]);
        engine.acknowledge_archive_presentation("observation_001");
        let mut app = App::default();
        app.apply_snapshot(engine.status(), Instant::now());
        app.apply_planet(engine.planet().unwrap(), Instant::now());
        app.connected();
        app.show_return_summary = false;

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
        engine.acknowledge_archive_presentation("observation_001");
        let mut app = App::default();
        app.apply_snapshot(engine.status(), Instant::now());
        app.apply_planet(engine.planet().unwrap(), Instant::now());
        app.connected();
        app.show_return_summary = false;

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
        app.clear_active_transmission_locally();
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
        assert!(overview.contains("QUESTION //"));
        assert!(overview.contains("CH I Retained Interval"));
        assert!(overview.contains("BALANCED"), "{overview}");

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
        assert!(confirmation.contains("Balanced") && confirmation.contains("rig"));
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
        app.show_return_summary = false;
        app.clear_active_transmission_locally();
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
        assert!(planet.contains("menu"));
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
        assert!(narrow.contains("menu"));

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
        app.show_return_summary = false;
        app.clear_active_transmission_locally();
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
    fn overview_exposes_construction_crews_and_accepted_depot_support() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([2, 2, 2]);
        let mut planet = engine.planet().unwrap();
        planet.network.project_capacity = 3;
        planet
            .network
            .active_projects
            .push(mind_core::protocol::BaseProjectSnapshot {
                project_id: 7,
                region_id: planet.core_region_id.clone(),
                subregion_id: format!("{}_site_1", planet.core_region_id),
                module: None,
                maintenance: false,
                logistics_supported: true,
                remaining_ms: 45_000,
                phase_total_ms: 90_000,
                cost: mind_core::protocol::ResourceStock {
                    structural: 10,
                    conductive: 6,
                    silicate: 4,
                },
            });
        let mut app = App::default();
        app.apply_snapshot(engine.status(), Instant::now());
        app.apply_planet(planet, Instant::now());
        app.show_return_summary = false;
        app.clear_active_transmission_locally();
        app.workspace = Workspace::Overview;

        let output = screen(
            &app,
            120,
            40,
            RenderOptions {
                ascii: true,
                no_color: true,
            },
        );
        assert!(output.contains("FOOTHOLD"));
        assert!(output.contains("DEPOT SUPPLY"));
        assert!(output.contains("1 / 3 committed"), "{output}");
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
        app.clear_active_transmission_locally();
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
        assert!(planet.contains("STORES"));
        assert!(planet.contains("RAW"));
        assert!(planet.contains("REF"));
        assert!(planet.contains("FAB"));
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
                archive_record_id: None,
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
        app.show_return_summary = false;
        app.clear_active_transmission_locally();
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

    #[test]
    fn archive_separates_permanent_observation_from_current_interpretation() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([2, 2, 2]);
        let mut planet = engine.planet().unwrap();
        planet.archive.experiment = Some(mind_core::protocol::ArchiveExperimentSnapshot {
            available: true,
            active: false,
            running: false,
            remaining_ms: 75_000,
            phase_total_ms: 75_000,
            status: "Recovered timing blocks are ready for a repeatable comparison.".into(),
        });
        let mut app = App::default();
        app.apply_snapshot(engine.status(), Instant::now());
        app.apply_planet(planet, Instant::now());
        app.connected();
        app.show_return_summary = false;
        app.clear_active_transmission_locally();
        app.workspace = Workspace::Archive;

        let output = screen(
            &app,
            100,
            36,
            RenderOptions {
                ascii: true,
                no_color: true,
            },
        );
        assert!(output.is_ascii());
        assert!(output.contains("PERMANENT ARCHIVE"));
        assert!(output.contains("Retained Interval"));
        assert!(output.contains("OBSERVATION"));
        assert!(output.contains("CURRENT INTERPRETATION"));
        assert!(output.contains("permanent knowledge"));
        assert!(output.contains("ARCHIVE EXPERIMENT"));
        assert!(output.contains("READY"));
        assert!(output.contains("begin timing comparison"));
    }

    #[test]
    fn solar_horizon_opens_a_terminal_system_map_with_ascii_fallback() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([2, 2, 2]);
        let mut planet = engine.planet().unwrap();
        planet.system_map.available = true;
        planet.archive.unpresented_record_ids.clear();
        planet.objective.title = "SOLAR HORIZON REACHED".into();
        planet.objective.guidance =
            "Open System Map to review the local frame. No system-scale operations are available in this demo."
                .into();
        planet.objective.complete = true;

        let mut app = App::default();
        app.apply_snapshot(engine.status(), Instant::now());
        app.apply_planet(planet, Instant::now());
        app.connected();
        app.show_return_summary = false;
        app.clear_active_transmission_locally();
        assert_eq!(app.workspace, Workspace::SystemMap);

        let unicode = screen(
            &app,
            120,
            36,
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        assert!(unicode.contains("SYSTEM MAP"));
        assert!(unicode.contains("LOCAL SYSTEM // OBSERVATION FRAME"));
        assert!(unicode.contains("CURRENT WORLD"));
        assert!(unicode.contains("OUTER RETURNS // UNRESOLVED"));
        assert!(unicode.contains("SOLAR HORIZON"));
        assert!(unicode.contains("PLANETARY CHAPTER COMPLETE"));
        assert!(unicode.contains("not the whole of"));
        assert!(unicode.contains("reachable environment"));

        let ascii = screen(
            &app,
            72,
            28,
            RenderOptions {
                ascii: true,
                no_color: true,
            },
        );
        assert!(ascii.is_ascii());
        assert!(ascii.contains("SYSTEM MAP"));
        assert!(ascii.contains("CURRENT WORLD"));
        assert!(ascii.contains("SOLAR HORIZON"));
        assert!(ascii.contains("PLANETARY CHAPTER COMPLETE"));
        assert!(ascii.contains("No system-scale actions"));

        let narrow = screen(
            &app,
            50,
            20,
            RenderOptions {
                ascii: true,
                no_color: true,
            },
        );
        assert!(narrow.contains("SOLAR HORIZON REACHED"));
        assert!(narrow.contains("current world tracked"));
        assert!(narrow.contains("Outer returns remain unresolved"));
        assert!(narrow.contains("PLANETARY CHAPTER COMPLETE"));
        assert!(narrow.contains("No system-scale actions"));
    }

    #[test]
    fn session_menu_and_destructive_confirmation_are_readable_in_fallback_mode() {
        let mut app = App::default();
        app.apply_snapshot(status(true), Instant::now());
        app.connected();
        app.open_session_menu();

        let menu = screen(
            &app,
            80,
            24,
            RenderOptions {
                ascii: true,
                no_color: true,
            },
        );
        assert!(menu.is_ascii());
        assert!(menu.contains("SESSION MENU"));
        assert!(menu.contains("CONTINUE"));
        assert!(menu.contains("DETACH"));
        assert!(menu.contains("SAVE & STOP"));
        assert!(menu.contains("NEW PLAYTHROUGH"));
        assert!(menu.contains("simulation continues"));

        app.confirm_new_playthrough();
        let confirmation = screen(
            &app,
            80,
            24,
            RenderOptions {
                ascii: true,
                no_color: true,
            },
        );
        assert!(confirmation.contains("CREATE A COMPLETELY NEW PLAYTHROUGH?"));
        assert!(confirmation.contains("permanently replaced"));
        assert!(confirmation.contains("write failure"));
        assert!(confirmation.contains("unchanged"));
        assert!(confirmation.contains("ENTER create new playthrough"));

        app.begin_daemon_stop();
        let stopping = screen(
            &app,
            80,
            24,
            RenderOptions {
                ascii: true,
                no_color: true,
            },
        );
        assert!(stopping.contains("STOPPING SUBSTRATE"));
        assert!(stopping.contains("stopping all"));
        assert!(stopping.contains("services"));
    }

    #[test]
    fn discovery_transmission_reveals_diagram_observation_and_interpretation_in_unicode_and_ascii()
    {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([2, 2, 2]);
        let mut app = App::default();
        app.apply_snapshot(engine.status(), Instant::now());
        app.apply_planet(engine.planet().unwrap(), Instant::now());
        app.connected();
        app.show_return_summary = false;
        app.transmission_revealed = true;

        let unicode = screen(
            &app,
            104,
            34,
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        assert!(
            unicode.contains("TRANSMISSION // RECORD 01 OF 01"),
            "{unicode}"
        );
        assert!(unicode.contains("RETAINED INTERVAL"), "{unicode}");
        assert!(unicode.contains("OBSERVATION //"), "{unicode}");
        assert!(unicode.contains("CURRENT INTERPRETATION //"), "{unicode}");
        assert!(unicode.contains("moving away from shelter"), "{unicode}");
        assert!(unicode.contains("file permanently in Archive"), "{unicode}");

        let ascii = screen(
            &app,
            78,
            26,
            RenderOptions {
                ascii: true,
                no_color: true,
            },
        );
        assert!(ascii.is_ascii());
        assert!(ascii.contains("TRANSMISSION // RECORD 01 OF 01"), "{ascii}");
        assert!(ascii.contains("RETAINED INTERVAL"), "{ascii}");
        assert!(ascii.contains("OBSERVATION //"), "{ascii}");
        assert!(ascii.contains("CURRENT INTERPRETATION //"), "{ascii}");
    }

    #[test]
    fn chapter_card_and_header_present_foreground_question_pips_and_next_step() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([2, 2, 2]);
        engine.acknowledge_archive_presentation("observation_001");
        let mut app = App::default();
        app.apply_snapshot(engine.status(), Instant::now());
        app.apply_planet(engine.planet().unwrap(), Instant::now());
        app.connected();
        app.show_return_summary = false;

        let header_frame = screen(
            &app,
            132,
            32,
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        assert!(
            header_frame.contains("CH I Retained Interval"),
            "{header_frame}"
        );
        assert!(header_frame.contains("QUESTION //"), "{header_frame}");
        assert!(header_frame.contains("NEXT //"), "{header_frame}");

        app.toggle_chapter_card();
        let card = screen(
            &app,
            96,
            28,
            RenderOptions {
                ascii: true,
                no_color: true,
            },
        );
        assert!(card.is_ascii());
        assert!(
            card.contains("CHAPTER I OF 5 // RETAINED INTERVAL"),
            "{card}"
        );
        assert!(card.contains("OPEN QUESTION //"), "{card}");
        assert!(card.contains("CHAPTER STEPS //"), "{card}");
    }

    #[test]
    fn return_summary_quotes_newly_recorded_archive_evidence_verbatim() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([2, 2, 2]);
        let mut app = App::default();
        app.apply_snapshot(engine.status(), Instant::now());
        app.apply_planet(engine.planet().unwrap(), Instant::now());
        app.connected();
        // Initial attach after awakening has observation_001 in unreviewed_activity with archive_record_id.
        let output = screen(
            &app,
            100,
            36,
            RenderOptions {
                ascii: true,
                no_color: true,
            },
        );
        assert!(output.is_ascii());
        assert!(output.contains("RETURN SUMMARY"), "{output}");
        assert!(
            output.contains("NEW EVIDENCE RECORDED // RETAINED INTERVAL"),
            "{output}"
        );
        assert!(output.contains("moving away from shelter"), "{output}");
        assert!(
            output.contains("1 full discovery transmission queued next"),
            "{output}"
        );
    }

    #[test]
    fn coherence_onboarding_renders_signal_oscilloscope_and_lock_state() {
        let mut app = App::default();
        app.apply_snapshot(status(false), Instant::now());
        app.connected();

        let drifting = screen(
            &app,
            96,
            30,
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        assert!(drifting.contains("SIGNAL DRIFT"), "{drifting}");
        assert!(
            drifting.contains('▁') || drifting.contains('▃') || drifting.contains('▅'),
            "{drifting}"
        );

        app.band_positions = [5, 5, 5];
        let locked = screen(
            &app,
            96,
            30,
            RenderOptions {
                ascii: true,
                no_color: true,
            },
        );
        assert!(locked.is_ascii());
        assert!(locked.contains("SIGNAL LOCK"), "{locked}");
        assert!(locked.contains("ENTER  observe"), "{locked}");
    }

    #[test]
    fn archive_experiment_renders_live_waveform_in_running_and_paused_states() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([2, 2, 2]);
        engine.acknowledge_archive_presentation("observation_001");
        let mut planet = engine.planet().unwrap();
        planet.archive.experiment = Some(mind_core::protocol::ArchiveExperimentSnapshot {
            available: false,
            active: true,
            running: true,
            remaining_ms: 42_000,
            phase_total_ms: 75_000,
            status: "Comparing recovered timing intervals against internal cadence.".into(),
        });
        let mut app = App::default();
        app.apply_snapshot(engine.status(), Instant::now());
        app.apply_planet(planet.clone(), Instant::now());
        app.connected();
        app.show_return_summary = false;
        app.clear_active_transmission_locally();
        app.workspace = Workspace::Archive;

        let running = screen(
            &app,
            104,
            36,
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        assert!(running.contains("RUNNING"), "{running}");
        assert!(running.contains("BLOCK A"), "{running}");
        assert!(running.contains("CADENCE"), "{running}");
        assert!(running.contains("LOCK 44%"), "{running}");

        if let Some(exp) = planet.archive.experiment.as_mut() {
            exp.running = false;
        }
        app.apply_planet(planet, Instant::now());
        let paused = screen(
            &app,
            104,
            36,
            RenderOptions {
                ascii: true,
                no_color: true,
            },
        );
        assert!(paused.is_ascii());
        assert!(paused.contains("PAUSED"), "{paused}");
        assert!(paused.contains("BLOCK A"), "{paused}");
        assert!(paused.contains("CADENCE"), "{paused}");
    }

    #[test]
    fn local_field_renders_pixel_art_terrain_sprites_and_survey_sweep_with_ascii_fallback() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([2, 2, 2]);
        engine.acknowledge_archive_presentation("observation_001");
        let mut planet = engine.planet().unwrap();
        let region_id = planet.core_region_id.clone();
        planet.network.subregions.clear();
        for slot in 0..16_u8 {
            let (knowledge, class, base_id) = match slot {
                0 => (
                    SubregionKnowledge::Observed,
                    Some(SubregionClass::ShelteredShelf),
                    Some("node_01".into()),
                ),
                1 => (
                    SubregionKnowledge::Detected,
                    Some(SubregionClass::ExposedRidge),
                    None,
                ),
                2 => (
                    SubregionKnowledge::Observed,
                    Some(SubregionClass::BasinFloor),
                    None,
                ),
                _ => (SubregionKnowledge::Unknown, None, None),
            };
            planet.network.subregions.push(SubregionSnapshot {
                id: format!("{region_id}_site_{}", slot + 1),
                designation: format!("R-081 / S-{}", slot + 1),
                region_id: region_id.clone(),
                slot,
                row: slot / 4,
                column: slot % 4,
                knowledge,
                class,
                hazard: (knowledge == SubregionKnowledge::Observed).then_some(RiskLevel::Low),
                energy_yield: (knowledge == SubregionKnowledge::Observed).then_some(4),
                deposit: (slot == 2).then_some(DepositSnapshot {
                    resource: ResourceKind::Conductive,
                    remaining: 18,
                    risk: RiskLevel::Low,
                }),
                base_id,
            });
        }
        planet.network.active_site_survey = Some(mind_core::protocol::SiteSurveySnapshot {
            subregion_id: planet.network.subregions[1].id.clone(),
            remaining_ms: 8_000,
            phase_total_ms: 14_000,
        });
        planet
            .network
            .bases
            .push(mind_core::protocol::BaseSnapshot {
                id: "node_01".into(),
                designation: "Node 01".into(),
                region_id: region_id.clone(),
                subregion_id: planet.network.subregions[0].id.clone(),
                integrity: 92,
                generation: 9,
                demand: 3,
                stored_energy: 64,
                energy_capacity: 100,
                connected: true,
                modules: vec![mind_core::protocol::BaseModuleSnapshot {
                    kind: BaseModuleKind::PowerArray,
                    enabled: true,
                    active: true,
                    generation: 8,
                    demand: 0,
                }],
            });

        let mut app = App::default();
        app.apply_snapshot(engine.status(), Instant::now());
        app.apply_planet(planet, Instant::now());
        app.connected();
        app.show_return_summary = false;
        app.local_view = true;

        let unicode = screen(
            &app,
            120,
            40,
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        assert!(unicode.contains("LOCAL FIELD"), "{unicode}");
        assert!(unicode.contains("SCAN"), "{unicode}");

        let ascii = screen(
            &app,
            120,
            40,
            RenderOptions {
                ascii: true,
                no_color: true,
            },
        );
        assert!(ascii.is_ascii());
        assert!(ascii.contains("LOCAL FIELD"), "{ascii}");
        assert!(ascii.contains("SCAN"), "{ascii}");
    }

    #[test]
    fn boot_intro_renders_full_boot_and_quick_power_on_in_unicode_and_ascii() {
        let now = Instant::now();
        let mut app = App::default();
        app.enable_boot_intro();
        app.apply_snapshot(status(false), now);
        app.connected();

        let render_at = |app: &App, at: Instant, options: RenderOptions| {
            let backend = TestBackend::new(100, 32);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal
                .draw(|frame| render(frame, app, options, at))
                .unwrap();
            let buffer = terminal.backend().buffer();
            let mut lines = Vec::new();
            for y in 0..32 {
                let mut row = String::new();
                for x in 0..100 {
                    row.push_str(buffer[(x, y)].symbol());
                }
                lines.push(row);
            }
            lines.join("\n")
        };

        let full_unicode = render_at(
            &app,
            now + std::time::Duration::from_millis(1_800),
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        assert!(
            full_unicode.contains("SYSTEM INITIALIZATION"),
            "{full_unicode}"
        );
        assert!(full_unicode.contains("CORE BUS"), "{full_unicode}");
        assert!(full_unicode.contains("PRESS ANY KEY"), "{full_unicode}");

        let full_ascii = render_at(
            &app,
            now + std::time::Duration::from_millis(1_800),
            RenderOptions {
                ascii: true,
                no_color: true,
            },
        );
        assert!(full_ascii.is_ascii());
        assert!(full_ascii.contains("SUBSTRATE"), "{full_ascii}");
        assert!(full_ascii.contains("SYSTEM INITIALIZATION"), "{full_ascii}");

        app.skip_boot_intro();
        assert!(!app.boot_intro_active(now + std::time::Duration::from_millis(1_800)));

        let mut returning = App::default();
        returning.enable_boot_intro();
        returning.apply_snapshot(status(true), now);
        returning.connected();
        let quick = render_at(
            &returning,
            now + std::time::Duration::from_millis(250),
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        assert!(quick.contains("TERMINAL LINK ACTIVE"), "{quick}");
        assert!(!returning.boot_intro_active(now + std::time::Duration::from_millis(900)));
    }

    #[test]
    fn crt_transition_wipe_renders_and_clears_on_input() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([2, 2, 2]);
        engine.acknowledge_archive_presentation("observation_001");
        let now = Instant::now();
        let mut app = App::default();
        app.apply_snapshot(engine.status(), now);
        app.apply_planet(engine.planet().unwrap(), now);
        app.connected();
        app.show_return_summary = false;
        app.trigger_transition(now);

        let mid = now + std::time::Duration::from_millis(90);
        assert!(app.transition_progress(mid).is_some());

        let backend = TestBackend::new(90, 28);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                render(
                    frame,
                    &app,
                    RenderOptions {
                        ascii: true,
                        no_color: true,
                    },
                    mid,
                )
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        let mut text = String::new();
        for y in 0..28 {
            for x in 0..90 {
                text.push_str(buffer[(x, y)].symbol());
            }
            text.push('\n');
        }
        assert!(text.is_ascii());
        assert!(text.contains("====="), "{text}");

        app.clear_transition();
        assert!(app.transition_progress(mid).is_none());
    }

    #[test]
    fn ansi16_mode_renders_clean_ega_palette_without_rgb() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([2, 2, 2]);
        engine.acknowledge_archive_presentation("observation_001");
        let now = Instant::now();
        let mut app = App::default();
        app.ansi16 = true;
        app.apply_snapshot(engine.status(), now);
        app.apply_planet(engine.planet().unwrap(), now);
        app.connected();
        app.show_return_summary = false;

        let backend = TestBackend::new(100, 32);
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
                    now,
                )
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        for y in 0..32 {
            for x in 0..100 {
                let cell = &buffer[(x, y)];
                assert!(
                    !matches!(cell.fg, ratatui::style::Color::Rgb(_, _, _)),
                    "unexpected RGB fg at ({x}, {y}): {:?}",
                    cell.fg
                );
                assert!(
                    !matches!(cell.bg, ratatui::style::Color::Rgb(_, _, _)),
                    "unexpected RGB bg at ({x}, {y}): {:?}",
                    cell.bg
                );
            }
        }
    }

    #[test]
    fn probe_bay_renders_scout_and_morphing_extractor_rigs_including_rebuild_gantry() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([2, 2, 2]);
        engine.acknowledge_archive_presentation("observation_001");
        let now = Instant::now();
        let mut planet = engine.planet().unwrap();
        let mut app = App::default();
        app.apply_snapshot(engine.status(), now);
        app.apply_planet(planet.clone(), now);
        app.connected();
        app.show_return_summary = false;
        app.workspace = Workspace::Probes;

        let scout_screen = screen(
            &app,
            116,
            32,
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        assert!(scout_screen.contains("PROBE BAY"), "{scout_screen}");
        assert!(scout_screen.contains("WIDE SENSOR DISH"), "{scout_screen}");
        assert!(scout_screen.contains("SLOT A"), "{scout_screen}");
        assert!(scout_screen.contains("RESERVED"), "{scout_screen}");

        // Switch to Extraction Probe and verify all three rigs visibly morph the blueprint.
        app.select_probe(1);
        let balanced = screen(
            &app,
            116,
            32,
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        assert!(balanced.contains("BALANCED FRAME"), "{balanced}");
        assert!(balanced.contains("RIG LOADOUT MATRIX"), "{balanced}");
        assert_eq!(
            app.extractor_loadout_action(),
            Some(ContextAction::ConfigureExtractor(ExtractorLoadout::Cargo))
        );

        planet.extractor.loadout = ExtractorLoadout::Cargo;
        planet.revision += 1;
        app.apply_planet(planet.clone(), now);
        let cargo = screen(
            &app,
            116,
            32,
            RenderOptions {
                ascii: true,
                no_color: true,
            },
        );
        assert!(cargo.is_ascii());
        assert!(cargo.contains("+2 OUTBOARD HOPPERS"), "{cargo}");

        planet.extractor.loadout = ExtractorLoadout::Shielded;
        planet.revision += 1;
        app.apply_planet(planet.clone(), now);
        let shielded = screen(
            &app,
            116,
            32,
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        assert!(shielded.contains("ABLATIVE CANOPY"), "{shielded}");

        // Verify rebuilding state displays the assembly gantry.
        planet.extractor.state = ExtractorState::Rebuilding;
        planet.extractor.rebuild_remaining_ms = 45_000;
        planet.revision += 1;
        app.apply_planet(planet, now);
        let rebuilding = screen(
            &app,
            116,
            32,
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        assert!(rebuilding.contains("GANTRY ACTIVE"), "{rebuilding}");
        assert!(rebuilding.contains("FABRICATING"), "{rebuilding}");

        // Enter jumps cleanly to Planet focused on the probe's region.
        assert!(app.focus_selected_probe_region());
        assert_eq!(app.workspace, Workspace::Planet);

        // Verify 80x24 narrow stacked Probe Bay can scroll to reach rig matrix and hardpoints.
        app.workspace = Workspace::Probes;
        app.select_probe(1);
        app.scroll_probe_panel(8);
        let narrow_scrolled = screen(
            &app,
            80,
            24,
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        assert!(
            narrow_scrolled.contains("RIG LOADOUT MATRIX")
                || narrow_scrolled.contains("TRV")
                || narrow_scrolled.contains("Balanced"),
            "{narrow_scrolled}"
        );
    }

    #[test]
    fn confirmed_new_playthrough_arms_full_boot_and_return_summary_allows_quick_boot_animation() {
        let now = Instant::now();
        let mut app = App::default();
        app.enable_boot_intro();
        app.apply_snapshot(status(true), now);
        app.connected();
        app.skip_boot_intro();

        // Confirmed new playthrough resets local state and re-arms full boot.
        app.new_playthrough_completed(mind_core::protocol::NewPlaythroughResult {
            ok: true,
            code: None,
        });
        app.apply_snapshot(status(false), now + std::time::Duration::from_millis(50));
        assert!(app.boot_intro_active(now + std::time::Duration::from_millis(100)));
        assert_eq!(
            app.boot_intro.map(|b| b.mode),
            Some(crate::boot::BootMode::FullBoot)
        );

        // Pending return summary must not suppress quick-boot animation frames.
        let mut returning = App::default();
        returning.enable_boot_intro();
        returning.show_return_summary = true;
        returning.apply_snapshot(status(true), now);
        assert!(returning.boot_intro_active(now + std::time::Duration::from_millis(100)));
        assert!(returning
            .next_frame_deadline(now + std::time::Duration::from_millis(100))
            .is_some());

        // Same-batch Resume + boot skip resets a queued transmission timer so its reveal starts fresh.
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([2, 2, 2]);
        let old_arrival = now - std::time::Duration::from_secs(5);
        let mut batched = App::default();
        batched.enable_boot_intro();
        batched.open_session_menu();
        batched.apply_snapshot(engine.status(), old_arrival);
        batched.apply_planet(engine.planet().unwrap(), old_arrival);
        batched.show_return_summary = false;
        assert!(batched.active_transmission_id.is_some());
        batched.close_session_menu();
        batched.skip_boot_intro();
        let opened_at = batched.transmission_opened_at.unwrap();
        assert!(
            opened_at > old_arrival,
            "transmission timer should reset after resume+skip batch"
        );
    }

    #[test]
    fn region_terrain_diorama_hides_unknown_and_renders_discovered_horizon() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([2, 2, 2]);
        engine.acknowledge_archive_presentation("observation_001");
        let now = Instant::now();
        let planet = engine.planet().unwrap();
        let mut app = App::default();
        app.apply_snapshot(engine.status(), now);
        app.apply_planet(planet.clone(), now);
        app.connected();
        app.show_return_summary = false;

        // Core region is surveyed and renders its terrain diorama and core presence badge.
        let surveyed = screen(
            &app,
            116,
            34,
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        assert!(surveyed.contains("CORE COMPLEX ONLINE"), "{surveyed}");

        // Select an unknown region: diorama shows interference static without leaking terrain.
        let unknown_id = planet
            .regions
            .iter()
            .find(|r| r.knowledge == mind_core::protocol::RegionKnowledge::Unknown)
            .unwrap()
            .id
            .clone();
        app.selected_region_id = Some(unknown_id);
        let unknown = screen(
            &app,
            116,
            34,
            RenderOptions {
                ascii: true,
                no_color: true,
            },
        );
        assert!(unknown.is_ascii());
        assert!(unknown.contains("UNRESOLVED HORIZON"), "{unknown}");
        assert!(!unknown.contains("CORE COMPLEX ONLINE"), "{unknown}");
    }

    #[test]
    fn local_structure_cutaway_and_discovery_flash_render_cleanly() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([2, 2, 2]);
        engine.acknowledge_archive_presentation("observation_001");
        let now = Instant::now();
        let mut planet = engine.planet().unwrap();
        let core_id = planet.core_region_id.clone();

        let mut app = App::default();
        app.apply_snapshot(engine.status(), now);
        app.apply_planet(planet.clone(), now);
        app.connected();
        app.show_return_summary = false;

        // Simulate a newly surveyed region + local node with multiple modules.
        if let Some(unknown) = planet
            .regions
            .iter_mut()
            .find(|r| r.knowledge == mind_core::protocol::RegionKnowledge::Unknown)
        {
            unknown.knowledge = mind_core::protocol::RegionKnowledge::Surveyed;
            unknown.terrain = Some(mind_core::protocol::TerrainClass::Highland);
        }
        planet.network.subregions.clear();
        planet.network.subregions.push(SubregionSnapshot {
            id: format!("{core_id}_site_1"),
            designation: "R-081 / S-1".into(),
            region_id: core_id.clone(),
            slot: 0,
            row: 0,
            column: 0,
            knowledge: SubregionKnowledge::Observed,
            class: Some(SubregionClass::ShelteredShelf),
            hazard: Some(RiskLevel::Low),
            energy_yield: Some(5),
            deposit: None,
            base_id: Some("node_01".into()),
        });
        planet
            .network
            .bases
            .push(mind_core::protocol::BaseSnapshot {
                id: "node_01".into(),
                designation: "Node 01".into(),
                region_id: core_id,
                subregion_id: planet.network.subregions[0].id.clone(),
                integrity: 88,
                generation: 10,
                demand: 4,
                stored_energy: 70,
                energy_capacity: 100,
                connected: true,
                modules: vec![
                    mind_core::protocol::BaseModuleSnapshot {
                        kind: BaseModuleKind::PowerArray,
                        enabled: true,
                        active: true,
                        generation: 8,
                        demand: 0,
                    },
                    mind_core::protocol::BaseModuleSnapshot {
                        kind: BaseModuleKind::Observatory,
                        enabled: true,
                        active: true,
                        generation: 0,
                        demand: 3,
                    },
                ],
            });
        planet.revision += 1;
        app.apply_planet(planet, now);
        assert!(!app.active_discovery_flash_ids(now).is_empty());

        app.local_view = true;
        let cutaway = screen(
            &app,
            120,
            38,
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        assert!(cutaway.contains("▐PP▌"), "{cutaway}");
        assert!(cutaway.contains("▐OO▌"), "{cutaway}");
        assert!(cutaway.contains("P·PWR"), "{cutaway}");

        let cutaway_ascii = screen(
            &app,
            120,
            38,
            RenderOptions {
                ascii: true,
                no_color: true,
            },
        );
        assert!(cutaway_ascii.is_ascii());
        assert!(cutaway_ascii.contains("[PP]"), "{cutaway_ascii}");
        assert!(cutaway_ascii.contains("[OO]"), "{cutaway_ascii}");
    }

    #[test]
    fn unfolding_tabs_compact_80x24_ansi256_and_pilot_strip_render_cleanly() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([2, 2, 2]);
        engine.acknowledge_archive_presentation("observation_001");
        let now = Instant::now();
        let mut app = App::default();
        app.apply_snapshot(engine.status(), now);
        app.apply_planet(engine.planet().unwrap(), now);
        app.connected();
        app.show_return_summary = false;

        // Fresh post-onboarding state unfolds only Planet, Archive, System.
        assert_eq!(
            app.visible_workspaces(),
            vec![Workspace::Planet, Workspace::Archive, Workspace::System]
        );
        assert!(app.select_visible_workspace(2));
        assert_eq!(app.workspace, Workspace::Archive);
        assert!(app.select_visible_workspace(1));
        assert_eq!(app.workspace, Workspace::Planet);

        // 80x24 uses side-by-side globe + inspector with polygon vista and compact header.
        let compact = screen(
            &app,
            80,
            24,
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        assert!(compact.contains("REGIONAL GLOBE"), "{compact}");
        assert!(compact.contains("REGION INSPECTOR"), "{compact}");
        assert!(compact.contains("CORE COMPLEX ONLINE"), "{compact}");

        // Small split-pane (38x5) renders the 3-line Pilot Strip when connected with planet.
        let pilot = screen(
            &app,
            38,
            5,
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        assert!(pilot.contains("SUBSTRATE"), "{pilot}");
        assert!(pilot.contains("MAP 1/200"), "{pilot}");
        assert!(pilot.contains("NEXT"), "{pilot}");

        // xterm-256 mode emits zero RGB colors on the globe and polygon vista.
        app.ansi256 = true;
        let backend = TestBackend::new(100, 32);
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
                    now,
                )
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        let mut indexed_cells = 0usize;
        for y in 4..29 {
            for x in 1..99 {
                let cell = &buffer[(x, y)];
                if matches!(cell.fg, ratatui::style::Color::Indexed(_))
                    || matches!(cell.bg, ratatui::style::Color::Indexed(_))
                {
                    indexed_cells += 1;
                }
            }
        }
        assert!(
            indexed_cells > 100,
            "expected xterm-256 indexed cells in globe/vista"
        );
    }

    #[test]
    fn archive_replay_is_full_screen_without_background_bleed_or_re_ack() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([2, 2, 2]);
        engine.acknowledge_archive_presentation("observation_001");
        let now = Instant::now();
        let mut app = App::default();
        app.apply_snapshot(engine.status(), now);
        app.apply_planet(engine.planet().unwrap(), now);
        app.connected();
        app.show_return_summary = false;
        app.workspace = Workspace::Archive;

        assert!(app.open_archive_replay(now));
        app.transmission_revealed = true;
        let frame_txt = screen(
            &app,
            120,
            36,
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        assert!(frame_txt.contains("ARCHIVE REPLAY"), "{frame_txt}");
        assert!(frame_txt.contains("RETAINED INTERVAL"), "{frame_txt}");
        assert!(frame_txt.contains("return to Archive"), "{frame_txt}");
        // Truly full-screen: Planet/Archive chrome must not bleed around the edges.
        assert!(!frame_txt.contains("PERMANENT ARCHIVE"), "{frame_txt}");
        assert!(!frame_txt.contains("REGIONAL GLOBE"), "{frame_txt}");

        // Dismissing a replay returns None (does not re-acknowledge to the daemon).
        assert_eq!(app.reveal_or_dismiss_transmission(now), None);
        assert!(app.active_transmission_record().is_none());
    }

    #[test]
    fn xterm256_emits_zero_rgb_cells_across_full_frame() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([2, 2, 2]);
        engine.acknowledge_archive_presentation("observation_001");
        let now = Instant::now();
        let mut app = App::default();
        app.apply_snapshot(engine.status(), now);
        app.apply_planet(engine.planet().unwrap(), now);
        app.connected();
        app.show_return_summary = false;
        app.ansi256 = true;

        let backend = TestBackend::new(120, 36);
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
                    now,
                )
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        let mut rgb_cells = 0usize;
        let mut indexed_cells = 0usize;
        for y in 0..36 {
            for x in 0..120 {
                let cell = &buffer[(x, y)];
                if matches!(cell.fg, ratatui::style::Color::Rgb(_, _, _))
                    || matches!(cell.bg, ratatui::style::Color::Rgb(_, _, _))
                {
                    rgb_cells += 1;
                }
                if matches!(cell.fg, ratatui::style::Color::Indexed(_))
                    || matches!(cell.bg, ratatui::style::Color::Indexed(_))
                {
                    indexed_cells += 1;
                }
            }
        }
        assert_eq!(
            rgb_cells, 0,
            "xterm-256 mode must emit zero 24-bit RGB cells"
        );
        assert!(
            indexed_cells > 500,
            "expected indexed xterm-256 palette across full frame"
        );
    }

    #[test]
    fn solar_horizon_labels_avoid_sprites_at_wide_and_compact() {
        let theme = Theme::for_color_mode(false, false);
        let options = RenderOptions {
            ascii: false,
            no_color: false,
        };
        for &(chart_w, chart_h) in &[(76u16, 30u16), (52u16, 19u16), (78u16, 11u16)] {
            for &elapsed_ms in &[200u64, 1300, 2700] {
                let backend = TestBackend::new(chart_w, chart_h);
                let mut terminal = Terminal::new(backend).unwrap();
                let elapsed = std::time::Duration::from_millis(elapsed_ms);
                terminal
                    .draw(|frame| {
                        art::render_system_cliffhanger_chart(
                            frame,
                            Rect::new(0, 0, chart_w, chart_h),
                            elapsed,
                            0.4,
                            options,
                            theme,
                            crate::raster::ColorTier::TrueColor,
                        );
                    })
                    .unwrap();
                let buf = terminal.backend().buffer();
                let inner_w = chart_w - 2;
                let inner_h = chart_h - 2;
                let mut found_cw = None;
                let mut found_star = None;
                for y in 1..(chart_h - 1) {
                    let row_str: String =
                        (1..(chart_w - 1)).map(|x| buf[(x, y)].symbol()).collect();
                    if let Some(pos) = row_str.find("CURRENT WORLD") {
                        found_cw = Some((pos as u16, y - 1));
                    }
                    if let Some(pos) = row_str.find("LOCAL STAR") {
                        found_star = Some((pos as u16, y - 1));
                    }
                }
                let (cw_col, cw_row) = found_cw.expect("CURRENT WORLD label must render");
                let (_, star_lbl_row) = found_star.expect("LOCAL STAR label must render");
                // Neither label may sit on the bottom legend row.
                assert!(
                    cw_row < inner_h - 1,
                    "CURRENT WORLD hit legend row at {chart_w}x{chart_h}"
                );
                assert!(
                    star_lbl_row < inner_h - 1,
                    "LOCAL STAR hit legend row at {chart_w}x{chart_h}"
                );
                let (world_col, world_row) = crate::cinematic::solar_horizon_world_cell(
                    inner_w,
                    inner_h,
                    elapsed.as_secs_f32(),
                    0.4,
                );
                let overlaps = world_row.abs_diff(cw_row) <= 1
                    && world_col + 2 >= cw_col
                    && world_col <= cw_col + 15;
                assert!(
                    !overlaps,
                    "CURRENT WORLD ({cw_col},{cw_row}) overlaps world sprite ({world_col},{world_row}) at {chart_w}x{chart_h} {elapsed_ms}ms"
                );
                let bottom_legend: String = (1..(chart_w - 1))
                    .map(|x| buf[(x, chart_h - 2)].symbol())
                    .collect();
                assert!(
                    bottom_legend.contains("OUTER RETURNS"),
                    "bottom legend missing at {chart_w}x{chart_h}: {bottom_legend}"
                );
            }
        }
    }

    #[test]
    fn narrow_transmission_scrolls_and_pilot_strip_reports_active_research() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([2, 2, 2]);
        let now = Instant::now();
        let mut app = App::default();
        app.apply_snapshot(engine.status(), now);
        app.apply_planet(engine.planet().unwrap(), now);
        app.connected();
        app.show_return_summary = false;
        app.transmission_revealed = true;

        let top_narrow = screen(
            &app,
            48,
            14,
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        assert!(
            top_narrow.contains("MORE"),
            "narrow transmission must show scroll continuation cue: {top_narrow}"
        );
        app.scroll_transmission(6);
        let scrolled_narrow = screen(
            &app,
            48,
            14,
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        assert!(
            scrolled_narrow.contains("interpretation")
                || scrolled_narrow.contains("prior")
                || scrolled_narrow.contains("calibration")
                || scrolled_narrow.contains("TOP"),
            "scrolling narrow transmission should reveal lower prose: {scrolled_narrow}"
        );

        // Pilot Strip must report active research instead of IDLE.
        app.reveal_or_dismiss_transmission(now);
        engine.acknowledge_archive_presentation("observation_001");
        let mut planet = engine.planet().unwrap();
        planet.research.unlocked = true;
        planet.research.active = Some(mind_core::protocol::ActiveResearchSnapshot {
            project: mind_core::protocol::ResearchProject::ExtendedTelemetry,
            remaining_ms: 42_000,
            phase_total_ms: 75_000,
        });
        planet.revision += 1;
        app.apply_planet(planet, now);
        let pilot = screen(
            &app,
            38,
            5,
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        assert!(
            pilot.contains("RESEARCH") && !pilot.contains("IDLE") && !pilot.contains("NEXT: "),
            "pilot strip must report active research without duplicate NEXT prefix, got: {pilot}"
        );
    }

    #[test]
    fn local_lead_focus_selects_announced_frontier_site_and_enables_survey() {
        let mut state = mind_core::game::GameState::with_seed(17);
        state.complete_onboarding([2, 2, 2]);
        state.acknowledge_archive_presentation("observation_001");
        let target = state
            .planet_snapshot()
            .unwrap()
            .regions
            .into_iter()
            .find(|r| r.hop_distance_from_core == 1)
            .unwrap();
        state.deploy_scout(&target.id);
        state.advance(40_000);
        let planet = state.planet_snapshot().unwrap();
        let now = Instant::now();
        let mut app = App::default();
        app.apply_snapshot(status(true), now);
        app.apply_planet(planet, now);
        app.connected();
        app.show_return_summary = false;
        app.selected_region_id = Some(target.id.clone());
        app.enter_local_view();
        // Start on an already-observed site (or any site), then press [N].
        assert!(app.focus_campaign_lead());
        let selected = app
            .selected_subregion()
            .expect("selected subregion after N");
        assert_eq!(
            selected.knowledge,
            mind_core::protocol::SubregionKnowledge::Detected,
            "N must select a Detected frontier sector, got {:?}",
            selected.knowledge
        );
        let notice = app.notice.as_deref().unwrap_or("");
        assert!(
            notice.contains(&selected.designation),
            "N notice ({notice}) must match selected site designation ({})",
            selected.designation
        );
        assert!(
            matches!(
                app.local_primary_action(),
                Some(ContextAction::SurveySubregion(ref id)) if id == &selected.id
            ),
            "focused survey [S] must immediately be ready on the N-selected site"
        );
    }

    #[test]
    fn compact_80x24_local_tiles_preserve_all_ids_and_help_scrolls_to_archive() {
        let mut state = mind_core::game::GameState::with_seed(17);
        state.complete_onboarding([2, 2, 2]);
        state.acknowledge_archive_presentation("observation_001");
        let target = state
            .planet_snapshot()
            .unwrap()
            .regions
            .into_iter()
            .find(|r| r.hop_distance_from_core == 1)
            .unwrap();
        state.deploy_scout(&target.id);
        state.advance(40_000);
        let now = Instant::now();
        let mut app = App::default();
        app.apply_snapshot(status(true), now);
        app.apply_planet(state.planet_snapshot().unwrap(), now);
        app.connected();
        app.show_return_summary = false;
        app.selected_region_id = Some(target.id);
        app.enter_local_view();

        let local_80 = screen(
            &app,
            80,
            24,
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        for id in ["S-10", "S-11", "S-12", "S-13", "S-14", "S-15", "S-16"] {
            assert!(
                local_80.contains(id),
                "80x24 local field must preserve distinct {id}: {local_80}"
            );
        }
        assert!(
            !local_80.contains("SELECTE"),
            "tile title clipped: {local_80}"
        );
        assert!(
            !local_80.contains("UNCHARTE"),
            "tile status clipped: {local_80}"
        );

        // Help modal at 80x24 scrolls cleanly to reveal Archive controls and closing notes.
        app.local_view = false;
        app.show_help = true;
        app.help_scroll = 18;
        let help_80 = screen(
            &app,
            80,
            24,
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        assert!(help_80.contains("LOCAL FIELD"), "{help_80}");
        assert!(help_80.contains("Core always rebuilds"), "{help_80}");
    }

    #[test]
    fn research_active_row_displays_active_and_compact_session_menu_fits_38x5() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([2, 2, 2]);
        engine.acknowledge_archive_presentation("observation_001");
        let now = Instant::now();
        let mut planet = engine.planet().unwrap();
        // Even with 3 analysis units, Research tab stays hidden until research.unlocked is true.
        planet.research.analysis_units = 3;
        planet.research.unlocked = false;
        let mut app = App::default();
        app.apply_snapshot(engine.status(), now);
        app.apply_planet(planet.clone(), now);
        app.connected();
        app.show_return_summary = false;
        assert!(!app.visible_workspaces().contains(&Workspace::Research));

        // Once unlocked and active, the active direction displays ACTIVE (never LOCKED).
        planet.research.unlocked = true;
        planet.research.active = Some(mind_core::protocol::ActiveResearchSnapshot {
            project: mind_core::protocol::ResearchProject::FieldHardening,
            remaining_ms: 50_000,
            phase_total_ms: 75_000,
        });
        for opt in &mut planet.research.options {
            opt.available = false;
        }
        planet.revision += 1;
        app.apply_planet(planet, now);
        app.workspace = Workspace::Research;
        let research_txt = screen(
            &app,
            120,
            36,
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        assert!(
            research_txt.contains("Field Hardening")
                && research_txt
                    .lines()
                    .any(|l| l.contains("Field Hardening") && l.contains("ACTIVE")),
            "active research row must display ACTIVE: {research_txt}"
        );

        // Compact 38x5 Session Menu renders clearly on Q.
        app.open_session_menu();
        let menu_38 = screen(
            &app,
            38,
            5,
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        assert!(menu_38.contains("SESSION MENU"), "{menu_38}");
        assert!(menu_38.contains("CONTINUE"), "{menu_38}");
    }
}
