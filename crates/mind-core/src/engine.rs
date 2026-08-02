use crate::{
    protocol::{DiagnosticPulseResult, EngineEvent, EngineEventKind, HealthState, StatusSnapshot},
    PROTOCOL_VERSION,
};
use chrono::{DateTime, Utc};
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        Arc, Mutex, MutexGuard,
    },
    time::{Duration, Instant},
};
use tokio::{sync::watch, time::MissedTickBehavior};

pub struct MindEngine {
    tick_rate_hz: u32,
    tick: AtomicU64,
    clients: AtomicU32,
    shutting_down: AtomicBool,
    started_at: DateTime<Utc>,
    started_instant: Instant,
    history: Mutex<EngineHistory>,
    mcp_address: SocketAddr,
}

const EVENT_CAPACITY: usize = 64;
const STATUS_EVENT_COUNT: usize = 20;

struct EngineHistory {
    next_event_id: u64,
    next_pulse_id: u64,
    events: std::collections::VecDeque<EngineEvent>,
    last_diagnostic: Option<DiagnosticPulseResult>,
}

impl MindEngine {
    pub fn new(tick_rate_hz: u32) -> Arc<Self> {
        Self::with_mcp_address(
            tick_rate_hz,
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7777),
        )
    }

    pub fn with_mcp_address(tick_rate_hz: u32, mcp_address: SocketAddr) -> Arc<Self> {
        let engine = Arc::new(Self {
            tick_rate_hz,
            tick: AtomicU64::new(0),
            clients: AtomicU32::new(0),
            shutting_down: AtomicBool::new(false),
            started_at: Utc::now(),
            started_instant: Instant::now(),
            history: Mutex::new(EngineHistory {
                next_event_id: 1,
                next_pulse_id: 1,
                events: std::collections::VecDeque::with_capacity(EVENT_CAPACITY),
                last_diagnostic: None,
            }),
            mcp_address,
        });
        engine.record_event(EngineEventKind::DaemonStarted, "Mind daemon started");
        engine
    }

    pub async fn run(self: Arc<Self>, mut shutdown: watch::Receiver<bool>) {
        let period = Duration::from_secs_f64(1.0 / f64::from(self.tick_rate_hz));
        let mut interval = tokio::time::interval(period);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        interval.tick().await;
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    self.tick.fetch_add(1, Ordering::Relaxed);
                }
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        self.shutting_down.store(true, Ordering::Release);
                        break;
                    }
                }
            }
        }
    }

    pub fn status(&self) -> StatusSnapshot {
        let history = self.history();
        StatusSnapshot {
            protocol_version: PROTOCOL_VERSION,
            daemon_version: env!("CARGO_PKG_VERSION").to_owned(),
            pid: std::process::id(),
            health: if self.shutting_down.load(Ordering::Acquire) {
                HealthState::ShuttingDown
            } else {
                HealthState::Ready
            },
            started_at: self.started_at,
            uptime_ms: self.started_instant.elapsed().as_millis() as u64,
            tick: self.tick.load(Ordering::Relaxed),
            tick_rate_hz: self.tick_rate_hz,
            connected_clients: self.clients.load(Ordering::Relaxed),
            mcp_url: format!("http://{}/mcp", self.mcp_address),
            last_diagnostic: history.last_diagnostic.clone(),
            recent_events: history
                .events
                .iter()
                .rev()
                .take(STATUS_EVENT_COUNT)
                .cloned()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect(),
        }
    }

    pub fn client_connected(&self) {
        self.clients.fetch_add(1, Ordering::Relaxed);
    }

    pub fn client_disconnected(&self) {
        self.clients.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn begin_shutdown(&self) {
        if self
            .shutting_down
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            self.record_event(
                EngineEventKind::ShutdownRequested,
                "Graceful shutdown requested",
            );
        }
    }

    pub fn trigger_diagnostic_pulse(&self) -> DiagnosticPulseResult {
        let tick = self.tick.load(Ordering::Relaxed);
        let health = if self.shutting_down.load(Ordering::Acquire) {
            HealthState::ShuttingDown
        } else {
            HealthState::Ready
        };
        let mut history = self.history();
        let result = DiagnosticPulseResult {
            pulse_id: history.next_pulse_id,
            emitted_at: Utc::now(),
            tick,
            health,
            components_checked: 0,
            paths_traversed: 0,
            message: "Core systems responsive; circuit traversal is unavailable until Phase 1.4"
                .to_owned(),
        };
        history.next_pulse_id += 1;
        history.last_diagnostic = Some(result.clone());
        Self::push_event(
            &mut history,
            tick,
            EngineEventKind::DiagnosticPulse,
            format!("Diagnostic pulse {} completed", result.pulse_id),
        );
        result
    }

    fn record_event(&self, kind: EngineEventKind, message: impl Into<String>) {
        let tick = self.tick.load(Ordering::Relaxed);
        let mut history = self.history();
        Self::push_event(&mut history, tick, kind, message.into());
    }

    fn push_event(history: &mut EngineHistory, tick: u64, kind: EngineEventKind, message: String) {
        let event = EngineEvent {
            event_id: history.next_event_id,
            occurred_at: Utc::now(),
            tick,
            kind,
            message,
        };
        history.next_event_id += 1;
        if history.events.len() == EVENT_CAPACITY {
            history.events.pop_front();
        }
        history.events.push_back(event);
    }

    fn history(&self) -> MutexGuard<'_, EngineHistory> {
        self.history
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn advances_ticks_and_stops() {
        let engine = MindEngine::new(20);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let task = tokio::spawn(engine.clone().run(shutdown_rx));
        tokio::task::yield_now().await;
        for _ in 0..3 {
            tokio::time::advance(Duration::from_millis(50)).await;
            tokio::task::yield_now().await;
        }
        assert!(engine.status().tick >= 2);
        shutdown_tx.send(true).unwrap();
        task.await.unwrap();
        assert_eq!(engine.status().health, HealthState::ShuttingDown);
    }

    #[test]
    fn diagnostic_pulses_are_recorded_and_events_are_bounded() {
        let engine = MindEngine::new(20);
        for expected in 1..=70 {
            assert_eq!(engine.trigger_diagnostic_pulse().pulse_id, expected);
        }
        let status = engine.status();
        assert_eq!(status.last_diagnostic.unwrap().pulse_id, 70);
        assert_eq!(status.recent_events.len(), STATUS_EVENT_COUNT);
        assert_eq!(status.recent_events.last().unwrap().event_id, 71);
    }

    #[test]
    fn shutdown_event_is_idempotent() {
        let engine = MindEngine::new(20);
        engine.begin_shutdown();
        engine.begin_shutdown();
        let shutdown_events = engine
            .status()
            .recent_events
            .into_iter()
            .filter(|event| event.kind == EngineEventKind::ShutdownRequested)
            .count();
        assert_eq!(shutdown_events, 1);
    }
}
