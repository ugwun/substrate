//! Late-1980s skippable CRT cold-boot / power-on sequence.
//!
//! Strictly operational diagnostics only — introduces no unapproved lore, stays Stage-0 safe,
//! respects ASCII/monochrome/TERM=dumb modes, and yields immediately to any player keypress.

use crate::{theme::Theme, ui::RenderOptions};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
use std::time::{Duration, Instant};

pub const FULL_BOOT_DURATION: Duration = Duration::from_millis(2_600);
pub const QUICK_BOOT_DURATION: Duration = Duration::from_millis(750);

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

const WORDMARK_UNICODE: [&str; 5] = [
    "▄▀▀ █ █ █▀▄ ▄▀▀ ▀█▀ █▀▄ ▄▀▄ ▀█▀ █▀▀",
    "▀▄  █ █ █▀▄ ▀▄   █  █▀▄ █▀█  █  █▀▀",
    "▀▀  ▀▀▀ ▀▀  ▀▀   ▀  ▀ ▀ ▀ ▀  ▀  ▀▀▀",
    "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━",
    "   PLANETARY ARRAY INITIALIZATION   ",
];

const WORDMARK_ASCII: [&str; 5] = [
    " ##  # # ##   ##  ### ##   #  ### ###",
    "#    # # # # #     #  # # # #  #  #  ",
    " ##  ### ##   ##   #  ##  ###  #  ###",
    "=====================================",
    "    PLANETARY ARRAY INITIALIZATION   ",
];

const POST_CHECKS: [(&str, &str, u128); 5] = [
    ("OK", "CORE BUS          LOOPBACK LINK READY", 700),
    ("OK", "SENSOR GRID       10x20 REGIONAL TOPOLOGY", 1_050),
    ("OK", "SURVEY SCOUT      STANDING BY AT CORE", 1_350),
    ("OK", "CORE FABRICATION  SUBSTRATE KERNEL ONLINE", 1_650),
    ("..", "SIGNAL COHERENCE  THREE BANDS DRIFTING", 1_950),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootMode {
    /// Full ~2.6 s CRT cold-boot sequence shown before initial Coherence onboarding.
    FullColdBoot,
    /// Brief ~0.75 s power-on crest shown when reattaching to an established daemon.
    QuickPowerOn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootIntroState {
    pub started_at: Instant,
    pub mode: BootMode,
    pub active: bool,
}

impl BootIntroState {
    pub fn start(now: Instant, mode: BootMode) -> Self {
        Self {
            started_at: now,
            mode,
            active: true,
        }
    }

    pub fn duration(self) -> Duration {
        match self.mode {
            BootMode::FullColdBoot => FULL_BOOT_DURATION,
            BootMode::QuickPowerOn => QUICK_BOOT_DURATION,
        }
    }

    pub fn elapsed(self, now: Instant) -> Duration {
        now.saturating_duration_since(self.started_at)
    }

    pub fn is_active(self, now: Instant) -> bool {
        self.active && self.elapsed(now) < self.duration()
    }

    pub fn progress(self, now: Instant) -> f64 {
        let total = self.duration().as_secs_f64().max(0.001);
        (self.elapsed(now).as_secs_f64() / total).clamp(0.0, 1.0)
    }

    pub fn skip(&mut self) {
        self.active = false;
    }
}

pub fn render_boot_intro(
    frame: &mut Frame<'_>,
    area: Rect,
    boot: BootIntroState,
    options: RenderOptions,
    theme: Theme,
    now: Instant,
) {
    frame.render_widget(Clear, area);
    frame.render_widget(Block::default().style(theme.canvas_style()), area);

    let elapsed_ms = boot.elapsed(now).as_millis();
    let progress = boot.progress(now);

    match boot.mode {
        BootMode::QuickPowerOn => render_quick_power_on(frame, area, progress, options, theme),
        BootMode::FullColdBoot => {
            render_full_cold_boot(frame, area, elapsed_ms, progress, options, theme)
        }
    }
}

fn render_quick_power_on(
    frame: &mut Frame<'_>,
    area: Rect,
    progress: f64,
    options: RenderOptions,
    theme: Theme,
) {
    let band_width = ((area.width as f64) * (progress * 1.4).min(1.0)).round() as usize;
    let bar_char = if options.ascii { '=' } else { '━' };
    let beam = bar_char.to_string().repeat(band_width.min(54));
    let crest = if options.ascii {
        "# SUBSTRATE // TERMINAL LINK ACTIVE #"
    } else {
        "◆ SUBSTRATE // TERMINAL LINK ACTIVE ◆"
    };

    let card = centered_rect(60.min(area.width.saturating_sub(2)), 9, area);
    let border_set = if options.ascii {
        ASCII_DOUBLE_BORDER
    } else {
        border::DOUBLE
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_set(border_set)
        .border_style(Style::default().fg(theme.cyan_bright).bold())
        .style(theme.blueprint_style());
    let inner = block.inner(card);
    frame.render_widget(block, card);

    let lines = vec![
        Line::raw(""),
        Line::styled(beam.clone(), Style::default().fg(theme.cyan_bright)),
        Line::styled(crest, Style::default().fg(theme.gold).bold()),
        Line::styled(beam, Style::default().fg(theme.cobalt_bright)),
        Line::raw(""),
        Line::styled(
            "PRESS ANY KEY TO SKIP",
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .style(theme.blueprint_style()),
        inner,
    );
}

fn render_full_cold_boot(
    frame: &mut Frame<'_>,
    area: Rect,
    elapsed_ms: u128,
    progress: f64,
    options: RenderOptions,
    theme: Theme,
) {
    // Phase 1 (0..380 ms): CRT horizontal raster beam expanding from center.
    if elapsed_ms < 380 {
        let beam_frac = (elapsed_ms as f64 / 380.0).clamp(0.05, 1.0);
        let beam_len = ((area.width.saturating_sub(6) as f64) * beam_frac).round() as usize;
        let glyph = if options.ascii { '-' } else { '━' };
        let core = if options.ascii { "*" } else { "◈" };
        let side = glyph.to_string().repeat(beam_len / 2);
        let line = Line::from(vec![
            Span::styled(side.clone(), Style::default().fg(theme.cyan_bright)),
            Span::styled(core, Style::default().fg(theme.gold).bold()),
            Span::styled(side, Style::default().fg(theme.cyan_bright)),
        ]);
        let mid = centered_rect(area.width, 3, area);
        frame.render_widget(
            Paragraph::new(vec![Line::raw(""), line])
                .alignment(Alignment::Center)
                .style(theme.canvas_style()),
            mid,
        );
        return;
    }

    let width = 66.min(area.width.saturating_sub(2));
    let height = 20.min(area.height.saturating_sub(2));
    let panel_area = centered_rect(width, height, area);
    let border_set = if options.ascii {
        ASCII_DOUBLE_BORDER
    } else {
        border::DOUBLE
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_set(border_set)
        .border_style(Style::default().fg(theme.cobalt_bright).bold())
        .title(Span::styled(
            " SUBSTRATE // COLD INITIALIZATION ",
            Style::default().fg(theme.gold).bold(),
        ))
        .style(theme.blueprint_style());
    let inner = block.inner(panel_area);
    frame.render_widget(block, panel_area);

    let wordmark = if options.ascii {
        WORDMARK_ASCII
    } else {
        WORDMARK_UNICODE
    };
    let reveal_rows = (((elapsed_ms.saturating_sub(380)) / 90) as usize + 1).min(wordmark.len());

    let mut lines: Vec<Line<'static>> = Vec::with_capacity(16);
    for (idx, row) in wordmark.iter().take(reveal_rows).enumerate() {
        let style = if idx < 3 {
            Style::default().fg(theme.gold).bold()
        } else if idx == 3 {
            Style::default().fg(theme.cyan_bright)
        } else {
            Style::default().fg(theme.text_strong).bold()
        };
        lines.push(Line::styled((*row).to_owned(), style));
    }
    lines.push(Line::raw(""));

    for (status, detail, threshold_ms) in POST_CHECKS {
        if elapsed_ms >= threshold_ms {
            let status_style = if status == "OK" {
                Style::default().fg(theme.cyan_bright).bold()
            } else {
                Style::default().fg(theme.amber).bold()
            };
            lines.push(Line::from(vec![
                Span::styled(format!("  [{status}] "), status_style),
                Span::styled(detail.to_owned(), Style::default().fg(theme.text_strong)),
            ]));
        }
    }

    let pct = (progress * 100.0).round().clamp(0.0, 100.0) as usize;
    let bar_cells = 24_usize;
    let filled = (bar_cells * pct) / 100;
    let (fill_ch, empty_ch) = if options.ascii {
        ('#', '.')
    } else {
        ('█', '░')
    };
    let meter = format!(
        "  INIT [{}{}] {:>3}%",
        fill_ch.to_string().repeat(filled),
        empty_ch
            .to_string()
            .repeat(bar_cells.saturating_sub(filled)),
        pct
    );
    lines.push(Line::raw(""));
    lines.push(Line::styled(meter, Style::default().fg(theme.gold).bold()));
    lines.push(Line::styled(
        "  PRESS ANY KEY TO ENTER COHERENCE",
        Style::default().fg(theme.text),
    ));

    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .style(theme.blueprint_style()),
        inner,
    );
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
