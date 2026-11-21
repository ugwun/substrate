//! Live late-1980s EGA schematic Probe Bay workspace.
//!
//! Renders inspectable half-block / box-drawing blueprints of the Survey Scout and Extraction
//! Probe from existing authoritative `PlanetSnapshot` data, with live rig morphing (`G`),
//! assembly gantry progress during extractor rebuild, research/profile modifiers, and clearly
//! marked reserved slots for future modular probe systems.

use crate::{app::App, theme::Theme, ui::RenderOptions};
use mind_core::protocol::{ExtractorLoadout, ExtractorState, PlanetSnapshot, ScoutState};
use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    symbols::border,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
use std::time::Instant;

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

pub const PROBE_COUNT: usize = 2;

pub fn render_probe_bay_body(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    planet: &PlanetSnapshot,
    options: RenderOptions,
    theme: Theme,
    now: Instant,
) {
    if area.width >= 96 {
        let columns = Layout::horizontal([Constraint::Percentage(54), Constraint::Percentage(46)])
            .split(area);
        render_blueprint_panel(frame, columns[0], app, planet, options, theme, now);
        render_telemetry_panel(frame, columns[1], app, planet, options, theme, now);
    } else {
        let rows =
            Layout::vertical([Constraint::Percentage(54), Constraint::Percentage(46)]).split(area);
        render_blueprint_panel(frame, rows[0], app, planet, options, theme, now);
        render_telemetry_panel(frame, rows[1], app, planet, options, theme, now);
    }
}

fn render_blueprint_panel(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    planet: &PlanetSnapshot,
    options: RenderOptions,
    theme: Theme,
    now: Instant,
) {
    let selected_extractor = app.probe_selection % PROBE_COUNT == 1;
    let title = if selected_extractor {
        format!(
            " PROBE BAY // {} ({}) ",
            planet.extractor.designation.to_uppercase(),
            planet.extractor.loadout.title().to_uppercase()
        )
    } else {
        format!(
            " PROBE BAY // {} (PROTECTED) ",
            planet.scout.designation.to_uppercase()
        )
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_set(if options.ascii {
            ASCII_DOUBLE_BORDER
        } else {
            border::DOUBLE
        })
        .style(theme.blueprint_style())
        .border_style(
            Style::default()
                .fg(theme.cobalt_bright)
                .add_modifier(Modifier::BOLD),
        )
        .title(Span::styled(
            title,
            Style::default().fg(theme.gold).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width < 12 || inner.height < 6 {
        return;
    }

    let mut lines = Vec::new();
    // Chassis switcher tabs inside the blueprint bay.
    let scout_tab = if !selected_extractor {
        Span::styled(
            format!(" [1] {} ", planet.scout.designation),
            theme.key_style(),
        )
    } else {
        Span::styled(
            format!("  1  {} ", planet.scout.designation),
            Style::default().fg(theme.muted),
        )
    };
    let extractor_tab = if selected_extractor {
        Span::styled(
            format!(" [2] {} ", planet.extractor.designation),
            theme.key_style(),
        )
    } else {
        Span::styled(
            format!("  2  {} ", planet.extractor.designation),
            Style::default().fg(theme.muted),
        )
    };
    lines.push(Line::from(vec![
        scout_tab,
        Span::styled("   ", theme.blueprint_style()),
        extractor_tab,
    ]));

    let grid_rule = if options.ascii {
        "-".repeat(usize::from(inner.width.saturating_sub(2)).min(52))
    } else {
        "┄".repeat(usize::from(inner.width.saturating_sub(2)).min(52))
    };
    lines.push(Line::styled(
        grid_rule.clone(),
        Style::default().fg(theme.blueprint_grid),
    ));

    let blink = (app.globe_phase * 4.0).floor() as i64 % 2 == 0;
    if selected_extractor {
        lines.extend(extractor_blueprint_lines(
            app,
            planet,
            blink,
            options.ascii,
            theme,
            now,
        ));
    } else {
        lines.extend(scout_blueprint_lines(planet, blink, options.ascii, theme));
    }

    lines.push(Line::styled(
        grid_rule,
        Style::default().fg(theme.blueprint_grid),
    ));
    let status_banner = if selected_extractor {
        extractor_status_banner(app, planet, now)
    } else {
        scout_status_banner(app, planet, now)
    };
    lines.push(Line::styled(
        status_banner,
        Style::default().fg(theme.gold).bold(),
    ));

    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .alignment(Alignment::Center)
            .style(theme.blueprint_style()),
        inner,
    );
}

fn scout_blueprint_lines(
    planet: &PlanetSnapshot,
    blink: bool,
    ascii: bool,
    theme: Theme,
) -> Vec<Line<'static>> {
    let hull = Style::default().fg(theme.cyan_bright).bold();
    let frame_s = Style::default().fg(theme.cobalt_bright);
    let callout = Style::default().fg(theme.gold).bold();
    let active = Style::default().fg(theme.amber).bold();
    let dim = Style::default().fg(theme.muted);

    let state = planet.scout.state;
    let moving = matches!(state, ScoutState::Outbound | ScoutState::Returning);
    let scanning = state == ScoutState::Surveying;

    let mast_tip = match (ascii, blink) {
        (true, true) => "*",
        (true, false) => "+",
        (false, true) => "◈",
        (false, false) => "◇",
    };
    let sweep_line = if scanning {
        if ascii {
            if blink {
                "       \\  |  /       "
            } else {
                "      -- [*] --      "
            }
        } else if blink {
            "       ╲  │  ╱       "
        } else {
            "      ━━  ◈  ━━      "
        }
    } else if moving {
        if ascii {
            if blink {
                "       >>>|<<<       "
            } else {
                "        >>|<<        "
            }
        } else if blink {
            "       ▓▓▓█▓▓▓       "
        } else {
            "        ▒▓█▓▒        "
        }
    } else if ascii {
        "      [DOCKED]       "
    } else {
        "      ▪ STANDBY ▪    "
    };

    if ascii {
        vec![
            Line::from(vec![
                Span::styled(format!("          {mast_tip}          "), active),
                Span::styled("<- TELEMETRY MAST", callout),
            ]),
            Line::from(vec![
                Span::styled("       .--+--.       ", hull),
                Span::styled("<- WIDE SENSOR DISH", dim),
            ]),
            Line::from(vec![
                Span::styled("  ====[ (===) ]====  ", hull),
                Span::styled("<- SOLAR / THERMAL FINS", dim),
            ]),
            Line::from(vec![
                Span::styled("     /| [###] |\\     ", frame_s),
                Span::styled("<- PROTECTED CHASSIS", callout),
            ]),
            Line::from(vec![
                Span::styled("    /_|  ===  |_\\    ", hull),
                Span::styled("<- RELAY TRANSPONDER", dim),
            ]),
            Line::from(vec![
                Span::styled("       / | | \\       ", frame_s),
                Span::styled("<- VECTOR THRUSTERS", dim),
            ]),
            Line::from(vec![
                Span::styled(sweep_line, active),
                Span::styled(
                    if scanning {
                        "  ACTIVE SWEEP"
                    } else if moving {
                        "  TRANSIT BURN"
                    } else {
                        "  STATION HOLD"
                    },
                    dim,
                ),
            ]),
        ]
    } else {
        vec![
            Line::from(vec![
                Span::styled(format!("          {mast_tip}          "), active),
                Span::styled("◀─ TELEMETRY MAST", callout),
            ]),
            Line::from(vec![
                Span::styled("       ▄▀▀█▀▀▄       ", hull),
                Span::styled("◀─ WIDE SENSOR DISH", dim),
            ]),
            Line::from(vec![
                Span::styled("  ▄▄▄█▌ ▐███▌ ▐█▄▄▄  ", hull),
                Span::styled("◀─ THERMAL / SOLAR FINS", dim),
            ]),
            Line::from(vec![
                Span::styled("  ▀▀▀█▌ ▐█▓█▌ ▐█▀▀▀  ", frame_s),
                Span::styled("◀─ PROTECTED CHASSIS", callout),
            ]),
            Line::from(vec![
                Span::styled("      ▀▄█████▄▀      ", hull),
                Span::styled("◀─ RELAY TRANSPONDER", dim),
            ]),
            Line::from(vec![
                Span::styled("        ▀▄▀▄▀        ", frame_s),
                Span::styled("◀─ VECTOR THRUSTERS", dim),
            ]),
            Line::from(vec![
                Span::styled(sweep_line, active),
                Span::styled(
                    if scanning {
                        "  ACTIVE SWEEP"
                    } else if moving {
                        "  TRANSIT BURN"
                    } else {
                        "  STATION HOLD"
                    },
                    dim,
                ),
            ]),
        ]
    }
}

fn extractor_blueprint_lines(
    app: &App,
    planet: &PlanetSnapshot,
    blink: bool,
    ascii: bool,
    theme: Theme,
    now: Instant,
) -> Vec<Line<'static>> {
    let hull = Style::default().fg(theme.cyan_bright).bold();
    let rig_accent = match planet.extractor.loadout {
        ExtractorLoadout::Balanced => Style::default().fg(theme.cobalt_bright).bold(),
        ExtractorLoadout::Cargo => Style::default().fg(theme.gold).bold(),
        ExtractorLoadout::Shielded => Style::default().fg(theme.magenta).bold(),
    };
    let callout = Style::default().fg(theme.gold).bold();
    let active = Style::default().fg(theme.amber).bold();
    let dim = Style::default().fg(theme.muted);

    let rebuilding = planet.extractor.state == ExtractorState::Rebuilding
        && planet.extractor.rebuild_remaining_ms > 0;
    let remaining_rebuild = app.display_remaining_ms(planet.extractor.rebuild_remaining_ms, now);
    let total_rebuild = planet.extractor.loadout.rebuild_duration_ms().max(1);
    let rebuild_pct = if rebuilding {
        100_u64.saturating_sub(remaining_rebuild.saturating_mul(100) / total_rebuild)
    } else {
        100
    };

    let drill_active = planet.extractor.state == ExtractorState::Extracting;
    let moving = matches!(
        planet.extractor.state,
        ExtractorState::Outbound | ExtractorState::Returning
    );

    let rows: Vec<(&'static str, &'static str, bool)> = match (ascii, planet.extractor.loadout) {
        (true, ExtractorLoadout::Balanced) => vec![
            ("       .-----.       ", "<- TELEMETRY DOME", false),
            ("     / [=====] \\     ", "<- BALANCED FRAME", true),
            ("    | [##] [##] |    ", "<- TWIN ORE PODS", true),
            ("    |===|===|===|    ", "<- CORE COUPLING", false),
            ("      \\  | |  /      ", "<- STABILIZERS", false),
            ("        \\ | /        ", "<- ROTARY AUGER", true),
        ],
        (true, ExtractorLoadout::Cargo) => vec![
            ("       .-----.       ", "<- TELEMETRY DOME", false),
            ("  +--+/[=====]\\+--+  ", "<- +2 OUTBOARD HOPPERS", true),
            ("  |##||[##|##]||##|  ", "<- EXPANDED CARGO BAY", true),
            ("  +--+|=|===|=|+--+  ", "<- REINFORCED RACKS", true),
            ("      \\  | |  /      ", "<- HEAVY STRUTS", false),
            ("        \\ | /        ", "<- ROTARY AUGER", false),
        ],
        (true, ExtractorLoadout::Shielded) => vec![
            ("     /=========\\     ", "<- ABLATIVE CANOPY", true),
            ("   //  [=====]  \\\\   ", "<- OUTER ARMOR SHELL", true),
            ("  ||  |[#] [#]|  ||  ", "<- COMPACT ORE CELL", true),
            ("   \\\\  |=====|  //   ", "<- IMPACT BAFFLES", true),
            ("     \\===| |===/     ", "<- SHIELDED STRUTS", false),
            ("        \\ | /        ", "<- ROTARY AUGER", false),
        ],
        (false, ExtractorLoadout::Balanced) => vec![
            ("       ▄█████▄       ", "◀─ TELEMETRY DOME", false),
            ("     ▄▀███████▀▄     ", "◀─ BALANCED FRAME", true),
            ("    █▌ ▓██ ██▓ ▐█    ", "◀─ TWIN ORE PODS", true),
            ("    ▀███▀▀▀▀▀███▀    ", "◀─ CORE COUPLING", false),
            ("      ▀▄ ▐█▌ ▄▀      ", "◀─ STABILIZERS", false),
            ("         ▀▼▀         ", "◀─ ROTARY AUGER", true),
        ],
        (false, ExtractorLoadout::Cargo) => vec![
            ("       ▄█████▄       ", "◀─ TELEMETRY DOME", false),
            ("  ▄██▄▀███████▀▄██▄  ", "◀─ +2 OUTBOARD HOPPERS", true),
            ("  ████▌▓██ ██▓▐████  ", "◀─ EXPANDED CARGO BAY", true),
            ("  ▀██▀██▀▀▀▀▀██▀██▀  ", "◀─ REINFORCED RACKS", true),
            ("      ▀▄ ▐█▌ ▄▀      ", "◀─ HEAVY STRUTS", false),
            ("         ▀▼▀         ", "◀─ ROTARY AUGER", false),
        ],
        (false, ExtractorLoadout::Shielded) => vec![
            ("     ╔═════════╗     ", "◀─ ABLATIVE CANOPY", true),
            ("   ▄█▌ ███████ ▐█▄   ", "◀─ OUTER ARMOR SHELL", true),
            ("   ██▌ ▐▓█ █▓▌ ▐██   ", "◀─ COMPACT ORE CELL", true),
            ("   ▀█▌ ▀▀▀▀▀▀▀ ▐█▀   ", "◀─ IMPACT BAFFLES", true),
            ("     ╚═══▐█▌═══╝     ", "◀─ SHIELDED STRUTS", false),
            ("         ▀▼▀         ", "◀─ ROTARY AUGER", false),
        ],
    };

    let row_count = rows.len();
    // During rebuild, fill from bottom row up according to rebuild_pct.
    let built_rows = if rebuilding {
        ((rebuild_pct as usize * row_count) / 100).min(row_count)
    } else {
        row_count
    };

    let mut out = Vec::with_capacity(row_count + 1);
    for (idx, (art_row, label, is_rig_part)) in rows.into_iter().enumerate() {
        let from_bottom = row_count - 1 - idx;
        if rebuilding && from_bottom > built_rows {
            let wireframe = if ascii {
                "    . . . . . . .    "
            } else {
                "    · · · · · · ·    "
            };
            out.push(Line::from(vec![
                Span::styled(wireframe, Style::default().fg(theme.blueprint_grid)),
                Span::styled(label, Style::default().fg(theme.faint)),
            ]));
        } else if rebuilding && from_bottom == built_rows {
            let spark = match (ascii, blink) {
                (true, true) => "  *==[FABRICATING]==*",
                (true, false) => "  +==[FABRICATING]==+",
                (false, true) => "  ✦══[FABRICATING]══✦",
                (false, false) => "  ✧──[FABRICATING]──✧",
            };
            out.push(Line::from(vec![
                Span::styled(spark, active),
                Span::styled(format!("  {rebuild_pct}%"), callout),
            ]));
        } else {
            let style = if is_rig_part { rig_accent } else { hull };
            out.push(Line::from(vec![
                Span::styled(art_row, style),
                Span::styled(label, if is_rig_part { callout } else { dim }),
            ]));
        }
    }

    let footer_motion = if rebuilding {
        if ascii {
            format!(
                "  GANTRY ACTIVE // {}s  ",
                remaining_rebuild.div_ceil(1_000)
            )
        } else {
            format!(
                "  ◈ GANTRY ACTIVE // {}s",
                remaining_rebuild.div_ceil(1_000)
            )
        }
    } else if drill_active {
        if ascii {
            if blink {
                "       vvv[*]vvv     "
            } else {
                "        vv[+]vv      "
            }
            .to_owned()
        } else if blink {
            "       ▼▼▼◈▼▼▼       ".to_owned()
        } else {
            "        ▼▼◎▼▼        ".to_owned()
        }
    } else if moving {
        if ascii {
            if blink {
                "       >>>|<<<       "
            } else {
                "        >>|<<        "
            }
            .to_owned()
        } else if blink {
            "       ▓▓▓█▓▓▓       ".to_owned()
        } else {
            "        ▒▓█▓▒        ".to_owned()
        }
    } else if ascii {
        "      [BAY READY]    ".to_owned()
    } else {
        "      ▪ BAY READY ▪  ".to_owned()
    };
    out.push(Line::from(vec![
        Span::styled(footer_motion, active),
        Span::styled(
            if rebuilding {
                "  REBUILDING HULL"
            } else if drill_active {
                "  CORE EXTRACTION"
            } else if moving {
                "  TRANSIT BURN"
            } else {
                "  PRESS G TO SWAP RIG"
            },
            if rebuilding || drill_active {
                callout
            } else {
                dim
            },
        ),
    ]));
    out
}

fn scout_status_banner(app: &App, planet: &PlanetSnapshot, now: Instant) -> String {
    let region = region_name(planet, &planet.scout.region_id);
    if let Some(mission) = &planet.active_mission {
        let target = region_name(planet, &mission.target_region_id);
        let remaining = if mission.phase == ScoutState::Holding {
            "HOLDING ON STATION".to_owned()
        } else {
            let ms = app.display_remaining_ms(mission.remaining_ms, now);
            format!("{}s REMAINING", ms.div_ceil(1_000))
        };
        format!(
            "MISSION #{:03} // {:?} // {target} // {remaining}",
            mission.mission_id, mission.phase
        )
        .to_uppercase()
    } else {
        format!("STATUS // READY AT {region}").to_uppercase()
    }
}

fn extractor_status_banner(app: &App, planet: &PlanetSnapshot, now: Instant) -> String {
    if let Some(mission) = &planet.active_extraction {
        let target = region_name(planet, &mission.target_region_id);
        let ms = app.display_remaining_ms(mission.remaining_ms, now);
        format!(
            "MISSION #{:03} // {:?} // {target} // {}s REMAINING",
            mission.mission_id,
            mission.phase,
            ms.div_ceil(1_000)
        )
        .to_uppercase()
    } else if planet.extractor.state == ExtractorState::Rebuilding {
        let ms = app.display_remaining_ms(planet.extractor.rebuild_remaining_ms, now);
        format!(
            "STATUS // REBUILDING {} // {}s REMAINING",
            planet.extractor.loadout.title(),
            ms.div_ceil(1_000)
        )
        .to_uppercase()
    } else {
        format!("STATUS // READY // {}", planet.extractor.loadout.title()).to_uppercase()
    }
}

fn render_telemetry_panel(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    planet: &PlanetSnapshot,
    options: RenderOptions,
    theme: Theme,
    now: Instant,
) {
    let selected_extractor = app.probe_selection % PROBE_COUNT == 1;
    let mut lines = Vec::new();

    if selected_extractor {
        lines.push(Line::styled(
            "CHASSIS TELEMETRY //",
            Style::default().fg(theme.cyan_bright).bold(),
        ));
        lines.push(kv(
            "DESIGNATION",
            planet.extractor.designation.clone(),
            theme,
        ));
        lines.push(kv(
            "STATE",
            format!("{:?}", planet.extractor.state).to_uppercase(),
            theme,
        ));
        let location = planet
            .active_extraction
            .as_ref()
            .map(|m| region_name(planet, &m.target_region_id))
            .unwrap_or_else(|| region_name(planet, &planet.core_region_id));
        lines.push(kv("ASSIGNMENT", location, theme));
        lines.push(kv(
            "ACTIVE RIG",
            format!(
                "{} ({})",
                planet.extractor.loadout.title(),
                planet.extractor.loadout.summary()
            ),
            theme,
        ));
        if let Some(mission) = &planet.active_extraction {
            let ms = app.display_remaining_ms(mission.remaining_ms, now);
            lines.push(kv(
                "CARGO / RISK",
                format!(
                    "{} units // {:?} // {}s",
                    mission.cargo_capacity,
                    mission.risk,
                    ms.div_ceil(1_000)
                ),
                theme,
            ));
        }
        if let Some(route) = &planet.extraction_route.region_id {
            lines.push(kv("AUTO ROUTE", region_name(planet, route), theme));
        }

        lines.push(Line::raw(""));
        lines.push(Line::styled(
            "RIG LOADOUT MATRIX //",
            Style::default().fg(theme.gold).bold(),
        ));
        for loadout in ExtractorLoadout::ALL {
            let is_active = planet.extractor.loadout == loadout;
            let marker = match (options.ascii, is_active) {
                (true, true) => "> ",
                (false, true) => "▶ ",
                (_, false) => "  ",
            };
            let delta = loadout.cargo_delta();
            let cargo_str = if delta > 0 {
                format!("+{delta}")
            } else {
                format!("{delta}")
            };
            let row_text = format!(
                "{marker}{:<12} CARGO {:>2}  TRV {:>3}%  CYCLE {:>2}s  REBUILD {:>2}s",
                loadout.title(),
                cargo_str,
                loadout.travel_percent(),
                loadout.extraction_duration_ms() / 1_000,
                loadout.rebuild_duration_ms() / 1_000,
            );
            lines.push(Line::styled(
                row_text,
                if is_active {
                    Style::default().fg(theme.gold).bold()
                } else {
                    Style::default().fg(theme.muted)
                },
            ));
        }
        lines.push(Line::from(vec![
            Span::styled(" G ", theme.key_style()),
            Span::styled(
                if planet.extractor.state == ExtractorState::Ready {
                    format!(" swap to {}", planet.extractor.loadout.next().title())
                } else {
                    " rig locked until probe is ready".to_owned()
                },
                Style::default().fg(if planet.extractor.state == ExtractorState::Ready {
                    theme.text_strong
                } else {
                    theme.muted
                }),
            ),
        ]));
    } else {
        lines.push(Line::styled(
            "CHASSIS TELEMETRY //",
            Style::default().fg(theme.cyan_bright).bold(),
        ));
        lines.push(kv("DESIGNATION", planet.scout.designation.clone(), theme));
        lines.push(kv(
            "STATE",
            format!("{:?}", planet.scout.state).to_uppercase(),
            theme,
        ));
        lines.push(kv(
            "LOCATION",
            region_name(planet, &planet.scout.region_id),
            theme,
        ));
        lines.push(kv(
            "RELIABILITY",
            "Protected survey frame (failure-free)".to_owned(),
            theme,
        ));
        let queued = planet.survey_queue.queued_region_ids.len();
        lines.push(kv(
            "SURVEY QUEUE",
            if queued == 0 {
                "idle".to_owned()
            } else {
                format!("{queued} region(s) queued")
            },
            theme,
        ));
        if let Some(site_survey) = &planet.network.active_site_survey {
            let ms = app.display_remaining_ms(site_survey.remaining_ms, now);
            lines.push(kv(
                "LOCAL SCAN",
                format!("{}s remaining", ms.div_ceil(1_000)),
                theme,
            ));
        }
    }

    lines.push(Line::raw(""));
    lines.push(Line::styled(
        "ACTIVE RESEARCH & PROFILE //",
        Style::default().fg(theme.cyan_bright).bold(),
    ));
    let active_profile = planet
        .research
        .profiles
        .iter()
        .find(|p| p.active)
        .map_or("Balanced", |p| p.title.as_str());
    lines.push(kv("PROFILE", active_profile.to_owned(), theme));
    let completed: Vec<&str> = planet
        .research
        .options
        .iter()
        .filter(|o| o.completed)
        .map(|o| o.title.as_str())
        .collect();
    lines.push(kv(
        "UNLOCKS",
        if completed.is_empty() {
            "none yet".to_owned()
        } else {
            completed.join(" / ")
        },
        theme,
    ));

    lines.push(Line::raw(""));
    lines.push(Line::styled(
        "MODULAR HARDPOINTS // PLANNED",
        Style::default().fg(theme.muted).bold(),
    ));
    let slot_frame = if options.ascii {
        [
            "  +-[SLOT A: SENSOR BAY   // RESERVED]-+",
            "  +-[SLOT B: UTILITY MOUNT // RESERVED]-+",
            "  +-[SLOT C: DRIVE COUPLING// RESERVED]-+",
        ]
    } else {
        [
            "  ┌┄[SLOT A · SENSOR BAY    // RESERVED]┄┐",
            "  ┌┄[SLOT B · UTILITY MOUNT // RESERVED]┄┐",
            "  ┌┄[SLOT C · DRIVE MODULE  // RESERVED]┄┐",
        ]
    };
    for slot in slot_frame {
        lines.push(Line::styled(slot, Style::default().fg(theme.faint)));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_set(if options.ascii {
            ASCII_BORDER
        } else {
            border::ROUNDED
        })
        .style(theme.panel_style())
        .border_style(Style::default().fg(theme.border))
        .title(Span::styled(
            " TELEMETRY & LOADOUT ",
            Style::default().fg(theme.muted),
        ));
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .wrap(Wrap { trim: true })
            .scroll((app.probe_panel_scroll, 0))
            .style(theme.panel_style())
            .block(block),
        area,
    );
}

fn kv(label: &'static str, value: String, theme: Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<13}"), Style::default().fg(theme.muted)),
        Span::styled(value, Style::default().fg(theme.text_strong)),
    ])
}

fn region_name(planet: &PlanetSnapshot, region_id: &str) -> String {
    planet
        .regions
        .iter()
        .find(|region| region.id == region_id)
        .map_or_else(|| region_id.to_owned(), |region| region.designation.clone())
}
