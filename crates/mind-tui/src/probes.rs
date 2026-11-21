//! Technical Probe Bay workspace powered by the universal half-block polygon rasterizer.
//!
//! Renders full-canvas faceted schematics of the Survey Scout and Extraction Probe on a cohesive
//! deep-teal drafting field with live rig morphing (`G`), vertical scanline rebuild assembly,
//! unwrapped loadout comparison matrix, and active research/profile chips.

use crate::{app::App, cinematic, raster::blit_to_frame, theme::Theme, ui::RenderOptions};
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
    if area.width >= 92 {
        let columns = Layout::horizontal([Constraint::Percentage(52), Constraint::Percentage(48)])
            .split(area);
        render_blueprint_panel(frame, columns[0], app, planet, options, theme, now);
        render_telemetry_panel(frame, columns[1], app, planet, options, theme, now);
    } else {
        let rows =
            Layout::vertical([Constraint::Percentage(52), Constraint::Percentage(48)]).split(area);
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
        .style(theme.panel_style())
        .border_style(
            Style::default()
                .fg(theme.cyan_bright)
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

    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(4),
        Constraint::Length(2),
    ])
    .split(inner);

    // Top craft switcher tabs (uses Left/Right arrows so 1-7 stay global workspace hotkeys).
    let scout_tab = if !selected_extractor {
        Span::styled(
            format!(" {} ", planet.scout.designation.to_uppercase()),
            theme.key_style(),
        )
    } else {
        Span::styled(
            format!(" {} ", planet.scout.designation.to_uppercase()),
            Style::default().fg(theme.muted),
        )
    };
    let extractor_tab = if selected_extractor {
        Span::styled(
            format!(" {} ", planet.extractor.designation.to_uppercase()),
            theme.key_style(),
        )
    } else {
        Span::styled(
            format!(" {} ", planet.extractor.designation.to_uppercase()),
            Style::default().fg(theme.muted),
        )
    };
    let arrow_hint = if options.ascii { "<->" } else { "◀ ▶" };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            scout_tab,
            Span::styled(
                format!("  {arrow_hint}  "),
                Style::default().fg(theme.faint),
            ),
            extractor_tab,
        ]))
        .alignment(Alignment::Center)
        .style(theme.panel_style()),
        chunks[0],
    );

    // Center: Full-canvas half-block polygon craft schematic on cohesive teal drafting field.
    let rebuilding = planet.extractor.state == ExtractorState::Rebuilding
        && planet.extractor.rebuild_remaining_ms > 0;
    let remaining_rebuild = app.display_remaining_ms(planet.extractor.rebuild_remaining_ms, now);
    let total_rebuild = planet.extractor.loadout.rebuild_duration_ms().max(1);
    let rebuild_pct = if rebuilding {
        100_u64.saturating_sub(remaining_rebuild.saturating_mul(100) / total_rebuild)
    } else {
        100
    };

    let buf = cinematic::build_probe_schematic_buffer(
        selected_extractor,
        planet.extractor.loadout,
        planet.scout.state,
        planet.extractor.state,
        rebuild_pct,
        chunks[1].width,
        chunks[1].height,
        app.globe_phase,
    );
    blit_to_frame(&buf, frame, chunks[1], app.color_tier(options));

    // Bottom: Crisp craft hardware callout + live status banner.
    let callout_line = if !selected_extractor {
        let mode = match planet.scout.state {
            ScoutState::Surveying => "ACTIVE SCAN SWEEP",
            ScoutState::Outbound | ScoutState::Returning => "TRANSIT BURN",
            ScoutState::Holding => "HOLDING ON STATION",
            ScoutState::Ready => "STANDBY AT CORE",
        };
        Line::from(vec![
            Span::styled(
                "WIDE SENSOR DISH",
                Style::default().fg(theme.cyan_bright).bold(),
            ),
            Span::styled(" // ", Style::default().fg(theme.faint)),
            Span::styled(mode, Style::default().fg(theme.amber)),
        ])
    } else if rebuilding {
        Line::from(vec![Span::styled(
            format!(
                "GANTRY ACTIVE // FABRICATING {rebuild_pct}% // {}s",
                remaining_rebuild.div_ceil(1_000)
            ),
            Style::default().fg(theme.gold).bold(),
        )])
    } else {
        let rig_callout = match planet.extractor.loadout {
            ExtractorLoadout::Balanced => "BALANCED FRAME // TWIN ORE PODS",
            ExtractorLoadout::Cargo => "+2 OUTBOARD HOPPERS // EXPANDED BAY",
            ExtractorLoadout::Shielded => "ABLATIVE CANOPY // SHIELD PLATING",
        };
        Line::from(vec![
            Span::styled(rig_callout, Style::default().fg(theme.cyan_bright).bold()),
            Span::styled(" // ", Style::default().fg(theme.faint)),
            Span::styled(
                if planet.extractor.state == ExtractorState::Ready {
                    "G SWAP RIG"
                } else {
                    "RIG LOCKED"
                },
                Style::default().fg(theme.muted),
            ),
        ])
    };

    let status_banner = if selected_extractor {
        extractor_status_banner(app, planet, now)
    } else {
        scout_status_banner(app, planet, now)
    };
    frame.render_widget(
        Paragraph::new(vec![
            callout_line,
            Line::styled(status_banner, Style::default().fg(theme.gold).bold()),
        ])
        .alignment(Alignment::Center)
        .style(theme.panel_style()),
        chunks[2],
    );
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
        lines.push(Line::styled(
            "  RIG         CAP   TRV   CYC   RBLD",
            Style::default().fg(theme.faint),
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
            let short_name = match loadout {
                ExtractorLoadout::Balanced => "Balanced",
                ExtractorLoadout::Cargo => "Cargo",
                ExtractorLoadout::Shielded => "Shielded",
            };
            let row_text = format!(
                "{marker}{:<10} {:>3}  {:>4}%  {:>3}s   {:>3}s",
                short_name,
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
        lines.push(Line::raw(""));
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
        if let Some(mission) = &planet.active_mission {
            let label = match mission.phase {
                ScoutState::Outbound => "DESTINATION",
                ScoutState::Surveying | ScoutState::Holding => "STATION",
                ScoutState::Returning => "RETURNING",
                ScoutState::Ready => "LOCATION",
            };
            lines.push(kv(
                label,
                region_name(planet, &mission.target_region_id),
                theme,
            ));
        } else {
            lines.push(kv(
                "LOCATION",
                region_name(planet, &planet.scout.region_id),
                theme,
            ));
        }
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
    let sep = if options.ascii { " | " } else { " · " };
    lines.push(Line::styled(
        format!("HARDPOINTS // SLOT A{sep}SLOT B{sep}SLOT C (RESERVED)"),
        Style::default().fg(theme.faint),
    ));

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
            .wrap(Wrap { trim: false })
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
        .find(|r| r.id == region_id)
        .map_or_else(|| region_id.to_owned(), |r| r.designation.clone())
}
