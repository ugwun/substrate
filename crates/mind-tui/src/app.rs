use mind_core::protocol::{DiagnosticPulseResult, EngineEvent, StatusSnapshot};
use std::{collections::VecDeque, time::Instant};

const EVENT_CACHE_CAPACITY: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionState {
    Connecting,
    Connected,
    Disconnected(String),
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

    pub fn diagnostic_completed(&mut self, result: &DiagnosticPulseResult) {
        self.notice = Some(format!("DIAGNOSTIC PULSE {} COMPLETE", result.pulse_id));
    }

    pub fn diagnostic_failed(&mut self, error: impl Into<String>) {
        self.notice = Some(format!("DIAGNOSTIC FAILED: {}", error.into()));
    }

    pub fn scroll_up(&mut self, amount: usize) {
        self.follow_events = false;
        self.event_scroll = self.event_scroll.saturating_add(amount);
        self.clamp_scroll();
    }

    pub fn scroll_down(&mut self, amount: usize) {
        self.event_scroll = self.event_scroll.saturating_sub(amount);
        if self.event_scroll == 0 {
            self.follow_events = true;
        }
    }

    pub fn scroll_home(&mut self) {
        self.follow_events = false;
        self.event_scroll = self.events.len().saturating_sub(1);
    }

    pub fn scroll_end(&mut self) {
        self.follow_events = true;
        self.event_scroll = 0;
    }

    pub fn toggle_follow(&mut self) {
        self.follow_events = !self.follow_events;
        if self.follow_events {
            self.event_scroll = 0;
        }
    }

    fn clamp_scroll(&mut self) {
        self.event_scroll = self.event_scroll.min(self.events.len().saturating_sub(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use mind_core::protocol::{EngineEventKind, HealthState};
    use std::time::Duration;

    fn snapshot(tick: u64, events: Vec<EngineEvent>) -> StatusSnapshot {
        StatusSnapshot {
            protocol_version: 1,
            daemon_version: "0.1.0".into(),
            pid: 42,
            health: HealthState::Ready,
            started_at: Utc::now(),
            uptime_ms: tick * 50,
            tick,
            tick_rate_hz: 20,
            cpu_usage_percent: Some(2.5),
            connected_clients: 1,
            mcp_url: "http://127.0.0.1:7777/mcp".into(),
            last_diagnostic: None,
            recent_events: events,
        }
    }

    fn event(id: u64) -> EngineEvent {
        EngineEvent {
            event_id: id,
            occurred_at: Utc::now(),
            tick: id,
            kind: EngineEventKind::DiagnosticPulse,
            message: format!("event {id}"),
        }
    }

    #[test]
    fn derives_observed_rate_and_deduplicates_events() {
        let mut app = App::default();
        let start = Instant::now();
        app.apply_snapshot(snapshot(10, vec![event(1)]), start);
        app.apply_snapshot(
            snapshot(30, vec![event(1), event(2)]),
            start + Duration::from_secs(1),
        );
        assert_eq!(app.observed_tick_rate, Some(20.0));
        assert_eq!(app.events.len(), 2);
    }

    #[test]
    fn preserves_scrolled_content_when_new_events_arrive() {
        let mut app = App::default();
        let start = Instant::now();
        app.apply_snapshot(snapshot(1, vec![event(1), event(2)]), start);
        app.scroll_up(1);
        app.apply_snapshot(
            snapshot(2, vec![event(2), event(3)]),
            start + Duration::from_secs(1),
        );
        assert!(!app.follow_events);
        assert_eq!(app.event_scroll, 2);
        app.scroll_end();
        assert!(app.follow_events);
        assert_eq!(app.event_scroll, 0);
    }
}
