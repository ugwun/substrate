//! Late-1980s terminal-native visual scenes, pixel-art terrain tiles, instrument waveforms,
//! and the animated Solar Horizon observation frame.
//!
//! Every visual keeps Substrate Dark as the base palette, conveys operational state rather than
//! mere ornament, never exposes undiscovered truth, and ships equivalent ASCII/monochrome modes.

use crate::{theme::Theme, ui::RenderOptions};
use mind_core::protocol::{
    AtmosphericSeverity, BaseModuleKind, BaseSnapshot, PlanetSnapshot, RegionKnowledge,
    RegionSnapshot, SubregionClass, SubregionKnowledge, SubregionSnapshot, TerrainClass,
};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
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

/// 5-row late-1980s pixel-art regional horizon diorama for the Planet inspector.
///
/// Renders only discovered terrain, atmosphere, and player-visible presence; unknown regions
/// display interference static and never leak undiscovered truth.
pub fn region_terrain_diorama(
    region: &RegionSnapshot,
    planet: &PlanetSnapshot,
    phase: f64,
    options: RenderOptions,
    theme: Theme,
) -> Vec<Line<'static>> {
    let blink = (phase * 3.0).floor() as i64 % 2 == 0;
    let width = 34usize;

    if region.knowledge == RegionKnowledge::Unknown || region.terrain.is_none() {
        let noise_a = if options.ascii {
            if blink {
                ". : . : . : . : . : . : . : . : . "
            } else {
                ": . : . : . : . : . : . : . : . : "
            }
        } else if blink {
            "░▒░ ░▒░ ░▒░ ░▒░ ░▒░ ░▒░ ░▒░ ░▒░ ░▒"
        } else {
            "▒░▒ ▒░▒ ▒░▒ ▒░▒ ▒░▒ ▒░▒ ▒░▒ ▒░▒ ▒░"
        };
        let noise_b = if options.ascii {
            "  : .   UNRESOLVED HORIZON   . :  "
        } else {
            "  ▒░  [ UNRESOLVED HORIZON ]  ░▒  "
        };
        let noise_c = if options.ascii {
            ": . : . :  SURVEY REQUIRED  : . : "
        } else {
            "░▒░▒░   SURVEY TO RESOLVE   ▒░▒░▒░"
        };
        return vec![
            Line::styled(noise_a, Style::default().fg(theme.faint)),
            Line::styled(noise_b, Style::default().fg(theme.muted).bold()),
            Line::styled(noise_c, Style::default().fg(theme.faint)),
        ];
    }

    let terrain = region.terrain.unwrap_or(TerrainClass::Plain);
    let has_base = planet
        .network
        .bases
        .iter()
        .any(|base| base.region_id == region.id);
    let scout_here = planet.scout.region_id == region.id
        || planet
            .active_mission
            .as_ref()
            .is_some_and(|m| m.target_region_id == region.id);
    let extractor_here = planet
        .active_extraction
        .as_ref()
        .is_some_and(|m| m.target_region_id == region.id);

    let sky_base = match (options.ascii, region.atmospheric_severity, blink) {
        (true, Some(AtmosphericSeverity::Severe), true) => "  ////  //   ////   //   ////   / ",
        (true, Some(AtmosphericSeverity::Severe), false) => " ////   ///   ////  ///   ////    ",
        (true, Some(AtmosphericSeverity::Variable), true) => " .   ~ ~    .    ~ ~    .    ~ ~  ",
        (true, Some(AtmosphericSeverity::Variable), false) => "  .    ~ ~    .    ~ ~    .   ~   ",
        (true, _, _) => "  .      *       .        .    *  ",
        (false, Some(AtmosphericSeverity::Severe), true) => "  ╱╱╱╱  ╱╱   ╱╱╱╱   ╱╱   ╱╱╱╱   ╱ ",
        (false, Some(AtmosphericSeverity::Severe), false) => " ╱╱╱╱   ╱╱╱   ╱╱╱╱  ╱╱╱   ╱╱╱╱    ",
        (false, Some(AtmosphericSeverity::Variable), true) => " ·   ≈≈    ·    ≈≈    ·    ≈≈    ·",
        (false, Some(AtmosphericSeverity::Variable), false) => "  ·    ≈≈    ·    ≈≈    ·   ≈    ·",
        (false, _, _) => "  ·      ✧       ·        ·    ✧  ",
    };

    let craft_tag = if scout_here && extractor_here {
        if options.ascii {
            "[S+X]"
        } else {
            "◈ ▣"
        }
    } else if scout_here {
        if options.ascii {
            "<S>"
        } else {
            "◈"
        }
    } else if extractor_here {
        if options.ascii {
            "[X]"
        } else {
            "▣"
        }
    } else {
        ""
    };

    let max_sky_chars = width.saturating_sub(craft_tag.chars().count() + 2);
    let sky_trimmed: String = sky_base.chars().take(max_sky_chars).collect();
    let mut sky_spans = vec![Span::styled(
        sky_trimmed,
        Style::default().fg(match region.atmospheric_severity {
            Some(AtmosphericSeverity::Severe) => theme.warning,
            Some(AtmosphericSeverity::Variable) => theme.cobalt_bright,
            _ => theme.faint,
        }),
    )];
    if !craft_tag.is_empty() {
        sky_spans.push(Span::styled(
            format!(" {craft_tag}"),
            Style::default().fg(theme.gold).bold(),
        ));
    }

    let (mid, low, ground) = match (options.ascii, terrain) {
        (true, TerrainClass::Basin) => (
            "\\__                            __/",
            "   \\____    .  .  .       ____/   ",
            "========\\________________/========",
        ),
        (true, TerrainClass::Plain) => (
            "                                  ",
            "   .---.             .--.         ",
            "___/   \\_____________/  \\_________",
        ),
        (true, TerrainClass::Highland) => (
            "     /^^^^^\\         /^^^^\\       ",
            "   _/       \\_______/      \\_     ",
            "__/                          \\____",
        ),
        (true, TerrainClass::Ridge) => (
            "       /\\       /\\      /\\        ",
            "    __/  \\   __/  \\  __/  \\__     ",
            "___/      \\_/      \\/        \\____",
        ),
        (true, TerrainClass::Fractured) => (
            "  /\\   /\\     /\\   /\\    /\\  /\\   ",
            "_/  \\_/  \\___/  \\_/  \\__/  \\/  \\_ ",
            "===\\_/====\\_/====\\_/====\\_/=======",
        ),
        (true, TerrainClass::Polar) => (
            "    /====\\      /======\\    /==\\  ",
            "  _/      \\____/        \\__/    \\_",
            "===/======\\====/========\\==/======",
        ),
        (false, TerrainClass::Basin) => (
            "█▄                              ▄█",
            " ▀██▄▄       · · ·          ▄▄██▀ ",
            "▓▓▓▀▀████████████████████████▀▀▓▓▓",
        ),
        (false, TerrainClass::Plain) => (
            "                                  ",
            "   ▄▄▄               ▄▄           ",
            "▁▁▄███▄▁▁▁▁▁▂▃▂▁▁▁▁▁▄██▄▁▁▁▁▁▁▁▁▁▁",
        ),
        (false, TerrainClass::Highland) => (
            "     ▄█████▄         ▄████▄       ",
            "   ▄█████████▄▄▄▄▄▄▄████████▄     ",
            "▄▄████████████████████████████▄▄▄▄",
        ),
        (false, TerrainClass::Ridge) => (
            "       ▄█▄      ▄█▄     ▄█▄       ",
            "    ▄▄█████▄  ▄█████▄ ▄█████▄▄    ",
            "▁▁▄█████████▀████████▀█████████▄▁▁",
        ),
        (false, TerrainClass::Fractured) => (
            "  ▄█▄  ▄█▄    ▄█▄  ▄█▄   ▄█▄ ▄█▄  ",
            "▄█▀ ▀▄█▀ ▀▄▄▄█▀ ▀▄█▀ ▀▄▄█▀ ▀█▀ ▀█▄",
            "▀▀▓▓▄▀▓▓▓▄▀▓▓▓▄▀▓▓▓▄▀▓▓▓▄▀▓▓▓▄▓▓▀▀",
        ),
        (false, TerrainClass::Polar) => (
            "    ▄▀▀▀▀▄      ▄▀▀▀▀▀▀▄    ▄▀▀▄  ",
            "  ▄████████▄▄▄▄██████████▄▄██████▄",
            "▃▅██████████████████████████████▅▃",
        ),
    };

    let terrain_fg = match terrain {
        TerrainClass::Polar => theme.cobalt_bright,
        TerrainClass::Fractured | TerrainClass::Ridge => theme.amber,
        TerrainClass::Highland => theme.cyan,
        TerrainClass::Basin | TerrainClass::Plain => theme.cyan_bright,
    };

    let structure_badge = if region.core {
        if options.ascii {
            "  [CORE COMPLEX ONLINE] "
        } else {
            "  ◇ CORE COMPLEX ONLINE "
        }
    } else if has_base {
        if options.ascii {
            "  [PLANETARY NODE ACTIVE] "
        } else {
            "  ▄█◈█▄ PLANETARY NODE ACTIVE "
        }
    } else if region.relay {
        if options.ascii {
            "  [RANGE RELAY ACTIVE] "
        } else {
            "  ◈ RANGE RELAY ACTIVE "
        }
    } else {
        ""
    };

    let mut lines = vec![
        Line::from(sky_spans),
        Line::styled(mid, Style::default().fg(terrain_fg)),
        Line::styled(low, Style::default().fg(terrain_fg).bold()),
        Line::styled(ground, Style::default().fg(theme.border).bold()),
    ];
    if !structure_badge.is_empty() {
        lines.push(Line::styled(
            structure_badge,
            Style::default()
                .fg(if region.core {
                    theme.gold
                } else {
                    theme.success
                })
                .bold(),
        ));
    }
    lines
}

/// 4-row grounded planetary outpost cutaway for the Local Field inspector.
///
/// Displays surface domes, power arrays, telemetry masts, storage domes, and dish
/// modules with live powered, standby, and starved states.
pub fn local_structure_cutaway(
    site: &SubregionSnapshot,
    base: Option<&BaseSnapshot>,
    active_survey: bool,
    phase: f64,
    options: RenderOptions,
    theme: Theme,
) -> Vec<Line<'static>> {
    let blink = (phase * 3.5).floor() as i64 % 2 == 0;
    let Some(base) = base else {
        if active_survey {
            let sweep = if options.ascii {
                if blink {
                    "  =====[ SCANNING SECTOR ]=====   "
                } else {
                    "  -----[ SCANNING SECTOR ]-----   "
                }
            } else if blink {
                "  ═════◈ SCANNING SECTOR ◈═════   "
            } else {
                "  ─────◎ SCANNING SECTOR ◎─────   "
            };
            return vec![
                Line::styled(sweep, Style::default().fg(theme.gold).bold()),
                Line::styled(
                    class_silhouette_bottom(site.class, options.ascii).to_owned(),
                    Style::default().fg(theme.cyan_bright),
                ),
            ];
        }
        return Vec::new();
    };

    let mod_state = |kind: BaseModuleKind| {
        base.modules.iter().find(|m| m.kind == kind).map(|m| {
            if !m.enabled {
                (theme.muted, false)
            } else if m.active {
                (theme.cyan_bright, true)
            } else {
                (theme.warning, false)
            }
        })
    };

    let slot_glyph =
        |kind: BaseModuleKind, uni: &'static str, asc: &'static str| match mod_state(kind) {
            Some((color, active)) => {
                let text = if options.ascii { asc } else { uni };
                Span::styled(
                    text,
                    if active {
                        Style::default().fg(color).bold()
                    } else {
                        Style::default().fg(color)
                    },
                )
            }
            None => Span::styled(
                if options.ascii {
                    " .... "
                } else {
                    " ···· "
                },
                Style::default().fg(theme.faint),
            ),
        };

    let dish_pulse = if mod_state(BaseModuleKind::Observatory).is_some_and(|(_, a)| a) && blink {
        if options.ascii {
            " *O*  "
        } else {
            " ✦◉✦  "
        }
    } else if options.ascii {
        " \\O/  "
    } else {
        " ╲◉╱  "
    };
    let mast_pulse = if mod_state(BaseModuleKind::Communications).is_some_and(|(_, a)| a) && blink {
        if options.ascii {
            " ^|^  "
        } else {
            " ◈┴◈  "
        }
    } else if options.ascii {
        "  |   "
    } else {
        "  ┴   "
    };

    let roof_line = Line::from(vec![
        Span::raw(" "),
        slot_glyph(BaseModuleKind::PowerArray, " ▄▀▀▄ ", " /==\\ "),
        slot_glyph(BaseModuleKind::Communications, mast_pulse, mast_pulse),
        slot_glyph(BaseModuleKind::Depot, " ▄██▄ ", " /##\\ "),
        slot_glyph(BaseModuleKind::Workshop, " ▄▀▄  ", " /+\\  "),
        slot_glyph(BaseModuleKind::Laboratory, " ▄▀▀▄ ", " (OO) "),
        slot_glyph(BaseModuleKind::Observatory, dish_pulse, dish_pulse),
    ]);

    let body_line = Line::from(vec![
        Span::raw(" "),
        slot_glyph(BaseModuleKind::PowerArray, " ▐PP▌ ", " [PP] "),
        slot_glyph(BaseModuleKind::Communications, " ▐CC▌ ", " [CC] "),
        slot_glyph(BaseModuleKind::Depot, " ▐DD▌ ", " [DD] "),
        slot_glyph(BaseModuleKind::Workshop, " ▐WW▌ ", " [WW] "),
        slot_glyph(BaseModuleKind::Laboratory, " ▐LL▌ ", " [LL] "),
        slot_glyph(BaseModuleKind::Observatory, " ▐OO▌ ", " [OO] "),
    ]);

    let ground = if options.ascii {
        "======================================"
    } else if base.integrity <= 50 {
        "▀▓▀█▀▓▀█▀▓▀█▀▓▀█▀▓▀█▀▓▀█▀▓▀█▀▓▀█▀▓▀█▀▓"
    } else {
        "▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀"
    };
    let ground_color = if !base.connected || base.integrity <= 20 {
        theme.warning
    } else if base.integrity <= 50 {
        theme.amber
    } else {
        theme.cobalt_bright
    };
    let legend = if options.ascii {
        "  P:PWR C:COM D:DEP W:WRK L:LAB O:OBS"
    } else {
        "  P·PWR C·COM D·DEP W·WRK L·LAB O·OBS"
    };
    vec![
        roof_line,
        body_line,
        Line::styled(ground, Style::default().fg(ground_color).bold()),
        Line::styled(legend, Style::default().fg(theme.faint)),
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

/// Animated client-local parallax pullback & unresolved outer-return observation chart
/// powered by the universal half-block polygon rasterizer.
///
/// Deliberately leaves the count, types, and orbital arrangement of outer worlds unresolved.
pub fn render_system_cliffhanger_chart(
    frame: &mut Frame<'_>,
    area: Rect,
    elapsed: Duration,
    phase: f64,
    options: RenderOptions,
    theme: Theme,
    tier: crate::raster::ColorTier,
) {
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
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width < 16 || inner.height < 6 {
        return;
    }

    let elapsed_secs = elapsed.as_secs_f32();
    let buf = crate::cinematic::build_solar_horizon_buffer(
        inner.width,
        inner.height,
        elapsed_secs,
        phase,
    );
    crate::raster::blit_to_frame(&buf, frame, inner, tier);

    // Crisp minimal telemetry labels anchored dynamically outside star/world sprites with solid backing pills.
    let (star_col, star_row, corona_rows) =
        crate::cinematic::solar_horizon_star_cell(inner.width, inner.height, elapsed_secs);
    let star_text = " LOCAL STAR ";
    let star_len = star_text.len() as u16;
    let max_label_row = inner.height.saturating_sub(2).max(1);
    let (star_rel_x, star_rel_y) = if star_row + corona_rows < max_label_row {
        (
            star_col
                .saturating_sub(star_len / 2)
                .min(inner.width.saturating_sub(star_len)),
            star_row + corona_rows + 1,
        )
    } else if star_row >= corona_rows + 2 {
        (
            star_col
                .saturating_sub(star_len / 2)
                .min(inner.width.saturating_sub(star_len)),
            star_row - corona_rows - 1,
        )
    } else {
        (
            star_col
                .saturating_sub(star_len / 2)
                .min(inner.width.saturating_sub(star_len)),
            max_label_row,
        )
    };
    let star_label_x = inner.x + star_rel_x;
    let star_label_y = inner.y + star_rel_y.min(max_label_row);
    let star_w = star_len.min((inner.x + inner.width).saturating_sub(star_label_x));
    if star_w >= 8 {
        let star_rect = Rect::new(star_label_x, star_label_y, star_w, 1);
        frame.render_widget(ratatui::widgets::Clear, star_rect);
        frame.render_widget(
            Paragraph::new(Line::styled(
                star_text,
                theme.canvas_style().fg(theme.gold).bold(),
            )),
            star_rect,
        );
    }

    let (world_col, world_row) =
        crate::cinematic::solar_horizon_world_cell(inner.width, inner.height, elapsed_secs, phase);
    let cw_text = " CURRENT WORLD ";
    let label_len = cw_text.len() as u16;
    // Place label cleanly outside the world sprite + glow radius (5 cols / 3 rows clear).
    let (cw_rel_x, cw_rel_y) = if world_col + 5 + label_len <= inner.width {
        (world_col + 5, world_row)
    } else if world_col >= label_len + 5 {
        (world_col - label_len - 4, world_row)
    } else {
        let clamped_x = world_col
            .saturating_sub(label_len / 2)
            .min(inner.width.saturating_sub(label_len));
        let below_y = if world_row + 3 <= max_label_row {
            world_row + 3
        } else {
            world_row.saturating_sub(3).max(1)
        };
        (clamped_x, below_y)
    };
    let cw_x = inner.x + cw_rel_x;
    let cw_y = inner.y + cw_rel_y.clamp(1, max_label_row);
    let cw_w = label_len.min((inner.x + inner.width).saturating_sub(cw_x));
    if cw_w >= 10 {
        let cw_rect = Rect::new(cw_x, cw_y, cw_w, 1);
        frame.render_widget(ratatui::widgets::Clear, cw_rect);
        frame.render_widget(
            Paragraph::new(Line::styled(
                cw_text,
                theme.canvas_style().fg(theme.cyan_bright).bold(),
            )),
            cw_rect,
        );
    }

    // Bottom legend bar inside the observation frame on a solid clear band.
    let bot_y = inner.y + inner.height.saturating_sub(1);
    let bot_rect = Rect::new(inner.x, bot_y, inner.width, 1);
    let (left_txt, right_txt) = if inner.width >= 48 {
        (" REFERENCE TRACK", "OUTER RETURNS // UNRESOLVED ")
    } else {
        (" REF TRACK", "OUTER RETURNS UNRESOLVED ")
    };
    let pad = usize::from(inner.width).saturating_sub(left_txt.len() + right_txt.len());
    frame.render_widget(ratatui::widgets::Clear, bot_rect);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(left_txt, theme.canvas_style().fg(theme.muted)),
            Span::styled(" ".repeat(pad), theme.canvas_style()),
            Span::styled(right_txt, theme.canvas_style().fg(theme.cyan)),
        ]))
        .style(theme.canvas_style()),
        bot_rect,
    );
}

/// Fast non-blocking 1980s CRT interleaved horizontal raster-wipe overlay (~180 ms).
///
/// Renders a crisp cyan/gold retrace band sweeping top-to-bottom across the frame without
/// hiding authoritative content once completed.
pub fn render_crt_wipe(
    frame: &mut Frame<'_>,
    area: Rect,
    progress: f64,
    options: RenderOptions,
    theme: Theme,
) {
    if area.width < 4 || area.height < 4 || progress >= 1.0 {
        return;
    }
    let progress = progress.clamp(0.0, 1.0);
    let sweep_y = area.y + ((area.height as f64) * progress).floor() as u16;
    let lead_glyph = if options.ascii { '=' } else { '━' };
    let trail_glyph = if options.ascii { '-' } else { '░' };
    let width = usize::from(area.width);

    let lead_style = if theme.no_color() {
        Style::default().add_modifier(Modifier::BOLD | Modifier::REVERSED)
    } else {
        Style::default()
            .fg(theme.gold)
            .bg(theme.blueprint)
            .add_modifier(Modifier::BOLD)
    };
    let trail_style = if theme.no_color() {
        Style::default()
    } else {
        Style::default().fg(theme.cyan_bright).bg(theme.blueprint)
    };

    if sweep_y < area.y + area.height {
        let lead_rect = Rect::new(area.x, sweep_y, area.width, 1);
        frame.render_widget(
            Paragraph::new(lead_glyph.to_string().repeat(width)).style(lead_style),
            lead_rect,
        );
    }
    if sweep_y > area.y {
        let trail_y = sweep_y - 1;
        let trail_rect = Rect::new(area.x, trail_y, area.width, 1);
        frame.render_widget(
            Paragraph::new(trail_glyph.to_string().repeat(width)).style(trail_style),
            trail_rect,
        );
    }
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
    // Pre-wrap so indented lines keep their indent on continuation rows.
    let lines = crate::textwrap::hang_wrap(lines, usize::from(area.width.saturating_sub(2)));
    frame.render_widget(
        Paragraph::new(lines)
            .style(theme.raised_style())
            .block(block),
        area,
    );
}
