use crate::{
    app::{
        App, ConnectionState, ContextAction, OnboardingView, SessionMenuAction, SessionMenuState,
        Workspace,
    },
    art, boot,
    globe::{self, GlobeRenderOptions},
    probes,
    theme::Theme,
    transmission,
};
use mind_core::protocol::{
    BaseModuleKind, EngineEventKind, ExtractorLoadout, ExtractorState, MissionReportOutcome,
    PlanetSnapshot, ScoutState, SubregionKnowledge, SubregionSnapshot,
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

pub fn render(frame: &mut Frame<'_>, app: &App, options: RenderOptions, now: Instant) {
    let area = frame.area();
    let theme = Theme::for_color_mode(options.no_color, app.ansi16);
    frame.render_widget(Block::default().style(theme.canvas_style()), area);
    if area.width < 44 || area.height < 14 {
        render_too_small(frame, area, options, theme);
        if app.session_menu.is_some() {
            render_session_menu(frame, area, app, options, theme);
        }
        return;
    }
    if app.session_menu.is_none() {
        if let Some(boot_state) = app
            .boot_intro
            .filter(|boot_state| boot_state.is_active(now))
        {
            boot::render_boot_intro(frame, area, boot_state, options, theme, now);
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
    if app.show_return_summary {
        render_return_summary(frame, area, app, options, theme);
    } else if let (Some(planet), Some((index, record))) =
        (&app.planet, app.active_transmission_record())
    {
        let source = record.region_id.as_ref().map_or_else(
            || "Planetary context".to_owned(),
            |region_id| region_designation(planet, region_id).to_owned(),
        );
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
                phase: app.globe_phase,
                options,
                theme,
            },
        );
    } else if app.show_chapter_card {
        if let Some(planet) = &app.planet {
            transmission::render_chapter_card(frame, area, &planet.campaign, options, theme);
        }
    }
    if app.show_help {
        render_help(frame, area, options, theme);
    }
    if app.session_menu.is_some() {
        render_session_menu(frame, area, app, options, theme);
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
    let header_height = if area.height >= 24 { 6 } else { 5 };
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

    if area.width >= 100 {
        let columns = Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)])
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
            label: "primary",
        },
        KeyHint {
            key: "P/C/D/W/L/O",
            label: "modules",
        },
        KeyHint {
            key: "G/M",
            label: "rig / maintain",
        },
        KeyHint {
            key: "PGUP/DN",
            label: "details",
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
                label: "menu",
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
    let mut summary_lines = vec![
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
        field_line(
            "LOCAL SITES",
            if planet.network.unlocked {
                format!(
                    "{} surveyed // {} required for horizon",
                    planet.network.horizon.surveyed_subregions,
                    planet.network.horizon.required_subregions
                )
            } else {
                "locked until routine automation is proven".into()
            },
            theme,
        ),
        field_line(
            "NODES",
            format!(
                "{} total // {} connected // {} beyond relay envelope",
                planet.network.bases.len(),
                planet.network.horizon.connected_bases,
                planet.network.horizon.remote_bases
            ),
            theme,
        ),
        field_line(
            "NETWORK POWER",
            format!(
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
            theme,
        ),
        field_line(
            "CREWS",
            format!(
                "{} / {} committed",
                planet.network.active_projects.len(),
                planet.network.project_capacity
            ),
            theme,
        ),
        field_line(
            "LOGISTICS",
            format!(
                "{} active connected depot(s) // each adds one crew, maximum two",
                planet
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
                    .count()
            ),
            theme,
        ),
        field_line(
            "OBSERVATION",
            format!(
                "labs {}/{} // arrays {}/{} // regions {}/{}",
                planet.network.horizon.analysis_sites,
                planet.network.horizon.required_analysis_sites,
                planet.network.horizon.observation_sites,
                planet.network.horizon.required_observation_sites,
                planet.network.horizon.surveyed_regions,
                planet.network.horizon.required_regions
            ),
            theme,
        ),
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
    ];
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
    let summary = Text::from(summary_lines);
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
        Constraint::Length(4),
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
        Constraint::Length(4),
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
        let list = Text::from(
            planet
                .archive
                .records
                .iter()
                .enumerate()
                .flat_map(|(index, candidate)| {
                    let marker = if index == selected { ">" } else { " " };
                    [
                        Line::from(vec![
                            Span::styled(
                                format!("{marker} {:02} ", index + 1),
                                Style::default().fg(if index == selected {
                                    theme.amber
                                } else {
                                    theme.faint
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
                        ]),
                        Line::styled(
                            format!("     {}", candidate.medium.to_uppercase()),
                            Style::default().fg(theme.muted),
                        ),
                    ]
                })
                .collect::<Vec<_>>(),
        );
        let source = record.region_id.as_ref().map_or_else(
            || "Planetary context".into(),
            |region_id| region_designation(planet, region_id).to_owned(),
        );
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
        ));
        detail_lines.extend([
            Line::from(""),
            section_line("OBSERVATION", theme),
            Line::styled(
                terminal_text(&record.observation, options.ascii),
                Style::default().fg(theme.text),
            ),
            Line::from(""),
            section_line("CURRENT INTERPRETATION", theme),
            Line::styled(
                terminal_text(&record.interpretation, options.ascii),
                Style::default().fg(theme.muted),
            ),
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
        let panels = if rows[1].width >= 88 {
            Layout::horizontal([Constraint::Percentage(34), Constraint::Percentage(66)])
                .split(rows[1])
        } else {
            Layout::vertical([Constraint::Percentage(38), Constraint::Percentage(62)])
                .split(rows[1])
        };
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
        frame.render_widget(
            Paragraph::new(detail)
                .wrap(Wrap { trim: true })
                .style(theme.panel_style())
                .block(panel(" OBSERVATION ", options, theme, PanelTone::Standard)),
            panels[1],
        );
    }
    let mut hints = vec![
        KeyHint {
            key: "UP/DOWN",
            label: "select record",
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

fn terminal_text(value: &str, ascii: bool) -> String {
    if !ascii {
        return value.to_owned();
    }
    let mut rendered = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '“' | '”' => rendered.push('"'),
            '‘' | '’' => rendered.push('\''),
            '—' | '–' => rendered.push('-'),
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
        .filter_map(|record_id| {
            planet
                .archive
                .records
                .iter()
                .find(|record| record.record_id == record_id)
        })
        .collect();
    let width = area.width.saturating_sub(6).clamp(38, 86);
    let extra_evidence_lines = if archived_evidence.is_empty() { 0 } else { 5 };
    let height = (planet.unreviewed_activity.len() as u16 + 15 + extra_evidence_lines)
        .min(area.height.saturating_sub(2))
        .max(12);
    let popup = centered_rect(width, height, area);
    frame.render_widget(Clear, popup);
    let mut lines = vec![
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
        Line::styled(
            "While the daemon remained active:",
            Style::default().fg(theme.text).bold(),
        ),
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
            .take(height.saturating_sub(11) as usize)
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
    if let Some(evidence) = archived_evidence.last() {
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            format!(
                "NEW EVIDENCE RECORDED // {} ({})",
                evidence.title.to_uppercase(),
                evidence.medium.to_uppercase()
            ),
            Style::default().fg(theme.amber).bold(),
        ));
        lines.push(Line::styled(
            terminal_text(&evidence.observation, options.ascii),
            Style::default().fg(theme.text_strong),
        ));
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
        format!(
            "NEXT  {}",
            terminal_text(&planet.campaign.next_action, options.ascii)
        ),
        Style::default().fg(theme.text_strong),
    ));
    if !planet.archive.unpresented_record_ids.is_empty() {
        lines.push(Line::styled(
            format!(
                "      {} full discovery transmission(s) queued next.",
                planet.archive.unpresented_record_ids.len()
            ),
            Style::default().fg(theme.cyan_bright),
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
    let compact = area.width < 118;
    let ultra_compact = area.width < 84;
    let tab = |workspace, label: &str| {
        let formatted = if compact {
            label.to_owned()
        } else {
            format!(" {label} ")
        };
        if active == workspace {
            Span::styled(formatted, theme.selection_style().fg(theme.cyan_bright))
        } else {
            Span::styled(formatted, Style::default().fg(theme.muted))
        }
    };
    let mut navigation = if ultra_compact {
        vec![Span::styled(
            if options.ascii { "# " } else { "◆ " },
            Style::default().fg(theme.amber),
        )]
    } else {
        vec![
            Span::styled(
                if options.ascii { " # " } else { " ◆ " },
                Style::default().fg(theme.amber),
            ),
            Span::styled("SUBSTRATE ", Style::default().fg(theme.text_strong).bold()),
            Span::styled("// ", Style::default().fg(theme.faint)),
        ]
    };
    navigation.extend([
        tab(Workspace::Planet, "PLANET"),
        Span::raw(" "),
        tab(Workspace::Overview, "OVERVIEW"),
        Span::raw(" "),
        tab(Workspace::Probes, "PROBES"),
        Span::raw(" "),
        tab(Workspace::Research, "RESEARCH"),
        Span::raw(" "),
        tab(Workspace::Archive, "ARCHIVE"),
        Span::raw(" "),
    ]);
    if app
        .planet
        .as_ref()
        .is_some_and(|planet| planet.system_map.available)
    {
        navigation.extend([tab(Workspace::SystemMap, "SYSTEM MAP"), Span::raw(" ")]);
    }
    navigation.push(tab(Workspace::System, "SYSTEM"));
    if let Some(planet) = &app.planet {
        if !ultra_compact {
            let pips = transmission::step_pips(&planet.campaign, options.ascii);
            navigation.extend([
                Span::styled(
                    if compact { " // " } else { "  //  " },
                    Style::default().fg(theme.faint),
                ),
                Span::styled(
                    format!(
                        "CH {} {} ",
                        transmission::roman_chapter(planet.campaign.chapter_number),
                        planet.campaign.title
                    ),
                    Style::default().fg(theme.amber).bold(),
                ),
                Span::styled(format!("[{pips}]"), Style::default().fg(theme.cyan_bright)),
            ]);
        }
    }
    let second_line = if let Some(planet) = &app.planet {
        Line::from(vec![
            Span::styled(" QUESTION // ", Style::default().fg(theme.cyan).bold()),
            Span::styled(
                terminal_text(&planet.campaign.question, options.ascii),
                Style::default().fg(theme.text_strong).bold(),
            ),
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
    let mut lines = vec![Line::from(navigation), second_line];
    if active == Workspace::Planet {
        if area.height >= 6 {
            if let Some(planet) = &app.planet {
                let mut next_spans = vec![Span::styled(
                    " NEXT // ",
                    Style::default().fg(theme.amber).bold(),
                )];
                if let Some(key) = &planet.campaign.next_action_key {
                    next_spans.push(Span::styled(format!(" {key} "), theme.key_style()));
                    next_spans.push(Span::raw(" "));
                }
                next_spans.push(Span::styled(
                    terminal_text(&planet.campaign.next_action, options.ascii),
                    Style::default().fg(theme.text),
                ));
                lines.push(Line::from(next_spans));
            }
        }
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
    if width < 140 {
        let profile = planet
            .research
            .profiles
            .iter()
            .find(|profile| profile.active)
            .map_or("Balanced", |profile| profile.title.as_str());
        return Line::from(vec![
            Span::styled(" STORES // ", Style::default().fg(theme.amber).bold()),
            Span::styled(
                format!(
                    "MAP {surveyed}/{}  RAW {}/{}/{}  REF {}/{}/{}  FAB {fabrication}  P {profile}  A {}",
                    planet.regions.len(),
                    raw.structural,
                    raw.conductive,
                    raw.silicate,
                    refined.structural,
                    refined.conductive,
                    refined.silicate,
                    planet.research.analysis_units
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

fn render_probes(
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
                key: "1/2",
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
                label: "switch probe",
            },
            KeyHint {
                key: "G",
                label: "cycle rig",
            },
            KeyHint {
                key: "PGUP/DN",
                label: "scroll specs",
            },
            KeyHint {
                key: "ENTER",
                label: "jump to region",
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
    let row_count = u32::from(planet.network.subregion_rows.max(1));
    let column_count = u32::from(planet.network.subregion_columns.max(1));
    let rows = Layout::vertical(
        (0..row_count)
            .map(|_| Constraint::Ratio(1, row_count))
            .collect::<Vec<_>>(),
    )
    .split(inner);
    let selected_id = app.selected_subregion().map(|site| site.id.as_str());
    for (row_index, row_area) in rows.iter().enumerate() {
        let columns = Layout::horizontal(
            (0..column_count)
                .map(|_| Constraint::Ratio(1, column_count))
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
            let inner_rows = cell.height.saturating_sub(2);
            if inner_rows < 5 && art_lines.len() > 1 {
                art_lines.truncate(1);
            }
            let mut lines = art_lines;
            let sep = if options.ascii { "-" } else { "·" };
            lines.push(Line::styled(
                format!(
                    "{coordinate} {sep} {}",
                    site.designation
                        .split(" / ")
                        .last()
                        .unwrap_or(site.designation.as_str())
                ),
                Style::default().fg(theme.text_strong).bold(),
            ));
            if let (Some(energy), Some(hazard)) = (site.energy_yield, site.hazard) {
                lines.push(Line::styled(
                    format!("E{energy} // {}", risk_text(hazard)),
                    Style::default().fg(if hazard == mind_core::protocol::RiskLevel::High {
                        theme.warning
                    } else {
                        theme.text
                    }),
                ));
            } else {
                lines.push(Line::styled(
                    if active_survey {
                        "SCANNING"
                    } else {
                        match site.knowledge {
                            SubregionKnowledge::Unknown => "UNCHARTED",
                            SubregionKnowledge::Detected => "FRONTIER",
                            SubregionKnowledge::Surveyed => "SURVEYED",
                            SubregionKnowledge::Observed => "OBSERVED",
                        }
                    },
                    Style::default().fg(if active_survey {
                        theme.amber
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
            frame.render_widget(
                Paragraph::new(lines)
                    .alignment(Alignment::Center)
                    .style(if selected {
                        theme.raised_style()
                    } else {
                        theme.panel_style()
                    })
                    .block(panel(
                        if selected {
                            " SELECTED "
                        } else if newly_scanned {
                            " NEW SCAN "
                        } else if active_survey {
                            " SCAN "
                        } else if site.knowledge == SubregionKnowledge::Unknown {
                            " FOG "
                        } else {
                            " SECTOR "
                        },
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
            let mut lines = vec![
                Line::styled(
                    site.designation.clone(),
                    Style::default().fg(theme.text_strong).bold(),
                ),
                Line::from(""),
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
            ];
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
                    "LOCAL SURVEY",
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
            let site_base = planet
                .network
                .bases
                .iter()
                .find(|base| base.subregion_id == site.id);
            if let Some(base) = site_base {
                if area.height >= 30 {
                    lines.extend(art::local_structure_cutaway(
                        site,
                        Some(base),
                        false,
                        app.globe_phase,
                        options,
                        theme,
                    ));
                    lines.push(Line::from(""));
                }
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
            if let Some(base) = site_base {
                if area.height >= 20 && area.height < 30 {
                    lines.push(Line::from(""));
                    lines.push(section_line("OUTPOST SCHEMATIC", theme));
                    lines.extend(art::local_structure_cutaway(
                        site,
                        Some(base),
                        false,
                        app.globe_phase,
                        options,
                        theme,
                    ));
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
                    " SITE CONFIRMATION "
                } else {
                    " LOCAL INSPECTOR "
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
        local_action_line(
            "M",
            "maintenance",
            app.base_maintenance_action().is_some(),
            local_maintenance_blocker(planet, site),
            theme,
        ),
    ];
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
    if planet.network.active_site_survey.is_some() {
        return "another local survey is active".into();
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

            let mut lines = vec![designation_line(&region.designation, region.core, theme)];
            let region_has_concurrent_ops = planet
                .active_mission
                .as_ref()
                .is_some_and(|m| m.target_region_id == region.id)
                && planet
                    .active_extraction
                    .as_ref()
                    .is_some_and(|m| m.target_region_id == region.id);
            let show_top_diorama = area.height >= 22 && !region_has_concurrent_ops;
            if show_top_diorama {
                lines.extend(art::region_terrain_diorama(
                    region,
                    planet,
                    app.globe_phase,
                    options,
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
                if !show_top_diorama && area.height >= 22 {
                    lines.push(Line::from(""));
                    lines.extend(art::region_terrain_diorama(
                        region,
                        planet,
                        app.globe_phase,
                        options,
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
    let mut lines = vec![
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
            "G",
            format!("select {}", planet.extractor.loadout.next().title()),
            ContextAction::ConfigureExtractor(planet.extractor.loadout.next()),
            app.extractor_loadout_action(),
            None,
            extractor_loadout_blocker(planet),
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
    ];
    let local_field_available = planet
        .network
        .subregions
        .iter()
        .any(|site| site.region_id == region.id);
    let local_field_status = if !local_field_available {
        crate::app::LOCAL_FIELD_SURVEY_GUIDANCE
    } else if planet.network.unlocked {
        "open spatial local field"
    } else {
        "open 4x4 local field (focused survey ready)"
    };
    lines.push(Line::from(vec![
        Span::styled("[V] ", Style::default().fg(theme.amber).bold()),
        Span::styled(
            "local field",
            Style::default().fg(if local_field_available {
                theme.text_strong
            } else {
                theme.muted
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
    ]));
    lines
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

    let panel_area = centered_rect(58.min(area.width.saturating_sub(2)), 22, chunks[1]);
    let panel = panel(" AWAKENING ", options, theme, PanelTone::Focused);
    let inner = panel.inner(panel_area);
    frame.render_widget(panel, panel_area);

    match app.onboarding_view {
        OnboardingView::Adjusting => {
            let rows = Layout::vertical([
                Constraint::Length(2),
                Constraint::Min(11),
                Constraint::Length(1),
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
                Paragraph::new(art::coherence_oscilloscope(
                    app.band_positions,
                    app.globe_phase,
                    options.ascii,
                    theme,
                ))
                .alignment(Alignment::Center)
                .style(theme.panel_style()),
                rows[2],
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
                rows[3],
            );
            if let Some(notice) = &app.notice {
                frame.render_widget(
                    Paragraph::new(notice.as_str())
                        .alignment(Alignment::Center)
                        .style(theme.panel_style().fg(theme.warning).bold()),
                    rows[4],
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
        38.min(area.height.saturating_sub(2)),
        area,
    );
    let help = Text::from(vec![
        section_line("GLOBAL", theme),
        key_line("Q", "open session menu", theme),
        key_line("CTRL-C", "detach immediately; daemon keeps running", theme),
        key_line("R", "reconnect when disconnected", theme),
        key_line("TAB", "cycle workspace", theme),
        key_line("N", "focus current campaign lead on the globe", theme),
        key_line("C", "open current chapter briefing", theme),
        key_line("? / ESC", "close help", theme),
        Line::from(""),
        section_line("PLANET", theme),
        key_line("ARROWS", "navigate one region", theme),
        key_line("SHIFT + ARROWS", "jump across the 10 x 20 grid", theme),
        key_line("ENTER", "use the selected region's primary action", theme),
        key_line("S", "deploy survey scout", theme),
        key_line("X", "prepare extraction from a known deposit", theme),
        key_line("G", "cycle balanced / cargo / shielded rig", theme),
        key_line("B", "prepare relay construction when funded", theme),
        key_line("R", "recall the scout assigned to this region", theme),
        key_line("A", "queue or clear earned local automation", theme),
        key_line("PGUP / PGDN", "scroll the region inspector", theme),
        Line::from(""),
        section_line("LOCAL FIELD", theme),
        key_line("V / ESC", "enter local field / return to globe", theme),
        key_line("ARROWS", "move through adjacent sectors", theme),
        key_line("S / X / B", "survey / extract / establish node", theme),
        key_line("P / C", "build or toggle power / communications", theme),
        key_line("D / W", "build or toggle depot / workshop", theme),
        key_line("L / O", "build or toggle laboratory / observation", theme),
        key_line("M", "maintain a weathered node", theme),
        Line::from(""),
        section_line("OVERVIEW", theme),
        key_line("ARROWS", "select an operation or report", theme),
        key_line("ENTER", "inspect its region on the globe", theme),
        key_line("PGUP / PGDN", "scroll planet-wide details", theme),
        Line::from(""),
        section_line("PROBE BAY", theme),
        key_line(
            "LEFT / RIGHT / 1 / 2",
            "switch scout or extraction schematic",
            theme,
        ),
        key_line("G", "swap extractor rig live when ready", theme),
        key_line("ENTER", "jump to the probe's region on the globe", theme),
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
            let entries = [
                (
                    SessionMenuAction::Resume,
                    "CONTINUE",
                    "Return to the current playthrough.",
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
    frame.render_widget(Clear, menu_area);
    frame.render_widget(
        Paragraph::new(body)
            .wrap(Wrap { trim: false })
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
        assert!(ascii.contains("[P] suspend power"), "{ascii}");
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
            ascii.contains("G/M") && ascii.contains("rig / maintain"),
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
            output.contains("1 full discovery transmission(s) queued next"),
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
    fn boot_intro_renders_cold_boot_and_quick_power_on_in_unicode_and_ascii() {
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

        let cold_unicode = render_at(
            &app,
            now + std::time::Duration::from_millis(1_800),
            RenderOptions {
                ascii: false,
                no_color: false,
            },
        );
        assert!(
            cold_unicode.contains("COLD INITIALIZATION"),
            "{cold_unicode}"
        );
        assert!(cold_unicode.contains("CORE BUS"), "{cold_unicode}");
        assert!(cold_unicode.contains("PRESS ANY KEY"), "{cold_unicode}");

        let cold_ascii = render_at(
            &app,
            now + std::time::Duration::from_millis(1_800),
            RenderOptions {
                ascii: true,
                no_color: true,
            },
        );
        assert!(cold_ascii.is_ascii());
        assert!(cold_ascii.contains("SUBSTRATE"), "{cold_ascii}");
        assert!(cold_ascii.contains("COLD INITIALIZATION"), "{cold_ascii}");

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
            narrow_scrolled.contains("RIG LOADOUT MATRIX") || narrow_scrolled.contains("TRV"),
            "{narrow_scrolled}"
        );
    }

    #[test]
    fn confirmed_new_playthrough_arms_cold_boot_and_return_summary_allows_quick_boot_animation() {
        let now = Instant::now();
        let mut app = App::default();
        app.enable_boot_intro();
        app.apply_snapshot(status(true), now);
        app.connected();
        app.skip_boot_intro();

        // Confirmed new playthrough resets local state and re-arms full cold boot.
        app.new_playthrough_completed(mind_core::protocol::NewPlaythroughResult {
            ok: true,
            code: None,
        });
        app.apply_snapshot(status(false), now + std::time::Duration::from_millis(50));
        assert!(app.boot_intro_active(now + std::time::Duration::from_millis(100)));
        assert_eq!(
            app.boot_intro.map(|b| b.mode),
            Some(crate::boot::BootMode::FullColdBoot)
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
}
