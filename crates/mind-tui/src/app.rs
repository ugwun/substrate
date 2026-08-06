use mind_core::protocol::{
    DiagnosticPulseResult, EngineEvent, OnboardingCompletionResult, StatusSnapshot,
};
use std::{collections::VecDeque, time::Instant};

const EVENT_CACHE_CAPACITY: usize = 256;
pub const BAND_SLOTS: u8 = 12;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionState {
    Connecting,
    Connected,
    Disconnected(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnboardingView {
    Adjusting,
    Confirming,
    Submitting,
}

#[derive(Debug)]
pub struct App {
    pub connection: ConnectionState,
    pub snapshot: Option<StatusSnapshot>,
    pub events: VecDeque<EngineEvent>,
    pub observed_tick_rate: Option<f64>,
    pub last_update: Option<Instant>,
    pub event_scroll: usize,
    pub follow_events: bool,
    pub show_help: bool,
    pub notice: Option<String>,
    pub selected_band: usize,
    pub band_positions: [u8; 3],
    pub onboarding_view: OnboardingView,
    previous_tick_sample: Option<(u64, Instant)>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            connection: ConnectionState::Connecting,
            snapshot: None,
            events: VecDeque::new(),
            observed_tick_rate: None,
            last_update: None,
            event_scroll: 0,
            follow_events: true,
            show_help: false,
            notice: None,
            selected_band: 0,
            band_positions: [1, 5, 9],
            onboarding_view: OnboardingView::Adjusting,
            previous_tick_sample: None,
        }
    }
}

impl App {
    pub fn connected(&mut self) {
        self.connection = ConnectionState::Connected;
        self.notice = None;
    }

    pub fn connecting(&mut self) {
        self.connection = ConnectionState::Connecting;
    }

    pub fn disconnected(&mut self, message: impl Into<String>) {
        self.connection = ConnectionState::Disconnected(message.into());
        self.previous_tick_sample = None;
        self.observed_tick_rate = None;
        if self.onboarding_view == OnboardingView::Submitting {
            self.onboarding_view = OnboardingView::Confirming;
        }
    }

    pub fn apply_snapshot(&mut self, snapshot: StatusSnapshot, received_at: Instant) {
        if let Some((previous_tick, previous_at)) = self.previous_tick_sample {
            let elapsed = received_at
                .saturating_duration_since(previous_at)
                .as_secs_f64();
            if elapsed > 0.0 && snapshot.tick >= previous_tick {
                self.observed_tick_rate = Some((snapshot.tick - previous_tick) as f64 / elapsed);
            }
        }
        self.previous_tick_sample = Some((snapshot.tick, received_at));

        let mut inserted = 0;
        for event in &snapshot.recent_events {
            if self
                .events
                .iter()
                .all(|known| known.event_id != event.event_id)
            {
                self.events.push_back(event.clone());
                inserted += 1;
            }
        }
        while self.events.len() > EVENT_CACHE_CAPACITY {
            self.events.pop_front();
        }
        if self.follow_events {
            self.event_scroll = 0;
        } else {
            self.event_scroll = self.event_scroll.saturating_add(inserted);
            self.clamp_scroll();
        }

        self.last_update = Some(received_at);
        self.snapshot = Some(snapshot);
        self.connection = ConnectionState::Connected;
    }

    pub fn onboarding_pending(&self) -> bool {
        self.snapshot
            .as_ref()
            .is_none_or(|snapshot| !snapshot.onboarding.complete)
    }

    pub fn select_previous_band(&mut self) {
        self.selected_band = self.selected_band.checked_sub(1).unwrap_or(2);
        self.return_to_adjusting();
    }

    pub fn select_next_band(&mut self) {
        self.selected_band = (self.selected_band + 1) % 3;
        self.return_to_adjusting();
    }

    pub fn rotate_selected(&mut self, delta: i8) {
        let position = i16::from(self.band_positions[self.selected_band]);
        self.band_positions[self.selected_band] =
            (position + i16::from(delta)).rem_euclid(i16::from(BAND_SLOTS)) as u8;
        self.return_to_adjusting();
    }

    pub fn aligned(&self) -> bool {
        self.band_positions[1..]
            .iter()
            .all(|position| *position == self.band_positions[0])
    }

    pub fn observe(&mut self) -> bool {
        if self.aligned() {
            self.onboarding_view = OnboardingView::Confirming;
            self.notice = None;
            true
        } else {
            self.notice = Some("The signals do not yet agree.".into());
            false
        }
    }

    pub fn begin_submission(&mut self) {
        self.onboarding_view = OnboardingView::Submitting;
        self.notice = None;
    }

    pub fn cancel_confirmation(&mut self) {
        self.return_to_adjusting();
    }

    pub fn onboarding_completed(&mut self, result: OnboardingCompletionResult) {
        if result.ok {
            self.notice = Some("ONBOARDING COMPLETE".into());
        } else {
            self.notice = Some(format!(
                "ONBOARDING NOT COMPLETED: {}",
                result.code.as_deref().unwrap_or("unknown_error")
            ));
        }
        self.onboarding_view = OnboardingView::Adjusting;
    }

    pub fn diagnostic_completed(&mut self, result: &DiagnosticPulseResult) {
        self.notice = Some(format!("DIAGNOSTIC PULSE {} COMPLETE", result.pulse_id));
    }

    pub fn diagnostic_failed(&mut self, error: impl Into<String>) {
        self.notice = Some(format!("DIAGNOSTIC FAILED: {}", error.into()));
    }

    pub fn scroll_up(&mut self, amount: usize) {
        self.follow_events = false;
        self.event_scroll = (self.event_scroll + amount).min(self.max_scroll());
    }

    pub fn scroll_down(&mut self, amount: usize) {
        self.event_scroll = self.event_scroll.saturating_sub(amount);
        if self.event_scroll == 0 {
            self.follow_events = true;
        }
    }

    pub fn scroll_home(&mut self) {
        self.follow_events = false;
        self.event_scroll = self.max_scroll();
    }

    pub fn scroll_end(&mut self) {
        self.event_scroll = 0;
        self.follow_events = true;
    }

    pub fn toggle_follow(&mut self) {
        self.follow_events = !self.follow_events;
        if self.follow_events {
            self.event_scroll = 0;
        }
    }

    fn return_to_adjusting(&mut self) {
        self.onboarding_view = OnboardingView::Adjusting;
        self.notice = None;
    }

    fn max_scroll(&self) -> usize {
        self.events.len().saturating_sub(1)
    }

    fn clamp_scroll(&mut self) {
        self.event_scroll = self.event_scroll.min(self.max_scroll());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_rotation_and_alignment_are_deterministic() {
        let mut app = App::default();
        assert_eq!(app.band_positions, [1, 5, 9]);
        app.select_previous_band();
        assert_eq!(app.selected_band, 2);
        for _ in 0..4 {
            app.rotate_selected(1);
        }
        assert_eq!(app.band_positions[2], 1);
        app.select_next_band();
        assert_eq!(app.selected_band, 0);
        app.select_next_band();
        for _ in 0..4 {
            app.rotate_selected(-1);
        }
        assert!(app.aligned());
        assert!(app.observe());
        assert_eq!(app.onboarding_view, OnboardingView::Confirming);
    }

    #[test]
    fn misaligned_observation_does_not_advance() {
        let mut app = App::default();
        assert!(!app.observe());
        assert_eq!(app.onboarding_view, OnboardingView::Adjusting);
        assert_eq!(app.notice.as_deref(), Some("The signals do not yet agree."));
    }
}
