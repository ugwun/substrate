//! Late-1980s Discovery Transmission overlays, Archive measurement diagrams, and Chapter cards.
//!
//! All art and framing remain strictly grounded in player-visible observations and preserve full
//! ASCII and monochrome readability.

use crate::{theme::Theme, ui::RenderOptions};
use mind_core::protocol::{ArchiveRecordSnapshot, CampaignSnapshot};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};
use std::time::Duration;

const TRANSMISSION_REVEAL_MS: u128 = 1_600;

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

pub fn record_diagram(
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
                    Span::styled(" ┣━━━  Δ0.00 // SHELTER BIAS", dim),
                ]),
            ]
        }
        ("observation_001", true) => vec![
            Line::styled("  BAND A  ---| * |---  LOCKED", accent),
            Line::styled("  BAND B  ---| * |---  RETAINED INTERVAL", accent),
            Line::styled("  BAND C  ---| * |---  d0.00 // SHELTER BIAS", dim),
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
    pub phase: f64,
    pub options: RenderOptions,
    pub theme: Theme,
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
        phase,
        options,
        theme,
    } = view;
    let width = area.width.saturating_sub(4).clamp(40, 86);
    let height = area.height.saturating_sub(2).clamp(12, 28);
    let popup = centered_rect(width, height, area);
    frame.render_widget(Clear, popup);

    let title = format!(
        " TRANSMISSION // RECORD {:02} OF {:02} ",
        record_index + 1,
        total_records.max(record_index + 1)
    );
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
                .fg(theme.amber)
                .add_modifier(Modifier::BOLD),
        )
        .title(Span::styled(
            title,
            Style::default()
                .fg(theme.amber)
                .add_modifier(Modifier::BOLD),
        ))
        .style(theme.raised_style());
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let progress = if force_complete {
        1.0
    } else {
        (elapsed.as_millis() as f64 / TRANSMISSION_REVEAL_MS as f64).clamp(0.0, 1.0)
    };
    let obs_full = terminal_text(&record.observation, options.ascii);
    let interp_full = terminal_text(&record.interpretation, options.ascii);
    let obs_chars =
        ((obs_full.chars().count() as f64) * (progress / 0.72).min(1.0)).ceil() as usize;
    let obs_visible: String = obs_full.chars().take(obs_chars).collect();
    let interp_visible = if progress >= 0.72 {
        let interp_progress = ((progress - 0.72) / 0.28).clamp(0.0, 1.0);
        let chars = ((interp_full.chars().count() as f64) * interp_progress).ceil() as usize;
        interp_full.chars().take(chars).collect()
    } else {
        String::new()
    };

    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                if options.ascii { "* " } else { "◈ " },
                Style::default().fg(theme.amber).bold(),
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
        Line::from(vec![
            Span::styled("SOURCE  ", Style::default().fg(theme.muted)),
            Span::styled(source_label.to_owned(), Style::default().fg(theme.text)),
            Span::styled("   ARCHIVE  ", Style::default().fg(theme.muted)),
            Span::styled("PERMANENT RECORD", Style::default().fg(theme.success)),
        ]),
    ];

    if inner.height >= 16 {
        lines.push(Line::raw(""));
        lines.extend(record_diagram(
            &record.record_id,
            options.ascii,
            phase,
            theme,
        ));
    }

    lines.push(Line::raw(""));
    lines.push(Line::styled(
        "OBSERVATION //",
        Style::default().fg(theme.amber).bold(),
    ));
    for paragraph in obs_visible.split('\n') {
        lines.push(Line::styled(
            paragraph.to_owned(),
            Style::default().fg(theme.text_strong),
        ));
    }

    if !interp_visible.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            "CURRENT INTERPRETATION //",
            Style::default().fg(theme.cyan).bold(),
        ));
        lines.push(Line::styled(
            interp_visible,
            Style::default().fg(theme.muted),
        ));
    }

    if progress >= 1.0 && inner.height >= 20 {
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
        lines.push(Line::styled(
            terminal_text(&campaign.question, options.ascii),
            Style::default().fg(theme.text),
        ));
    }

    let footer_prompt = if progress < 1.0 {
        "SPACE / ENTER  reveal complete transmission"
    } else {
        "ENTER / ESC  file permanently in Archive and continue"
    };

    let chunks = Layout::vertical([Constraint::Min(4), Constraint::Length(2)]).split(inner);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .style(theme.raised_style()),
        chunks[0],
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(footer_prompt, Style::default().fg(theme.cyan_bright).bold()),
            Span::styled("   ", Style::default()),
            Span::styled(
                if let Some(key) = &campaign.next_action_key {
                    format!("NEXT [{key}]")
                } else {
                    String::new()
                },
                Style::default().fg(theme.amber),
            ),
        ]))
        .alignment(Alignment::Center)
        .style(theme.raised_style()),
        chunks[1],
    );
}

pub fn render_chapter_card(
    frame: &mut Frame<'_>,
    area: Rect,
    campaign: &CampaignSnapshot,
    options: RenderOptions,
    theme: Theme,
) {
    let width = area.width.saturating_sub(6).clamp(40, 78);
    let height = (campaign.steps.len() as u16 + 13)
        .min(area.height.saturating_sub(2))
        .max(12);
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

    let mut lines = vec![
        Line::styled("OPEN QUESTION //", Style::default().fg(theme.amber).bold()),
        Line::styled(
            terminal_text(&campaign.question, options.ascii),
            Style::default().fg(theme.text_strong).bold(),
        ),
        Line::raw(""),
        Line::styled(
            terminal_text(&campaign.voice, options.ascii),
            Style::default().fg(theme.text),
        ),
        Line::raw(""),
        Line::styled("CHAPTER STEPS //", Style::default().fg(theme.cyan).bold()),
    ];
    for step in &campaign.steps {
        let marker = match (options.ascii, step.complete) {
            (true, true) => "[#] ",
            (true, false) => "[ ] ",
            (false, true) => " ●  ",
            (false, false) => " ○  ",
        };
        lines.push(Line::from(vec![
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
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled("NEXT  ", Style::default().fg(theme.amber).bold()),
        Span::styled(
            terminal_text(&campaign.next_action, options.ascii),
            Style::default().fg(theme.text_strong),
        ),
    ]));

    let chunks = Layout::vertical([Constraint::Min(4), Constraint::Length(1)]).split(inner);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .style(theme.raised_style()),
        chunks[0],
    );
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
            '—' | '–' => rendered.push('-'),
            _ if character.is_ascii() => rendered.push(character),
            _ => rendered.push('?'),
        }
    }
    rendered
}
