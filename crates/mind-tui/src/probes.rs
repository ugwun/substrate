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
    widgets::{Block, Borders, Paragraph},
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
    if area.width >= 76 {
        let (left_pct, right_pct) = if area.width >= 96 { (52, 48) } else { (44, 56) };
        let columns = Layout::horizontal([
            Constraint::Percentage(left_pct),
            Constraint::Percentage(right_pct),
        ])
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
    let compact_bp = area.width < 44;
    let title = if selected_extractor {
        if compact_bp {
            format!(
                " PROBE BAY // {} ",
                planet.extractor.loadout.title().to_uppercase()
            )
        } else {
            format!(
                " PROBE BAY // {} ({}) ",
                planet.extractor.designation.to_uppercase(),
                planet.extractor.loadout.title().to_uppercase()
            )
        }
    } else if compact_bp {
        " PROBE BAY // SCOUT 01 ".to_owned()
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
    // The schematic needs a tab row, at least four drawing rows and both banner rows.
    if inner.width < 12 || inner.height < 7 {
        // Too short for the schematic: name the probe, its state and assignment instead.
        let (name, state, place) = if selected_extractor {
            let state = if planet.extractor.state == ExtractorState::Rebuilding
                && planet.extractor.rebuild_remaining_ms > 0
            {
                // Keep the progress visible in the short bay too.
                let remaining =
                    app.display_remaining_ms(planet.extractor.rebuild_remaining_ms, now);
                let total = planet.extractor.loadout.rebuild_duration_ms().max(1);
                let pct = 100_u64.saturating_sub(remaining.saturating_mul(100) / total);
                format!("REBUILDING {pct}%")
            } else {
                format!("{:?}", planet.extractor.state).to_uppercase()
            };
            (
                planet.extractor.designation.clone(),
                state,
                planet.active_extraction.as_ref().map_or_else(
                    || region_name(planet, &planet.core_region_id),
                    |mission| region_name(planet, &mission.target_region_id),
                ),
            )
        } else {
            (
                planet.scout.designation.clone(),
                planet.active_mission.as_ref().map_or_else(
                    || "READY".to_owned(),
                    |mission| format!("{:?}", mission.phase).to_uppercase(),
                ),
                planet.active_mission.as_ref().map_or_else(
                    || region_name(planet, &planet.scout.region_id),
                    |mission| region_name(planet, &mission.target_region_id),
                ),
            )
        };
        let width = usize::from(inner.width);
        let fit = |text: String| -> String { text.chars().take(width).collect() };
        // With a spare row, keep the mission banner (ID, destination, countdown) in the card.
        let banner = if selected_extractor {
            extractor_status_banner(app, planet, now, width)
        } else {
            scout_status_banner(app, planet, now, width)
        };
        let mut lines = vec![
            Line::styled(fit(name), Style::default().fg(theme.cyan_bright).bold()),
            Line::styled(fit(state), Style::default().fg(theme.amber)),
            Line::styled(fit(place), Style::default().fg(theme.text)),
        ];
        if inner.height >= 4 && !banner.is_empty() {
            lines.push(Line::styled(banner, Style::default().fg(theme.gold).bold()));
        }
        frame.render_widget(Paragraph::new(lines).style(theme.panel_style()), inner);
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

    let mut buf = cinematic::build_probe_schematic_buffer(
        selected_extractor,
        planet.extractor.loadout,
        planet.scout.state,
        planet.extractor.state,
        rebuild_pct,
        chunks[1].width,
        chunks[1].height,
        app.globe_phase,
    );
    if selected_extractor {
        if let Some(morph_p) = app.rig_morph_progress(now) {
            let sweep_y = (buf.height as f32) * morph_p;
            let wf = buf.width as f32;
            buf.add_radial_glow(
                wf * 0.5,
                sweep_y,
                wf * 0.55,
                4.0,
                crate::raster::Rgb(96, 226, 230),
                0.75 * (1.0 - morph_p * 0.4),
            );
            buf.stroke_line(
                2.0,
                sweep_y,
                (wf - 2.0).max(3.0),
                sweep_y,
                crate::raster::Rgb(252, 214, 86),
                0.95,
            );
        }
    }
    blit_to_frame(&buf, frame, chunks[1], app.color_tier(options));

    // Bottom: Crisp craft hardware callout + live status banner. Each callout picks the most
    // detailed complete form that fits the panel, so a state or countdown is never cut.
    let callout_width = usize::from(chunks[2].width);
    let fit_callout = |candidates: Vec<(String, Option<String>)>| -> (String, Option<String>) {
        let width_of = |(head, tail): &(String, Option<String>)| {
            head.chars().count() + tail.as_ref().map_or(0, |tail| tail.chars().count() + 4)
        };
        let fallback = candidates.last().cloned().unwrap_or_default();
        candidates
            .into_iter()
            .find(|candidate| width_of(candidate) <= callout_width)
            .unwrap_or(fallback)
    };
    let callout_line = if !selected_extractor {
        let (long_mode, short_mode) = match planet.scout.state {
            ScoutState::Surveying => ("ACTIVE SCAN SWEEP", "SCANNING"),
            ScoutState::Outbound | ScoutState::Returning => ("TRANSIT BURN", "TRANSIT"),
            ScoutState::Holding => ("HOLDING ON STATION", "ON STATION"),
            ScoutState::Ready => ("STANDBY AT CORE", "STANDBY"),
        };
        let (head, tail) = fit_callout(vec![
            ("WIDE SENSOR DISH".into(), Some(long_mode.into())),
            ("SENSOR DISH".into(), Some(long_mode.into())),
            ("SENSOR DISH".into(), Some(short_mode.into())),
            (short_mode.into(), None),
        ]);
        let mut spans = vec![Span::styled(
            head,
            Style::default().fg(theme.cyan_bright).bold(),
        )];
        if let Some(tail) = tail {
            spans.push(Span::styled(" // ", Style::default().fg(theme.faint)));
            spans.push(Span::styled(tail, Style::default().fg(theme.amber)));
        }
        Line::from(spans)
    } else if rebuilding {
        let seconds = remaining_rebuild.div_ceil(1_000);
        let (head, _) = fit_callout(vec![
            (
                format!("GANTRY ACTIVE // FABRICATING {rebuild_pct}% // {seconds}s"),
                None,
            ),
            (format!("FABRICATING {rebuild_pct}% // {seconds}s"), None),
            (format!("FAB {rebuild_pct}% {seconds}s"), None),
        ]);
        Line::from(vec![Span::styled(
            head,
            Style::default().fg(theme.gold).bold(),
        )])
    } else {
        let (long_rig, short_rig) = match planet.extractor.loadout {
            ExtractorLoadout::Balanced => ("BALANCED FRAME // TWIN ORE PODS", "BALANCED FRAME"),
            ExtractorLoadout::Cargo => {
                ("+2 OUTBOARD HOPPERS // EXPANDED BAY", "+2 OUTBOARD HOPPERS")
            }
            ExtractorLoadout::Shielded => ("ABLATIVE CANOPY // SHIELD PLATING", "ABLATIVE CANOPY"),
        };
        let lock = if planet.extractor.state == ExtractorState::Ready {
            "G SWAP RIG"
        } else {
            "RIG LOCKED"
        };
        let (head, tail) = fit_callout(vec![
            (long_rig.into(), Some(lock.into())),
            (short_rig.into(), Some(lock.into())),
            (short_rig.into(), None),
            (lock.into(), None),
        ]);
        let mut spans = vec![Span::styled(
            head,
            Style::default().fg(theme.cyan_bright).bold(),
        )];
        if let Some(tail) = tail {
            spans.push(Span::styled(" // ", Style::default().fg(theme.faint)));
            spans.push(Span::styled(tail, Style::default().fg(theme.muted)));
        }
        Line::from(spans)
    };

    let banner_width = usize::from(chunks[2].width);
    let status_banner = if selected_extractor {
        extractor_status_banner(app, planet, now, banner_width)
    } else {
        scout_status_banner(app, planet, now, banner_width)
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

/// The longest candidate that fits `width` cells; candidates run from most to least detailed.
fn first_fit(candidates: Vec<String>, width: usize) -> String {
    let fallback = candidates.last().cloned().unwrap_or_default();
    candidates
        .into_iter()
        .find(|candidate| candidate.chars().count() <= width)
        .unwrap_or_else(|| fallback.chars().take(width).collect())
}

fn scout_status_banner(app: &App, planet: &PlanetSnapshot, now: Instant, width: usize) -> String {
    let region = region_name(planet, &planet.scout.region_id);
    if let Some(mission) = &planet.active_mission {
        let target = region_name(planet, &mission.target_region_id);
        let id = mission.mission_id;
        let phase = format!("{:?}", mission.phase).to_uppercase();
        if mission.phase == ScoutState::Holding {
            first_fit(
                vec![
                    format!("MISSION #{id:03} // {phase} // {target}"),
                    format!("#{id:03} {phase} // {target}"),
                    format!("#{id:03} {phase} {target}"),
                    format!("{phase} {target}"),
                ],
                width,
            )
        } else {
            let secs = app
                .display_remaining_ms(mission.remaining_ms, now)
                .div_ceil(1_000);
            first_fit(
                vec![
                    format!("MISSION #{id:03} // {phase} // {target} // {secs}s REMAINING"),
                    format!("#{id:03} {phase} // {target} // {secs}s"),
                    format!("#{id:03} {phase} {target} {secs}s"),
                    format!("{target} {secs}s"),
                ],
                width,
            )
        }
    } else {
        first_fit(
            vec![
                format!("STATUS // READY AT {region}"),
                format!("READY AT {region}"),
                "READY".to_owned(),
            ],
            width,
        )
    }
}

fn extractor_status_banner(
    app: &App,
    planet: &PlanetSnapshot,
    now: Instant,
    width: usize,
) -> String {
    if let Some(mission) = &planet.active_extraction {
        let target = region_name(planet, &mission.target_region_id);
        let id = mission.mission_id;
        let phase = format!("{:?}", mission.phase).to_uppercase();
        let secs = app
            .display_remaining_ms(mission.remaining_ms, now)
            .div_ceil(1_000);
        first_fit(
            vec![
                format!("MISSION #{id:03} // {phase} // {target} // {secs}s REMAINING"),
                format!("#{id:03} {phase} // {target} // {secs}s"),
                format!("#{id:03} {phase} {target} {secs}s"),
                format!("{target} {secs}s"),
            ],
            width,
        )
    } else if planet.extractor.state == ExtractorState::Rebuilding {
        let secs = app
            .display_remaining_ms(planet.extractor.rebuild_remaining_ms, now)
            .div_ceil(1_000);
        let rig = planet.extractor.loadout.title().to_uppercase();
        first_fit(
            vec![
                format!("STATUS // REBUILDING {rig} // {secs}s REMAINING"),
                format!("REBUILDING {rig} // {secs}s"),
                format!("REBUILDING {secs}s"),
            ],
            width,
        )
    } else {
        let rig = planet.extractor.loadout.title().to_uppercase();
        first_fit(
            vec![
                format!("STATUS // READY // {rig}"),
                format!("READY // {rig}"),
                "READY".to_owned(),
            ],
            width,
        )
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
            Style::default().fg(theme.cyan).bold(),
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
            } else if queued == 1 {
                "1 region queued".to_owned()
            } else {
                format!("{queued} regions queued")
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
        Style::default().fg(theme.muted),
    ));

    let inner_h = usize::from(area.height.saturating_sub(2));
    // Pre-wrap with hanging indents so values continue under the value column and the overflow
    // cue counts real rendered rows.
    let mut lines = crate::textwrap::hang_wrap(lines, usize::from(area.width.saturating_sub(2)));
    // Keep a section heading with its first row instead of stranding it on the last visible row.
    let first_scroll = usize::from(app.probe_panel_scroll).min(lines.len().saturating_sub(inner_h));
    let last_visible = first_scroll + inner_h;
    if inner_h > 1 && last_visible < lines.len() {
        let plain: String = lines[last_visible - 1]
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        if plain.trim_end().ends_with(" //") {
            lines.insert(last_visible - 1, Line::raw(""));
        }
    }
    let total_lines = lines.len();
    let max_scroll = total_lines.saturating_sub(inner_h);
    app.scroll_limits
        .probe_rows
        .set(u16::try_from(inner_h).unwrap_or(u16::MAX));
    app.scroll_limits
        .probe_panel
        .set(u16::try_from(max_scroll).unwrap_or(u16::MAX));
    let scroll = usize::from(app.probe_panel_scroll).min(max_scroll);
    let title_str = match (scroll > 0, scroll < max_scroll, options.ascii) {
        (false, false, _) => " TELEMETRY & LOADOUT ",
        (false, true, true) => " TELEMETRY & LOADOUT [PgDn] ",
        (false, true, false) => " TELEMETRY & LOADOUT [PgDn ▼] ",
        (true, false, true) => " TELEMETRY & LOADOUT [PgUp] ",
        (true, false, false) => " TELEMETRY & LOADOUT [PgUp ▲] ",
        (true, true, true) => " TELEMETRY & LOADOUT [PgUp/PgDn] ",
        (true, true, false) => " TELEMETRY & LOADOUT [PgUp/PgDn ▲▼] ",
    };
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
            title_str,
            Style::default().fg(if total_lines > inner_h {
                theme.amber
            } else {
                theme.muted
            }),
        ));
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0))
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
