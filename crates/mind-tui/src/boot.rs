//! Full-bleed skippable Awakening / power-on sequence powered by the universal polygon rasterizer.
//!
//! Operational diagnostics only; respects ASCII/monochrome/TERM=dumb modes and yields
//! immediately to any player keypress.

use crate::{
    cinematic,
    raster::{blit_to_frame, ColorTier},
    theme::Theme,
    ui::RenderOptions,
};
use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph},
    Frame,
};
use std::time::{Duration, Instant};

pub const FULL_BOOT_DURATION: Duration = Duration::from_millis(2_600);
pub const QUICK_BOOT_DURATION: Duration = Duration::from_millis(750);

const POST_CHECKS: [(&str, &str, u128); 4] = [
    ("OK", "CORE BUS // LOOPBACK LINK", 680),
    ("OK", "SENSOR GRID // 10x20 TOPOLOGY", 1_050),
    ("OK", "SURVEY SCOUT // STANDBY", 1_400),
    ("..", "SIGNAL COHERENCE // BANDS DRIFTING", 1_800),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootMode {
    /// Full ~2.6 s planetary horizon Awakening sequence shown before initial Coherence onboarding.
    FullBoot,
    /// Brief ~0.75 s power-on limb sweep shown when reattaching to an established daemon.
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
            BootMode::FullBoot => FULL_BOOT_DURATION,
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
    tier: ColorTier,
    now: Instant,
) {
    frame.render_widget(Clear, area);
    frame.render_widget(Block::default().style(theme.canvas_style()), area);

    let elapsed_ms = boot.elapsed(now).as_millis();
    let progress = boot.progress(now);
    let quick_mode = boot.mode == BootMode::QuickPowerOn;

    // Full-bleed edge-to-edge planetary horizon backdrop.
    let mut buf =
        cinematic::build_awakening_buffer(area.width, area.height, progress as f32, quick_mode);
    // Reserve the title rectangle before drawing stars.
    let sky_mask = crate::raster::Rgb(1, 14, 22);
    if quick_mode {
        let overlay_h = 5.min(area.height);
        let overlay_y = area.height.saturating_sub(overlay_h) / 2;
        let mask_w = 52.min(area.width);
        let mask_x = area.width.saturating_sub(mask_w) / 2;
        buf.mask_rect(
            usize::from(mask_x),
            usize::from(overlay_y.saturating_sub(1)) * 2,
            usize::from(mask_x + mask_w),
            usize::from((overlay_y + overlay_h + 1).min(area.height)) * 2,
            sky_mask,
        );
    } else if elapsed_ms >= 360 {
        let title_y = (area.height / 7).max(1);
        let title_h = 4.min(area.height.saturating_sub(2));
        let mask_w = 46.min(area.width);
        let mask_x = area.width.saturating_sub(mask_w) / 2;
        buf.mask_rect(
            usize::from(mask_x),
            usize::from(title_y.saturating_sub(1)) * 2,
            usize::from(mask_x + mask_w),
            usize::from((title_y + title_h + 1).min(area.height)) * 2,
            sky_mask,
        );
    }
    blit_to_frame(&buf, frame, area, tier);

    if quick_mode {
        render_quick_overlay(frame, area, progress, options, theme);
    } else {
        render_boot_overlay(frame, area, elapsed_ms, progress, options, theme);
    }
}

fn render_quick_overlay(
    frame: &mut Frame<'_>,
    area: Rect,
    progress: f64,
    options: RenderOptions,
    theme: Theme,
) {
    let band_width =
        ((area.width.saturating_sub(12) as f64) * (progress * 1.4).min(1.0)).round() as usize;
    let bar_char = if options.ascii { '=' } else { '━' };
    let beam = bar_char.to_string().repeat(band_width.min(48));
    let crest = if options.ascii {
        "# SUBSTRATE // TERMINAL LINK ACTIVE #"
    } else {
        "◆ SUBSTRATE // TERMINAL LINK ACTIVE ◆"
    };

    let overlay_h = 5.min(area.height);
    let overlay_y = area.y + area.height.saturating_sub(overlay_h) / 2;
    let rect = Rect::new(area.x, overlay_y, area.width, overlay_h);
    let lines = vec![
        Line::styled(beam.clone(), Style::default().fg(theme.cyan_bright)),
        Line::styled(crest, Style::default().fg(theme.gold).bold()),
        Line::styled(beam, Style::default().fg(theme.cyan)),
        Line::raw(""),
        Line::styled(
            "PRESS ANY KEY TO SKIP",
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
    ];
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), rect);
}

fn render_boot_overlay(
    frame: &mut Frame<'_>,
    area: Rect,
    elapsed_ms: u128,
    progress: f64,
    options: RenderOptions,
    theme: Theme,
) {
    if elapsed_ms < 360 {
        if area.height >= 4 {
            let hint = Rect::new(area.x, area.y + area.height - 2, area.width, 1);
            frame.render_widget(
                Paragraph::new(Line::styled(
                    "PRESS ANY KEY TO SKIP",
                    Style::default().fg(theme.muted),
                ))
                .alignment(Alignment::Center),
                hint,
            );
        }
        return;
    }

    // Title block in the upper starfield.
    let title_y = area.y + (area.height / 7).max(1);
    let title_h = 4.min(area.height.saturating_sub(2));
    let title_rect = Rect::new(area.x, title_y, area.width, title_h);
    let rule = if options.ascii {
        "==================================="
    } else {
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    };
    let dot = if options.ascii { "*" } else { "◆" };
    let title_lines = vec![
        Line::from(vec![
            Span::styled(format!("{dot}  "), Style::default().fg(theme.gold).bold()),
            Span::styled("SUBSTRATE", Style::default().fg(theme.text_strong).bold()),
            Span::styled(format!("  {dot}"), Style::default().fg(theme.gold).bold()),
        ]),
        Line::styled(rule, Style::default().fg(theme.cyan_bright)),
        Line::styled(
            "SYSTEM INITIALIZATION // PLANETARY ARRAY",
            Style::default().fg(theme.amber).bold(),
        ),
    ];
    frame.render_widget(
        Paragraph::new(title_lines).alignment(Alignment::Center),
        title_rect,
    );

    // Compact floating operational check strip along the lower planetary silhouette.
    let mut status_lines: Vec<Line<'static>> = Vec::new();
    for (status, detail, threshold_ms) in POST_CHECKS {
        if elapsed_ms >= threshold_ms {
            let status_style = if status == "OK" {
                Style::default().fg(theme.cyan_bright).bold()
            } else {
                Style::default().fg(theme.gold).bold()
            };
            status_lines.push(Line::from(vec![
                Span::styled(format!("[{status}] "), status_style),
                Span::styled(detail.to_owned(), Style::default().fg(theme.text_strong)),
            ]));
        }
    }

    let pct = (progress * 100.0).round().clamp(0.0, 100.0) as usize;
    let bar_cells = 22_usize;
    let filled = (bar_cells * pct) / 100;
    let (fill_ch, empty_ch) = if options.ascii {
        ('#', '.')
    } else {
        ('█', '░')
    };
    status_lines.push(Line::raw(""));
    status_lines.push(Line::from(vec![
        Span::styled(
            format!(
                "INIT [{}{}] {:>3}%   ",
                fill_ch.to_string().repeat(filled),
                empty_ch
                    .to_string()
                    .repeat(bar_cells.saturating_sub(filled)),
                pct
            ),
            Style::default().fg(theme.gold).bold(),
        ),
        Span::styled(
            "PRESS ANY KEY TO ENTER COHERENCE",
            Style::default().fg(theme.text),
        ),
    ]));

    let bottom_h =
        (status_lines.len() as u16).min(area.height.saturating_sub(title_y - area.y + title_h + 1));
    if bottom_h >= 2 {
        let bottom_y = area.y + area.height.saturating_sub(bottom_h + 1);
        let bottom_rect = Rect::new(area.x, bottom_y, area.width, bottom_h);
        frame.render_widget(
            Paragraph::new(status_lines).alignment(Alignment::Center),
            bottom_rect,
        );
    }
}
