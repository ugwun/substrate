//! Late-1980s terminal-native visual scenes, pixel-art terrain tiles, instrument waveforms,
//! and the animated Solar Horizon observation frame.
//!
//! Every visual keeps Substrate Dark as the base palette, conveys operational state rather than
//! mere ornament, never exposes undiscovered truth, and ships equivalent ASCII/monochrome modes.

use crate::{theme::Theme, ui::RenderOptions};
use mind_core::protocol::{
    BaseModuleKind, BaseSnapshot, SubregionClass, SubregionKnowledge, SubregionSnapshot,
};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    symbols::{border, Marker},
    text::{Line, Span},
    widgets::{
        canvas::{Canvas, Circle, Line as CanvasLine},
        Block, Borders, Paragraph, Wrap,
    },
    Frame,
};
use std::time::Duration;

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

const PULLBACK_DURATION_SECS: f64 = 2.6;

/// Parameters for rendering a local sector pixel-art tile.
pub struct LocalSectorArtParams<'a> {
    pub site: &'a SubregionSnapshot,
    pub base: Option<&'a BaseSnapshot>,
    pub active_survey: bool,
    pub scout_present: bool,
    pub selected: bool,
    pub phase: f64,
    pub options: RenderOptions,
    pub theme: Theme,
}

/// Multi-row pixel-art local terrain tile with optional survey sweep or node sprite.
pub fn local_sector_art(params: LocalSectorArtParams<'_>) -> Vec<Line<'static>> {
    let LocalSectorArtParams {
        site,
        base,
        active_survey,
        scout_present,
        selected,
        phase,
        options,
        theme,
    } = params;
    let blink = (phase * 3.0).sin() > 0.0;
    let terrain_fg = if selected {
        theme.cyan_bright
    } else if site.knowledge == SubregionKnowledge::Unknown {
        theme.faint
    } else if site.knowledge == SubregionKnowledge::Detected {
        theme.cyan
    } else {
        theme.text
    };
    let terrain_style = Style::default().fg(terrain_fg).bold();
    let accent_style = Style::default().fg(theme.amber).bold();
    let muted_style = Style::default().fg(theme.muted);

    if site.knowledge == SubregionKnowledge::Unknown {
        return if options.ascii {
            vec![
                Line::styled(". : . : .", Style::default().fg(theme.faint)),
                Line::styled(": . ? . :", Style::default().fg(theme.muted)),
            ]
        } else if blink {
            vec![
                Line::styled("░ ▒ ░ ▒ ░", Style::default().fg(theme.faint)),
                Line::styled("▒ ░ · ░ ▒", Style::default().fg(theme.faint)),
            ]
        } else {
            vec![
                Line::styled("▒ ░ ▒ ░ ▒", Style::default().fg(theme.faint)),
                Line::styled("░ ▒ · ▒ ░", Style::default().fg(theme.faint)),
            ]
        };
    }

    // Active focused-survey scan overlay takes top row priority.
    if active_survey {
        let sweep = if options.ascii {
            if blink {
                "===[*]=== "
            } else {
                "---<+>--- "
            }
        } else if blink {
            "═══◈═══"
        } else {
            "───◎───"
        };
        let silhouette = class_silhouette_bottom(site.class, options.ascii);
        return vec![
            Line::styled(sweep.trim().to_owned(), accent_style),
            Line::styled(silhouette.to_owned(), terrain_style),
        ];
    }

    // Occupied base sprite on top row + terrain foundation beneath.
    if let Some(base) = base {
        let sprite = base_sprite(base, blink, options.ascii);
        let sprite_color = if !base.connected || base.integrity <= 20 {
            theme.warning
        } else {
            theme.success
        };
        let modules = module_strip(base, options.ascii);
        return vec![
            Line::styled(sprite, Style::default().fg(sprite_color).bold()),
            Line::styled(modules, muted_style),
        ];
    }

    let (top, bottom) = match (options.ascii, site.class) {
        (true, Some(SubregionClass::ShelteredShelf)) => (" /^^^^^\\ ", "_|_____|_"),
        (true, Some(SubregionClass::BasinFloor)) => ("\\       /", " \\_____/ "),
        (true, Some(SubregionClass::OpenPlain)) => ("  .   .  ", "_________"),
        (true, Some(SubregionClass::ExposedRidge)) => ("   /^\\   ", "_/ /|\\ \\_"),
        (true, Some(SubregionClass::FractureField)) => (" /\\   /\\ ", "/  \\_/  \\"),
        (true, Some(SubregionClass::PolarMargin)) => ("  /^^^\\  ", "_/== ==\\_"),
        (true, None) => ("  . ? .  ", "  - - -  "),
        (false, Some(SubregionClass::ShelteredShelf)) => (" ▄█████▄ ", "▂█▀   ▀█▂"),
        (false, Some(SubregionClass::BasinFloor)) => ("█▄     ▄█", " ▀█████▀ "),
        (false, Some(SubregionClass::OpenPlain)) => ("  ·   ·  ", "▁▁▂▃▂▁▁▁▁"),
        (false, Some(SubregionClass::ExposedRidge)) => ("   ▄█▄   ", "▁▄█▀█▀█▄▁"),
        (false, Some(SubregionClass::FractureField)) => ("▄█▄   ▄█▄", "▀ ▀▄▁▄▀ ▀"),
        (false, Some(SubregionClass::PolarMargin)) => ("  ▄▀▀▀▄  ", "▃▅█████▅▃"),
        (false, None) => (" ░░░░░░░ ", " · · · · "),
    };

    if site.knowledge == SubregionKnowledge::Detected && scout_present {
        let ping = if options.ascii {
            if blink {
                "+ "
            } else {
                ". "
            }
        } else if blink {
            "◎ "
        } else {
            "· "
        };
        return vec![
            Line::from(vec![
                Span::styled(ping, accent_style),
                Span::styled(top.trim().to_owned(), terrain_style),
            ]),
            Line::styled(bottom.to_owned(), terrain_style),
        ];
    }

    if site
        .deposit
        .as_ref()
        .is_some_and(|deposit| deposit.remaining > 0)
    {
        let glint = if options.ascii { "*" } else { "◆" };
        return vec![
            Line::from(vec![
                Span::styled(top.trim().to_owned(), terrain_style),
                Span::styled(format!(" {glint}"), accent_style),
            ]),
            Line::styled(bottom.to_owned(), terrain_style),
        ];
    }

    vec![
        Line::styled(top.to_owned(), terrain_style),
        Line::styled(bottom.to_owned(), terrain_style),
    ]
}

fn class_silhouette_bottom(class: Option<SubregionClass>, ascii: bool) -> &'static str {
    match (ascii, class) {
        (true, Some(SubregionClass::ShelteredShelf)) => "_|_____|_",
        (true, Some(SubregionClass::BasinFloor)) => " \\_____/ ",
        (true, Some(SubregionClass::OpenPlain)) => "_________",
        (true, Some(SubregionClass::ExposedRidge)) => "_/ /|\\ \\_",
        (true, Some(SubregionClass::FractureField)) => "/  \\_/  \\",
        (true, Some(SubregionClass::PolarMargin)) => "_/== ==\\_",
        (true, None) => "  - - -  ",
        (false, Some(SubregionClass::ShelteredShelf)) => "▂█▀   ▀█▂",
        (false, Some(SubregionClass::BasinFloor)) => " ▀█████▀ ",
        (false, Some(SubregionClass::OpenPlain)) => "▁▁▂▃▂▁▁▁▁",
        (false, Some(SubregionClass::ExposedRidge)) => "▁▄█▀█▀█▄▁",
        (false, Some(SubregionClass::FractureField)) => "▀ ▀▄▁▄▀ ▀",
        (false, Some(SubregionClass::PolarMargin)) => "▃▅█████▅▃",
        (false, None) => " ░░░░░░░ ",
    }
}

fn base_sprite(base: &BaseSnapshot, blink: bool, ascii: bool) -> String {
    let has = |kind| base.modules.iter().any(|m| m.kind == kind && m.active);
    if ascii {
        let beacon = if has(BaseModuleKind::Observatory) {
            if blink {
                "*"
            } else {
                "+"
            }
        } else if has(BaseModuleKind::Communications) {
            if blink {
                "^"
            } else {
                "|"
            }
        } else {
            "="
        };
        format!("[={beacon}=]")
    } else {
        let beacon = if has(BaseModuleKind::Observatory) {
            if blink {
                "✦"
            } else {
                "✧"
            }
        } else if has(BaseModuleKind::Communications) {
            if blink {
                "⌖"
            } else {
                "┴"
            }
        } else {
            "▪"
        };
        format!("▄█{beacon}█▄")
    }
}

fn module_strip(base: &BaseSnapshot, ascii: bool) -> String {
    if base.modules.is_empty() {
        return if ascii {
            "FOOTHOLD".into()
        } else {
            "▪ FOOTHOLD".into()
        };
    }
    base.modules
        .iter()
        .map(|module| {
            let code = match module.kind {
                BaseModuleKind::PowerArray => 'P',
                BaseModuleKind::Communications => 'C',
                BaseModuleKind::Depot => 'D',
                BaseModuleKind::Workshop => 'W',
                BaseModuleKind::Laboratory => 'L',
                BaseModuleKind::Observatory => 'O',
            };
            if module.active {
                code.to_string()
            } else if !module.enabled {
                code.to_ascii_lowercase().to_string()
            } else if ascii {
                format!("{code}!")
            } else {
                format!("{code}·")
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Live late-1980s interference oscilloscope for Coherence onboarding.
pub fn coherence_oscilloscope(
    positions: [u8; 3],
    phase: f64,
    ascii: bool,
    theme: Theme,
) -> Line<'static> {
    let spread_ab = circular_diff(positions[0], positions[1], 12);
    let spread_bc = circular_diff(positions[1], positions[2], 12);
    let aligned = spread_ab == 0 && spread_bc == 0;
    let disorder = f64::from(spread_ab + spread_bc) / 12.0;

    let glyphs_unicode = ['▁', '▂', '▃', '▄', '▅', '▆', '▇'];
    let glyphs_ascii = ['_', '.', '-', '=', '+', '^', '#'];
    let mut wave = String::with_capacity(28);
    for step in 0..24 {
        let x = f64::from(step) * 0.45 + phase * std::f64::consts::TAU;
        let sample = if aligned {
            (x * 0.85).sin() * 0.5 + 0.5
        } else {
            let raw =
                (x * 0.85).sin() + (x * (1.7 + disorder * 2.5)).cos() * (0.35 + disorder * 0.65);
            (raw * 0.35 + 0.5).clamp(0.0, 1.0)
        };
        let idx = ((sample * 6.0).round() as usize).min(6);
        wave.push(if ascii {
            glyphs_ascii[idx]
        } else {
            glyphs_unicode[idx]
        });
    }
    Line::from(vec![
        Span::styled(
            if aligned {
                "SIGNAL LOCK  "
            } else {
                "SIGNAL DRIFT "
            },
            Style::default()
                .fg(if aligned { theme.success } else { theme.muted })
                .bold(),
        ),
        Span::styled(
            wave,
            Style::default().fg(if aligned {
                theme.cyan_bright
            } else {
                theme.amber
            }),
        ),
    ])
}

fn circular_diff(a: u8, b: u8, modulus: u8) -> u8 {
    let diff = a.abs_diff(b);
    diff.min(modulus.saturating_sub(diff))
}

/// Compact animated transit/progress instrument bar for active regional operations.
pub fn operation_telemetry_strip(
    origin: &str,
    destination: &str,
    percent: u64,
    outbound: bool,
    ascii: bool,
    theme: Theme,
) -> Line<'static> {
    const SLOTS: usize = 12;
    let filled = ((percent.min(100) as usize) * SLOTS) / 100;
    let cursor = if outbound {
        filled.min(SLOTS - 1)
    } else {
        SLOTS
            .saturating_sub(1)
            .saturating_sub(filled.min(SLOTS - 1))
    };
    let mut track = String::with_capacity(SLOTS);
    for slot in 0..SLOTS {
        if slot == cursor {
            track.push(if ascii {
                if outbound {
                    '>'
                } else {
                    '<'
                }
            } else if outbound {
                '▸'
            } else {
                '◂'
            });
        } else if (outbound && slot < cursor) || (!outbound && slot > cursor) {
            track.push(if ascii { '=' } else { '━' });
        } else {
            track.push(if ascii { '-' } else { '┄' });
        }
    }
    Line::from(vec![
        Span::styled(format!("  [{origin}] "), Style::default().fg(theme.muted)),
        Span::styled(track, Style::default().fg(theme.cyan_bright).bold()),
        Span::styled(
            format!(" [{destination}] {percent}%"),
            Style::default().fg(theme.amber).bold(),
        ),
    ])
}

/// Live twin-trace waveform displayed in Archive during the 75-second laboratory timing comparison.
pub fn archive_experiment_waveform(
    running: bool,
    percent: u64,
    phase: f64,
    ascii: bool,
    theme: Theme,
) -> Vec<Line<'static>> {
    let shift = if running {
        (phase * 16.0).floor() as usize
    } else {
        0
    };
    let pattern_u = [
        '▅', '▆', '▂', ' ', '│', ' ', '▃', '▇', '▄', ' ', '│', ' ', '▂', '▅', '▆', ' ', '│', ' ',
    ];
    let pattern_a = [
        '=', '=', '-', ' ', '|', ' ', '-', '=', '-', ' ', '|', ' ', '-', '=', '=', ' ', '|', ' ',
    ];
    let pattern = if ascii { &pattern_a } else { &pattern_u };
    let len = pattern.len();
    let trace_a: String = (0..18).map(|i| pattern[(i + shift) % len]).collect();
    let trace_b: String = (0..18)
        .map(|i| pattern[(i + shift + usize::from(!running)) % len])
        .collect();
    let trace_style = Style::default().fg(if running {
        theme.cyan_bright
    } else {
        theme.warning
    });
    vec![
        Line::from(vec![
            Span::styled("  BLOCK A  ", Style::default().fg(theme.muted)),
            Span::styled(trace_a, trace_style),
            Span::styled(
                if running {
                    format!("  LOCK {percent}%")
                } else {
                    "  PAUSED".into()
                },
                Style::default()
                    .fg(if running { theme.amber } else { theme.warning })
                    .bold(),
            ),
        ]),
        Line::from(vec![
            Span::styled("  CADENCE  ", Style::default().fg(theme.muted)),
            Span::styled(trace_b, trace_style),
        ]),
    ]
}

/// Animated client-local parallax pullback & unresolved outer-return observation chart.
///
/// Deliberately leaves the count, types, and orbital arrangement of outer worlds unresolved.
pub fn render_system_cliffhanger_chart(
    frame: &mut Frame<'_>,
    area: Rect,
    elapsed: Duration,
    phase: f64,
    options: RenderOptions,
    theme: Theme,
) {
    let pullback = (elapsed.as_secs_f64() / PULLBACK_DURATION_SECS).clamp(0.0, 1.0);
    // Smooth cubic ease-out pullback from inner world framing to local star + outer dark.
    let eased = 1.0 - (1.0 - pullback).powi(3);
    let orbit_angle = 0.62 + phase * 0.35;
    let world_x = -12.0 + 24.0 * orbit_angle.cos();
    let world_y = 24.0 * orbit_angle.sin();
    let x_half = 26.0 + 34.0 * eased;
    let y_half = 14.0 + 18.0 * eased;
    let cam_x = world_x * (1.0 - eased);
    let cam_y = world_y * (1.0 - eased);

    let marker = if options.ascii {
        Marker::Custom('.')
    } else {
        Marker::Braille
    };
    let star_marker = if options.ascii { "*" } else { "✦" };
    let world_marker = if options.ascii { "o" } else { "●" };
    let border_set = if options.ascii {
        ASCII_BORDER
    } else {
        border::ROUNDED
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_set(border_set)
        .border_style(Style::default().fg(theme.cyan_bright).bold())
        .title(Span::styled(
            " LOCAL SYSTEM // OBSERVATION FRAME ",
            Style::default().fg(theme.cyan_bright).bold(),
        ))
        .style(theme.canvas_style());

    let corona_pulse = 4.4 + (phase * std::f64::consts::TAU).sin() * 0.55;
    let sweep_angle = phase * std::f64::consts::TAU;
    let sweep_x = -12.0 + 46.0 * sweep_angle.cos();
    let sweep_y = 26.0 * sweep_angle.sin();

    // Intermittent, shifting outer return blips that never imply a fixed planet count or orbit.
    let tick_bucket = (phase * 8.0).floor() as usize;
    let return_a_visible = eased > 0.55 && !tick_bucket.is_multiple_of(3);
    let return_b_visible = eased > 0.72 && (tick_bucket % 4 == 1 || tick_bucket % 4 == 2);
    let return_c_visible = eased > 0.85 && (tick_bucket.is_multiple_of(5) || tick_bucket % 5 == 3);

    let chart = Canvas::default()
        .block(block)
        .marker(marker)
        .x_bounds([cam_x - x_half, cam_x + x_half])
        .y_bounds([cam_y - y_half, cam_y + y_half])
        .paint(move |context| {
            // Subtle parallax sweep line from the local star frame.
            if eased > 0.35 {
                context.draw(&CanvasLine {
                    x1: -12.0,
                    y1: 0.0,
                    x2: sweep_x,
                    y2: sweep_y,
                    color: theme.faint,
                });
            }
            // Local star core and pulsing corona.
            context.draw(&Circle::new(-12.0, 0.0, 2.8, theme.amber));
            context.draw(&Circle::new(-12.0, 0.0, corona_pulse, theme.warning));
            // Reference orbital path of the current world.
            context.draw(&Circle::new(-12.0, 0.0, 24.0, theme.border));
            if eased > 0.5 {
                context.draw(&Circle::new(-12.0, 0.0, 40.0, theme.faint));
            }
            context.print(
                -12.8,
                0.8,
                Line::styled(star_marker, Style::default().fg(theme.amber).bold()),
            );
            context.print(
                -18.0,
                -7.2,
                Line::styled("LOCAL STAR", Style::default().fg(theme.muted)),
            );

            // Current world marker + subtle reticle.
            context.draw(&Circle::new(world_x, world_y, 1.4, theme.cyan_bright));
            context.print(
                world_x - 0.8,
                world_y + 0.8,
                Line::styled(world_marker, Style::default().fg(theme.cyan_bright).bold()),
            );
            context.print(
                world_x + 3.2,
                world_y + 1.2,
                Line::styled(
                    "CURRENT WORLD",
                    Style::default().fg(theme.text_strong).bold(),
                ),
            );

            // Faint intermittent outer interruptions — intentionally unclassified and non-enumerated.
            let blip = if options.ascii { "?" } else { "◌" };
            if return_a_visible {
                context.print(
                    31.0,
                    11.0,
                    Line::styled(blip, Style::default().fg(theme.cyan)),
                );
            }
            if return_b_visible {
                context.print(
                    38.0,
                    -12.0,
                    Line::styled(blip, Style::default().fg(theme.amber)),
                );
            }
            if return_c_visible {
                context.print(
                    -46.0,
                    16.0,
                    Line::styled(blip, Style::default().fg(theme.cyan)),
                );
            }

            context.print(
                cam_x - x_half + 4.0,
                cam_y - y_half + 3.0,
                Line::styled("REFERENCE TRACK", Style::default().fg(theme.faint)),
            );
            context.print(
                cam_x + 4.0,
                cam_y - y_half + 3.0,
                Line::styled(
                    "OUTER RETURNS // UNRESOLVED",
                    Style::default().fg(theme.cyan),
                ),
            );
        });
    frame.render_widget(chart, area);
}

/// Atmospheric in-fiction Solar Horizon closing panel grounded in `observation_005` ("Wider Frame").
pub fn render_system_cliffhanger_panel(
    frame: &mut Frame<'_>,
    area: Rect,
    options: RenderOptions,
    theme: Theme,
) {
    let border_set = if options.ascii {
        ASCII_BORDER
    } else {
        border::DOUBLE
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_set(border_set)
        .border_style(
            Style::default()
                .fg(theme.amber)
                .add_modifier(Modifier::BOLD),
        )
        .title(Span::styled(
            " SOLAR HORIZON ",
            Style::default()
                .fg(theme.amber)
                .add_modifier(Modifier::BOLD),
        ))
        .style(theme.raised_style());

    let quote = if options.ascii {
        "\"The planet is not the whole of my reachable environment.\""
    } else {
        "“The planet is not the whole of my reachable environment.”"
    };

    let lines = if area.height < 16 {
        vec![
            Line::styled(
                "SOLAR HORIZON REACHED",
                Style::default().fg(theme.success).bold(),
            ),
            Line::styled(
                "Local star centered // current world tracked",
                Style::default().fg(theme.text),
            ),
            Line::styled(
                "Outer returns remain unresolved.",
                Style::default().fg(theme.cyan),
            ),
            Line::raw(""),
            Line::styled(quote, Style::default().fg(theme.text_strong).bold()),
            Line::raw(""),
            Line::styled(
                "PLANETARY CHAPTER COMPLETE",
                Style::default().fg(theme.amber).bold(),
            ),
            Line::styled(
                "No system-scale actions at this resolution.",
                Style::default().fg(theme.muted),
            ),
        ]
    } else {
        vec![
            Line::styled(
                "SOLAR HORIZON REACHED",
                Style::default().fg(theme.success).bold(),
            ),
            Line::raw(""),
            Line::styled("FRAME RESOLVED //", Style::default().fg(theme.cyan).bold()),
            Line::styled("  Local star centered", Style::default().fg(theme.text)),
            Line::styled("  Current world tracked", Style::default().fg(theme.text)),
            Line::raw(""),
            Line::styled(
                "BEYOND CURRENT RESOLUTION //",
                Style::default().fg(theme.cyan).bold(),
            ),
            Line::styled(
                "  Periodic outer interruptions cross the frame.",
                Style::default().fg(theme.text),
            ),
            Line::styled(
                "  Their number, types, and arrangement remain unresolved.",
                Style::default().fg(theme.muted),
            ),
            Line::raw(""),
            Line::styled(
                "ARCHIVE 05 // WIDER FRAME",
                Style::default().fg(theme.amber).bold(),
            ),
            Line::styled(quote, Style::default().fg(theme.text_strong).bold()),
            Line::raw(""),
            Line::styled(
                "PLANETARY CHAPTER COMPLETE",
                Style::default().fg(theme.amber).bold(),
            ),
            Line::styled(
                "No system-scale operations are available at planetary resolution.",
                Style::default().fg(theme.text),
            ),
            Line::raw(""),
            Line::styled(
                "Review Planet, Research, and Archive with Tab, or open Q to detach.",
                Style::default().fg(theme.muted),
            ),
        ]
    };
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .style(theme.raised_style())
            .block(block),
        area,
    );
}
