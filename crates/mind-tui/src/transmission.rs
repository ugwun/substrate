//! Late-1980s Discovery Transmission overlays, Archive measurement diagrams, and Chapter cards.
//!
//! All art and framing remain strictly grounded in player-visible observations and preserve full
//! ASCII and monochrome readability.

use crate::{
    cinematic,
    raster::{blit_to_frame, ColorTier},
    theme::Theme,
    ui::RenderOptions,
};
use mind_core::protocol::{ArchiveRecordSnapshot, CampaignSnapshot};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
use std::time::Duration;

const TRANSMISSION_REVEAL_MS: u128 = 2_400;
const CARRIER_LOCK_MS: u128 = 350;

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

pub fn roman_chapter(number: u8) -> &'static str {
    match number {
        1 => "I",
        2 => "II",
        3 => "III",
        4 => "IV",
        5 => "V",
        _ => "?",
    }
}

pub fn step_pips(campaign: &CampaignSnapshot, ascii: bool) -> String {
    campaign
        .steps
        .iter()
        .map(|step| match (ascii, step.complete) {
            (true, true) => '#',
            (true, false) => '.',
            (false, true) => '●',
            (false, false) => '○',
        })
        .collect()
}

/// Per-record measurement diagram, fitted to `max_width` cells. Diagram rows never wrap: when a
/// row is too wide its caption switches to a compact form, then trailing captions are dropped.
pub fn record_diagram(
    record_id: &str,
    ascii: bool,
    phase: f64,
    theme: Theme,
    max_width: usize,
) -> Vec<Line<'static>> {
    fit_diagram(
        record_diagram_full(record_id, ascii, phase, theme),
        max_width,
    )
}

const COMPACT_CAPTIONS: &[(&str, &str)] = &[
    ("ONE SOURCE SIGNATURE", "ONE SIGNATURE"),
    ("SYM·WARM·ENC·VIEW", "4 TRAITS"),
    ("SYM/WARM/ENC/VIEW", "4 TRAITS"),
    ("UNRELATED DEVICE", "UNRELATED"),
    ("Δ0.00 // MEASURED OFFSET", "Δ0.00 OFFSET"),
    ("d0.00 // MEASURED OFFSET", "d0.00 OFFSET"),
    ("RETAINED INTERVAL", "RETAINED"),
    ("6 RECESSED CONTROLS", "6 CONTROLS"),
    ("2 SURFACE HEIGHTS", "2 HEIGHTS"),
    ("PASSIVE ENCLOSURE", "ENCLOSURE"),
    ("STAR + CURRENT WORLD", "STAR + WORLD"),
    ("OUTER RETURNS UNRESOLVED", "OUTER RETURNS"),
    ("PAUSE CLUSTERS ALIGN ACROSS BLOCKS", "PAUSES ALIGN"),
    ("PAUSE CLUSTERS ALIGN", "PAUSES ALIGN"),
    ("BLOCK TIMING", "BLOCKS"),
    ("MIND CADENCE", "CADENCE"),
];

fn fit_diagram(lines: Vec<Line<'static>>, max_width: usize) -> Vec<Line<'static>> {
    let width_of = |line: &Line<'_>| -> usize {
        line.spans
            .iter()
            .map(|span| span.content.chars().count())
            .sum()
    };
    lines
        .into_iter()
        .map(|mut line| {
            if width_of(&line) <= max_width {
                return line;
            }
            for span in &mut line.spans {
                let mut text = span.content.to_string();
                for (long, short) in COMPACT_CAPTIONS {
                    text = text.replace(long, short);
                }
                span.content = text.into();
            }
            while width_of(&line) > max_width && line.spans.len() > 1 {
                line.spans.pop();
            }
            if width_of(&line) > max_width {
                if let Some(span) = line.spans.first_mut() {
                    let text = span.content.to_string();
                    let head: String = text.chars().take(max_width).collect();
                    let cut = head.rfind("  ").map_or(head.as_str(), |at| &head[..at]);
                    span.content = cut.trim_end().to_owned().into();
                }
            }
            line
        })
        .collect()
}

fn record_diagram_full(
    record_id: &str,
    ascii: bool,
    phase: f64,
    theme: Theme,
) -> Vec<Line<'static>> {
    let pulse = (phase * 2.2).sin() > 0.0;
    let accent = Style::default().fg(theme.cyan_bright).bold();
    let warm = Style::default().fg(theme.amber).bold();
    let dim = Style::default().fg(theme.muted);
    let faint = Style::default().fg(theme.faint);

    match (record_id, ascii) {
        ("observation_001", false) => {
            let gap = if pulse { "▪" } else { "▫" };
            vec![
                Line::from(vec![
                    Span::styled("  BAND A  ━━━┫ ", dim),
                    Span::styled(gap, warm),
                    Span::styled(" ┣━━━  LOCKED", accent),
                ]),
                Line::from(vec![
                    Span::styled("  BAND B  ━━━┫ ", dim),
                    Span::styled(gap, warm),
                    Span::styled(" ┣━━━  RETAINED INTERVAL", accent),
                ]),
                Line::from(vec![
                    Span::styled("  BAND C  ━━━┫ ", dim),
                    Span::styled(gap, warm),
                    Span::styled(" ┣━━━  Δ0.00 // MEASURED OFFSET", dim),
                ]),
            ]
        }
        ("observation_001", true) => vec![
            Line::styled("  BAND A  ---| * |---  LOCKED", accent),
            Line::styled("  BAND B  ---| * |---  RETAINED INTERVAL", accent),
            Line::styled("  BAND C  ---| * |---  d0.00 // MEASURED OFFSET", dim),
        ],
        ("observation_002", false) => vec![
            Line::from(vec![
                Span::styled("  ┌─[▪][▪][▪][▪][▪][▪]─┐", accent),
                Span::styled("  6 RECESSED CONTROLS", dim),
            ]),
            Line::from(vec![
                Span::styled("  │  ▒▒▒ THERMAL HOLD  │", warm),
                Span::styled("  2 SURFACE HEIGHTS", dim),
            ]),
            Line::styled("  └────── 1 SPAN ──────┘  PASSIVE ENCLOSURE", faint),
        ],
        ("observation_002", true) => vec![
            Line::styled("  +-[#][#][#][#][#][#]-+  6 RECESSED CONTROLS", accent),
            Line::styled("  |  ::: THERMAL HOLD  |  2 SURFACE HEIGHTS", warm),
            Line::styled("  +------ 1 SPAN ------+  PASSIVE ENCLOSURE", faint),
        ],
        ("observation_003", false) => {
            let cursor = if pulse { "│" } else { "┆" };
            vec![
                Line::from(vec![
                    Span::styled("  RECOVERED ", dim),
                    Span::styled("▅▆▂ ", accent),
                    Span::styled(cursor, warm),
                    Span::styled(" ▃▇▄ ", accent),
                    Span::styled(cursor, warm),
                    Span::styled(" ▂▅▆ ", accent),
                    Span::styled(cursor, warm),
                    Span::styled(" ▆▃", accent),
                ]),
                Line::from(vec![
                    Span::styled("  INTERNAL  ", dim),
                    Span::styled("▄▆▂ ", accent),
                    Span::styled(cursor, warm),
                    Span::styled(" ▄▆▃ ", accent),
                    Span::styled(cursor, warm),
                    Span::styled(" ▃▅▅ ", accent),
                    Span::styled(cursor, warm),
                    Span::styled(" ▅▂", accent),
                ]),
                Line::styled("            ▲ PAUSE CLUSTERS ALIGN ACROSS BLOCKS", faint),
            ]
        }
        ("observation_003", true) => vec![
            Line::styled("  RECOVERED ==- | -=- | -== | =-   BLOCK TIMING", accent),
            Line::styled("  INTERNAL  ==- | -=- | -== | =-   MIND CADENCE", accent),
            Line::styled("                ^ PAUSE CLUSTERS ALIGN", faint),
        ],
        ("observation_004", false) => vec![
            Line::from(vec![
                Span::styled("  SITE I    ", dim),
                Span::styled("██████ █████ ████ ███", accent),
                Span::styled("  SYM·WARM·ENC·VIEW", faint),
            ]),
            Line::from(vec![
                Span::styled("  SITE II   ", dim),
                Span::styled("██████ █████ ████ ███", accent),
                Span::styled("  UNRELATED DEVICE", faint),
            ]),
            Line::from(vec![
                Span::styled("  SITE III  ", dim),
                Span::styled("██████ █████ ████ ███", warm),
                Span::styled("  ONE SOURCE SIGNATURE", warm),
            ]),
        ],
        ("observation_004", true) => vec![
            Line::styled(
                "  SITE I    ###### ##### #### ###  SYM/WARM/ENC/VIEW",
                accent,
            ),
            Line::styled(
                "  SITE II   ###### ##### #### ###  UNRELATED DEVICE",
                accent,
            ),
            Line::styled(
                "  SITE III  ###### ##### #### ###  ONE SOURCE SIGNATURE",
                warm,
            ),
        ],
        ("observation_005", false) => {
            let outer_a = if pulse { "·" } else { " " };
            let outer_b = if pulse { " " } else { "·" };
            vec![
                Line::from(vec![
                    Span::styled("      ", faint),
                    Span::styled(outer_a, accent),
                    Span::styled("    ╭───────╮    ", faint),
                    Span::styled(outer_b, accent),
                    Span::styled("  PARALLAX FRAME", dim),
                ]),
                Line::from(vec![
                    Span::styled("    ", faint),
                    Span::styled("✦", warm),
                    Span::styled(" ───  │   ●   │  ", accent),
                    Span::styled(outer_a, accent),
                    Span::styled("  STAR + CURRENT WORLD", dim),
                ]),
                Line::from(vec![
                    Span::styled("      ", faint),
                    Span::styled(outer_b, accent),
                    Span::styled("    ╰───────╯    ", faint),
                    Span::styled(outer_a, accent),
                    Span::styled("  OUTER RETURNS UNRESOLVED", warm),
                ]),
            ]
        }
        ("observation_005", true) => vec![
            Line::styled("      .   +-------+   .   PARALLAX FRAME", dim),
            Line::styled("    * --- |   o   |   .   STAR + CURRENT WORLD", accent),
            Line::styled("      .   +-------+   .   OUTER RETURNS UNRESOLVED", warm),
        ],
        (_, false) => vec![Line::styled("  ◈ ARCHIVE MEASUREMENT RECORDED", accent)],
        (_, true) => vec![Line::styled("  * ARCHIVE MEASUREMENT RECORDED", accent)],
    }
}

pub struct DiscoveryTransmissionView<'a> {
    pub record: &'a ArchiveRecordSnapshot,
    pub record_index: usize,
    pub total_records: usize,
    pub source_label: &'a str,
    pub campaign: &'a CampaignSnapshot,
    pub elapsed: Duration,
    pub force_complete: bool,
    pub is_replay: bool,
    pub scroll: u16,
    pub phase: f64,
    pub options: RenderOptions,
    pub theme: Theme,
    pub tier: ColorTier,
}

/// Pre-wrap prose to a fixed display width so progressive typewriter reveal never re-flows words.
fn prewrap_lines(text: &str, max_width: usize) -> Vec<String> {
    let width = max_width.max(12);
    let mut out = Vec::new();
    for raw_para in text.split('\n') {
        if raw_para.trim().is_empty() {
            out.push(String::new());
            continue;
        }
        let mut current = String::new();
        for word in raw_para.split_whitespace() {
            let word_len = word.chars().count();
            let cur_len = current.chars().count();
            if cur_len == 0 {
                current.push_str(word);
            } else if cur_len + 1 + word_len <= width {
                current.push(' ');
                current.push_str(word);
            } else {
                out.push(std::mem::take(&mut current));
                current.push_str(word);
            }
        }
        if !current.is_empty() {
            out.push(current);
        }
    }
    out
}

/// Take the first `budget` characters across pre-wrapped lines without changing line breaks.
fn reveal_prewrapped(lines: &[String], mut budget: usize) -> Vec<String> {
    let mut visible = Vec::with_capacity(lines.len());
    for line in lines {
        if budget == 0 {
            break;
        }
        let len = line.chars().count();
        if len <= budget {
            visible.push(line.clone());
            budget = budget.saturating_sub(len + 1);
        } else {
            visible.push(line.chars().take(budget).collect());
            break;
        }
    }
    visible
}

pub fn render_discovery_transmission(
    frame: &mut Frame<'_>,
    area: Rect,
    view: DiscoveryTransmissionView<'_>,
) {
    let DiscoveryTransmissionView {
        record,
        record_index,
        total_records,
        source_label,
        campaign,
        elapsed,
        force_complete,
        is_replay,
        scroll,
        phase,
        options,
        theme,
        tier,
    } = view;

    // Truly full-screen cinematic takeover — zero background bleed-through.
    frame.render_widget(Clear, area);
    frame.render_widget(Block::default().style(theme.canvas_style()), area);

    let progress = if force_complete {
        1.0
    } else {
        (elapsed.as_millis() as f64 / TRANSMISSION_REVEAL_MS as f64).clamp(0.0, 1.0)
    };
    let obs_full = terminal_text(&record.observation, options.ascii);
    let interp_full = terminal_text(&record.interpretation, options.ascii);

    // Layout: Top telemetry bar (1 row) + Full-bleed polygon scene (~44%) + Divider (1 row)
    // + Centered scrollable editorial prose block + Bottom prompt (1 row).
    let scene_height = if area.height >= 26 {
        ((area.height as u32 * 44) / 100).clamp(8, 16) as u16
    } else if area.height >= 20 {
        6
    } else {
        0
    };
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(scene_height),
        Constraint::Length(1),
        Constraint::Min(8),
        Constraint::Length(1),
    ])
    .split(area);

    // 1. Sleek top transmission telemetry bar.
    let mut header_title = format!(
        " TRANSMISSION // RECORD {:02} OF {:02} ",
        record_index + 1,
        total_records.max(record_index + 1)
    );
    let acquiring_carrier = !force_complete && elapsed.as_millis() < CARRIER_LOCK_MS;
    let mut right_tag = if acquiring_carrier {
        let frac = (elapsed.as_millis() as f64 / CARRIER_LOCK_MS as f64).clamp(0.0, 1.0);
        let filled = (frac * 10.0).round() as usize;
        let bar: String = (0..10)
            .map(|i| {
                if options.ascii {
                    if i < filled {
                        '#'
                    } else {
                        '.'
                    }
                } else if i < filled {
                    '█'
                } else {
                    '░'
                }
            })
            .collect();
        format!("CARRIER [{bar}] ACQUIRING ")
    } else if is_replay {
        format!("SOURCE {source_label}  //  ARCHIVE REPLAY ")
    } else {
        format!("SOURCE {source_label}  //  PERMANENT RECORD ")
    };
    if header_title.chars().count() + right_tag.chars().count() >= usize::from(area.width) {
        header_title = format!(
            " TRANSMISSION {:02}/{:02} ",
            record_index + 1,
            total_records.max(record_index + 1)
        );
        right_tag = if acquiring_carrier {
            "CARRIER LOCK ".into()
        } else if is_replay {
            format!("{source_label} // REPLAY ")
        } else {
            format!("{source_label} // RECORD ")
        };
    }
    let pad = usize::from(area.width)
        .saturating_sub(header_title.chars().count() + right_tag.chars().count());
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(header_title, Style::default().fg(theme.gold).bold()),
            Span::raw(" ".repeat(pad)),
            Span::styled(
                right_tag,
                Style::default().fg(if acquiring_carrier {
                    theme.amber
                } else {
                    theme.cyan_bright
                }),
            ),
        ]))
        .style(theme.panel_style()),
        rows[0],
    );

    // 2. Full-bleed bespoke polygon scene for this approved observation.
    if scene_height > 0 {
        let buf = cinematic::build_discovery_buffer(
            &record.record_id,
            rows[1].width,
            rows[1].height,
            phase,
            progress,
        );
        blit_to_frame(&buf, frame, rows[1], tier);
    }

    // 3. Hairline rule separating visual plate from prose.
    let rule = if options.ascii { "-" } else { "─" };
    frame.render_widget(
        Paragraph::new(Line::styled(
            rule.repeat(usize::from(rows[2].width)),
            Style::default().fg(theme.border),
        ))
        .style(theme.canvas_style()),
        rows[2],
    );

    // 4. Centered editorial observation & interpretation block (pre-wrapped + scrollable).
    let text_width = rows[3].width.saturating_sub(4).clamp(32, 84);
    let text_area = centered_rect(text_width, rows[3].height, rows[3]);
    let wrap_cols = usize::from(text_area.width).max(20);

    let obs_wrapped = prewrap_lines(&obs_full, wrap_cols);
    let interp_wrapped = prewrap_lines(&interp_full, wrap_cols);
    let obs_total_chars: usize = obs_wrapped.iter().map(|l| l.chars().count() + 1).sum();
    let interp_total_chars: usize = interp_wrapped.iter().map(|l| l.chars().count() + 1).sum();
    let obs_budget = ((obs_total_chars as f64) * (progress / 0.72).min(1.0)).ceil() as usize;
    let obs_visible = reveal_prewrapped(&obs_wrapped, obs_budget);
    let interp_visible = if progress >= 0.72 {
        let interp_progress = ((progress - 0.72) / 0.28).clamp(0.0, 1.0);
        let budget = ((interp_total_chars as f64) * interp_progress).ceil() as usize;
        reveal_prewrapped(&interp_wrapped, budget)
    } else {
        Vec::new()
    };

    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                if options.ascii { "* " } else { "◈ " },
                Style::default().fg(theme.gold).bold(),
            ),
            Span::styled(
                record.title.to_uppercase(),
                Style::default().fg(theme.text_strong).bold(),
            ),
            Span::styled("  //  ", Style::default().fg(theme.faint)),
            Span::styled(
                record.medium.to_uppercase(),
                Style::default().fg(theme.cyan_bright),
            ),
        ]),
        Line::raw(""),
        Line::styled("OBSERVATION //", Style::default().fg(theme.amber).bold()),
    ];
    for line in obs_visible {
        lines.push(Line::styled(line, Style::default().fg(theme.text_strong)));
    }

    if !interp_visible.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            "CURRENT INTERPRETATION //",
            Style::default().fg(theme.cyan).bold(),
        ));
        for line in interp_visible {
            lines.push(Line::styled(line, Style::default().fg(theme.muted)));
        }
    }

    if progress >= 1.0 && rows[3].height >= 13 && !is_replay {
        lines.push(Line::raw(""));
        lines.push(Line::from(vec![
            Span::styled(
                format!(
                    "CHAPTER {} // {}  ",
                    roman_chapter(campaign.chapter_number),
                    campaign.title
                ),
                Style::default().fg(theme.cyan_bright).bold(),
            ),
            Span::styled(
                format!("[{}]", step_pips(campaign, options.ascii)),
                Style::default().fg(theme.amber),
            ),
        ]));
        for qline in prewrap_lines(&terminal_text(&campaign.question, options.ascii), wrap_cols) {
            lines.push(Line::styled(qline, Style::default().fg(theme.text)));
        }
    }

    // The heading and chapter rows are composed spans; wrap them too so narrow viewports never
    // cut "TITLE // MEDIUM" mid-word, and so scroll bounds count real rows.
    let lines = crate::textwrap::hang_wrap(lines, wrap_cols);
    let total_lines = lines.len() as u16;
    let view_h = text_area.height;
    let max_scroll = total_lines.saturating_sub(view_h);
    let clamped_scroll = scroll.min(max_scroll);
    let has_more_below = clamped_scroll < max_scroll;
    let has_more_above = clamped_scroll > 0;

    frame.render_widget(
        Paragraph::new(lines)
            .scroll((clamped_scroll, 0))
            .style(theme.canvas_style()),
        text_area,
    );

    // 5. Crisp bottom action prompt with scroll continuation cue on compact terminals.
    let compact_footer = area.width < 68;
    let footer_prompt = if progress < 1.0 {
        if compact_footer {
            "SPACE/ENTER  reveal"
        } else {
            "SPACE / ENTER  reveal complete transmission"
        }
    } else if is_replay {
        if compact_footer {
            "ENTER/ESC  Archive"
        } else {
            "ENTER / ESC  return to Archive"
        }
    } else if compact_footer {
        "ENTER/ESC  file & continue"
    } else {
        "ENTER / ESC  file permanently in Archive and continue"
    };
    let scroll_cue = if has_more_below && has_more_above {
        if options.ascii {
            "  [UP/DN scroll]"
        } else {
            "  [↑/↓ scroll]"
        }
    } else if has_more_below {
        if options.ascii {
            "  v MORE [DN]"
        } else {
            "  ▼ MORE [↓]"
        }
    } else if has_more_above {
        if options.ascii {
            "  ^ TOP [UP]"
        } else {
            "  ▲ TOP [↑]"
        }
    } else {
        ""
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(footer_prompt, Style::default().fg(theme.cyan_bright).bold()),
            Span::styled(scroll_cue, Style::default().fg(theme.gold).bold()),
            Span::styled("   ", Style::default()),
            Span::styled(
                if !is_replay && scroll_cue.is_empty() {
                    campaign
                        .next_action_key
                        .as_ref()
                        .map_or_else(String::new, |key| format!("NEXT [{key}]"))
                } else {
                    String::new()
                },
                Style::default().fg(theme.amber),
            ),
        ]))
        .alignment(Alignment::Center)
        .style(theme.panel_style()),
        rows[4],
    );
}

pub fn render_chapter_card(
    frame: &mut Frame<'_>,
    area: Rect,
    campaign: &CampaignSnapshot,
    options: RenderOptions,
    theme: Theme,
) {
    let width = area.width.saturating_sub(6).clamp(40, 78).min(area.width);
    let inner_w = usize::from(width.saturating_sub(2));
    // Keep the workspace header and two-row footer clear (as Help does); then two border rows
    // plus the fixed close prompt.
    let max_body = usize::from(area.height.saturating_sub(4)).saturating_sub(3);

    let heading = |text: &'static str, color| Line::styled(text, Style::default().fg(color).bold());
    let question = Line::styled(
        terminal_text(&campaign.question, options.ascii),
        Style::default().fg(theme.text_strong).bold(),
    );
    let voice = Line::styled(
        terminal_text(&campaign.voice, options.ascii),
        Style::default().fg(theme.text),
    );
    let steps: Vec<Line<'static>> = campaign
        .steps
        .iter()
        .map(|step| {
            let marker = match (options.ascii, step.complete) {
                (true, true) => "[#] ",
                (true, false) => "[ ] ",
                (false, true) => " ●  ",
                (false, false) => " ○  ",
            };
            Line::from(vec![
                Span::styled(
                    marker,
                    Style::default().fg(if step.complete {
                        theme.success
                    } else {
                        theme.amber
                    }),
                ),
                Span::styled(
                    terminal_text(&step.label, options.ascii),
                    Style::default().fg(if step.complete {
                        theme.muted
                    } else {
                        theme.text_strong
                    }),
                ),
            ])
        })
        .collect();
    let next = Line::from(vec![
        Span::styled("NEXT  ", Style::default().fg(theme.amber).bold()),
        Span::styled(
            terminal_text(&campaign.next_action, options.ascii),
            Style::default().fg(theme.text_strong),
        ),
    ]);

    // Progressively compact so NEXT and the steps always fit: full card, then without the
    // narrative voice, then without headings and spacers, then without the question.
    // Each step on one row (truncated), for the tightest layout.
    let steps_one_row: Vec<Line<'static>> = campaign
        .steps
        .iter()
        .map(|step| {
            let marker = match (options.ascii, step.complete) {
                (true, true) => "[#] ",
                (true, false) => "[ ] ",
                (false, true) => " ●  ",
                (false, false) => " ○  ",
            };
            Line::from(vec![
                Span::styled(
                    marker,
                    Style::default().fg(if step.complete {
                        theme.success
                    } else {
                        theme.amber
                    }),
                ),
                Span::styled(
                    crate::ui::truncate_for_width(
                        &terminal_text(&step.label, options.ascii),
                        inner_w.saturating_sub(4),
                        options.ascii,
                    ),
                    Style::default().fg(if step.complete {
                        theme.muted
                    } else {
                        theme.text_strong
                    }),
                ),
            ])
        })
        .collect();
    let layouts: [&dyn Fn() -> Vec<Line<'static>>; 6] = [
        &|| {
            let mut lines = vec![
                heading("OPEN QUESTION //", theme.amber),
                question.clone(),
                Line::raw(""),
                voice.clone(),
                Line::raw(""),
                heading("CHAPTER STEPS //", theme.cyan),
            ];
            lines.extend(steps.iter().cloned());
            lines.extend([Line::raw(""), next.clone()]);
            lines
        },
        &|| {
            let mut lines = vec![
                heading("OPEN QUESTION //", theme.amber),
                question.clone(),
                Line::raw(""),
                heading("CHAPTER STEPS //", theme.cyan),
            ];
            lines.extend(steps.iter().cloned());
            lines.extend([Line::raw(""), next.clone()]);
            lines
        },
        &|| {
            let mut lines = vec![question.clone()];
            lines.extend(steps.iter().cloned());
            lines.push(next.clone());
            lines
        },
        &|| {
            // Tightest form: the question shrinks to one line so it is never lost entirely.
            let mut lines = vec![Line::styled(
                crate::ui::truncate_for_width(
                    &terminal_text(&campaign.question, options.ascii),
                    inner_w,
                    options.ascii,
                ),
                Style::default().fg(theme.text_strong).bold(),
            )];
            lines.extend(steps.iter().cloned());
            lines.push(next.clone());
            lines
        },
        &|| {
            let mut lines = vec![Line::styled(
                crate::ui::truncate_for_width(
                    &terminal_text(&campaign.question, options.ascii),
                    inner_w,
                    options.ascii,
                ),
                Style::default().fg(theme.text_strong).bold(),
            )];
            lines.extend(steps_one_row.iter().cloned());
            lines.push(next.clone());
            lines
        },
        &|| {
            // Smallest: the question and the next action, which matter most.
            vec![
                Line::styled(
                    crate::ui::truncate_for_width(
                        &terminal_text(&campaign.question, options.ascii),
                        inner_w,
                        options.ascii,
                    ),
                    Style::default().fg(theme.text_strong).bold(),
                ),
                next.clone(),
            ]
        },
    ];
    let cue_text = match (inner_w >= 46, options.ascii) {
        (true, true) => "... enlarge the terminal for the full briefing",
        (true, false) => "… enlarge the terminal for the full briefing",
        (false, true) => "... larger terminal shows more",
        (false, false) => "… larger terminal shows more",
    };
    let cue = Line::styled(
        crate::ui::truncate_for_width(cue_text, inner_w, options.ascii),
        Style::default().fg(theme.muted),
    );
    let mut body = Vec::new();
    for (index, build) in layouts.iter().enumerate() {
        body = crate::textwrap::hang_wrap(build(), inner_w);
        // Any compacted layout drops text, so it must leave a row for the cue.
        let needed = body.len() + usize::from(index > 0);
        if needed <= max_body {
            if index > 0 {
                body.push(cue.clone());
            }
            break;
        }
    }
    if body.len() > max_body && max_body > 0 {
        body.truncate(max_body - 1);
        body.push(cue);
    }

    let height = u16::try_from(body.len() + 3)
        .unwrap_or(u16::MAX)
        .min(area.height);
    let popup = centered_rect(width, height, area);
    frame.render_widget(Clear, popup);

    let border_set = if options.ascii {
        ASCII_DOUBLE_BORDER
    } else {
        border::DOUBLE
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_set(border_set)
        .border_style(
            Style::default()
                .fg(theme.cyan_bright)
                .add_modifier(Modifier::BOLD),
        )
        .title(Span::styled(
            format!(
                " CHAPTER {} OF {} // {} ",
                roman_chapter(campaign.chapter_number),
                campaign.total_chapters,
                campaign.title.to_uppercase()
            ),
            Style::default()
                .fg(theme.cyan_bright)
                .add_modifier(Modifier::BOLD),
        ))
        .style(theme.raised_style());
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let chunks = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(inner);
    frame.render_widget(Paragraph::new(body).style(theme.raised_style()), chunks[0]);
    frame.render_widget(
        Paragraph::new(Line::styled(
            "ENTER / ESC  continue",
            Style::default().fg(theme.cyan_bright).bold(),
        ))
        .alignment(Alignment::Center)
        .style(theme.raised_style()),
        chunks[1],
    );
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(height.min(area.height)),
            Constraint::Fill(1),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(width.min(area.width)),
            Constraint::Fill(1),
        ])
        .split(vertical[1])[1]
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
