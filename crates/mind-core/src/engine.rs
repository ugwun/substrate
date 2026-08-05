use crate::{
    dsl,
    game::{GameError, GameEvent, GameState},
    protocol::{
        CircuitGraphSnapshot, DiagnosticPulseResult, DslExecutionResult, EngineEvent,
        EngineEventKind, HealthState, StatusSnapshot,
    },
    PROTOCOL_VERSION,
};
use chrono::{DateTime, Utc};
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
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
    cpu_usage_bits: AtomicU32,
    clients: AtomicU32,
    shutting_down: AtomicBool,
    started_at: DateTime<Utc>,
    started_instant: Instant,
    history: Mutex<EngineHistory>,
    game: Mutex<GameState>,
    game_state_file: Option<PathBuf>,
    persistence_write: Mutex<()>,
    persistence_error: Mutex<Option<String>>,
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
        Self::build(tick_rate_hz, mcp_address, GameState::default(), None)
    }

    pub fn persistent(
        tick_rate_hz: u32,
        mcp_address: SocketAddr,
        game_state_file: PathBuf,
    ) -> Result<Arc<Self>, GameError> {
        let state = GameState::load(&game_state_file)?;
        Ok(Self::build(
            tick_rate_hz,
            mcp_address,
            state,
            Some(game_state_file),
        ))
    }

    fn build(
        tick_rate_hz: u32,
        mcp_address: SocketAddr,
        game: GameState,
        game_state_file: Option<PathBuf>,
    ) -> Arc<Self> {
        let engine = Arc::new(Self {
            tick_rate_hz,
            tick: AtomicU64::new(0),
            cpu_usage_bits: AtomicU32::new(f32::NAN.to_bits()),
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
            game: Mutex::new(game),
            game_state_file,
            persistence_write: Mutex::new(()),
            persistence_error: Mutex::new(None),
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
                    self.game().advance_tick(self.tick_rate_hz);
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
        let game = self.game().view();
        let history = self.history();
        StatusSnapshot {
            protocol_version: PROTOCOL_VERSION,
            daemon_version: env!("CARGO_PKG_VERSION").to_owned(),
            pid: std::process::id(),
            health: self.health(),
            started_at: self.started_at,
            uptime_ms: self.started_instant.elapsed().as_millis() as u64,
            tick: self.tick.load(Ordering::Relaxed),
            tick_rate_hz: self.tick_rate_hz,
            cpu_usage_percent: self.cpu_usage_percent(),
            resources: game.resources,
            progression: game.progression,
            circuit: game.circuit,
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

    pub fn set_cpu_usage_percent(&self, value: Option<f32>) {
        let value = value.filter(|value| value.is_finite() && *value >= 0.0);
        self.cpu_usage_bits
            .store(value.unwrap_or(f32::NAN).to_bits(), Ordering::Relaxed);
    }

    fn cpu_usage_percent(&self) -> Option<f32> {
        let value = f32::from_bits(self.cpu_usage_bits.load(Ordering::Relaxed));
        value.is_finite().then_some(value)
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
        let health = self.health();
        let circuit = self.game().view().circuit;
        let mut history = self.history();
        let result = DiagnosticPulseResult {
            pulse_id: history.next_pulse_id,
            emitted_at: Utc::now(),
            tick,
            health,
            components_checked: circuit.gate_count + 3,
            paths_traversed: circuit.wire_count,
            message: format!(
                "Circuit traversal complete; {} unresolved inputs; AWAKENING_BUS {}",
                circuit.unresolved_inputs,
                if circuit.awakening_bus { "HIGH" } else { "LOW" }
            ),
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

    pub fn execute_dsl(&self, command: &str) -> DslExecutionResult {
        let parsed = match dsl::parse(command) {
            Ok(parsed) => parsed,
            Err(diagnostic) => return self.dsl_failure(command, diagnostic),
        };
        let _persistence_write = self
            .game_state_file
            .as_ref()
            .map(|_| self.persistence_write());
        let mut state = self.game();
        let mut candidate = state.clone();
        let applied = match candidate.apply(parsed) {
            Ok(applied) => applied,
            Err(mut diagnostic) => {
                if diagnostic.start == diagnostic.end {
                    diagnostic.end = command.len();
                }
                return self.dsl_failure_with_state(command, diagnostic, &state);
            }
        };

        if applied.mutated {
            if let Some(path) = &self.game_state_file {
                if let Err(error) = candidate.save(path) {
                    drop(state);
                    self.set_persistence_error(Some(error.to_string()));
                    return self.dsl_failure(
                        command,
                        crate::protocol::DslDiagnostic {
                            code: "persistence_failed".into(),
                            message: format!("command was not committed: {error}"),
                            start: 0,
                            end: command.len(),
                        },
                    );
                }
                if let Some(generation) = candidate.dirty_generation() {
                    candidate.mark_saved(generation);
                }
            }
            *state = candidate;
        }
        let state_revision = state.state_revision();
        let circuit_revision = state.circuit_revision();
        drop(state);
        if applied.mutated {
            self.set_persistence_error(None);
        }
        if let Some(event) = applied.event {
            match event {
                GameEvent::LogicCoreActivated => {
                    self.record_event(EngineEventKind::LogicCoreActivated, "LOGIC_CORE activated")
                }
                GameEvent::CircuitChanged(message) => {
                    self.record_event(EngineEventKind::CircuitChanged, message)
                }
                GameEvent::MilestoneReached => self.record_event(
                    EngineEventKind::MilestoneReached,
                    "COGNITIVE_SPARK established; XOR fabrication unlocked",
                ),
            }
        }
        DslExecutionResult {
            ok: true,
            command: command.to_owned(),
            message: applied.message,
            diagnostic: None,
            state_revision,
            circuit_revision,
        }
    }

    pub fn circuit_graph(&self) -> CircuitGraphSnapshot {
        self.game().graph()
    }

    pub fn checkpoint_game_state(&self) -> Result<(), GameError> {
        let Some(path) = &self.game_state_file else {
            return Ok(());
        };
        let _persistence_write = self.persistence_write();
        let (snapshot, generation) = {
            let state = self.game();
            let Some(generation) = state.dirty_generation() else {
                self.set_persistence_error(None);
                return Ok(());
            };
            (state.clone(), generation)
        };
        if let Err(error) = snapshot.save(path) {
            self.set_persistence_error(Some(error.to_string()));
            return Err(error);
        }
        self.game().mark_saved(generation);
        self.set_persistence_error(None);
        Ok(())
    }

    pub fn game_state_file(&self) -> Option<&Path> {
        self.game_state_file.as_deref()
    }

    fn record_event(&self, kind: EngineEventKind, message: impl Into<String>) {
        let tick = self.tick.load(Ordering::Relaxed);
        let mut history = self.history();
        Self::push_event(&mut history, tick, kind, message.into());
    }

    fn health(&self) -> HealthState {
        if self.shutting_down.load(Ordering::Acquire) {
            HealthState::ShuttingDown
        } else if self.persistence_error().is_some() {
            HealthState::Degraded
        } else {
            HealthState::Ready
        }
    }

    fn set_persistence_error(&self, error: Option<String>) {
        let mut current = self.persistence_error();
        let changed = *current != error;
        let recovered = current.is_some() && error.is_none();
        *current = error.clone();
        drop(current);
        if changed {
            match error {
                Some(message) => self.record_event(
                    EngineEventKind::PersistenceDegraded,
                    format!("Game-state persistence degraded: {message}"),
                ),
                None if recovered => self.record_event(
                    EngineEventKind::PersistenceRecovered,
                    "Game-state persistence recovered",
                ),
                None => {}
            }
        }
    }

    fn dsl_failure(
        &self,
        command: &str,
        diagnostic: crate::protocol::DslDiagnostic,
    ) -> DslExecutionResult {
        let state = self.game();
        self.dsl_failure_with_state(command, diagnostic, &state)
    }

    fn dsl_failure_with_state(
        &self,
        command: &str,
        diagnostic: crate::protocol::DslDiagnostic,
        state: &GameState,
    ) -> DslExecutionResult {
        DslExecutionResult {
            ok: false,
            command: command.to_owned(),
            message: diagnostic.message.clone(),
            diagnostic: Some(diagnostic),
            state_revision: state.state_revision(),
            circuit_revision: state.circuit_revision(),
        }
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

    fn game(&self) -> MutexGuard<'_, GameState> {
        self.game.lock().unwrap_or_else(|error| error.into_inner())
    }

    fn persistence_error(&self) -> MutexGuard<'_, Option<String>> {
        self.persistence_error
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    fn persistence_write(&self) -> MutexGuard<'_, ()> {
        self.persistence_write
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

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

    #[test]
    fn cpu_telemetry_starts_unavailable_and_rejects_invalid_samples() {
        let engine = MindEngine::new(20);
        assert_eq!(engine.status().cpu_usage_percent, None);
        engine.set_cpu_usage_percent(Some(12.5));
        assert_eq!(engine.status().cpu_usage_percent, Some(12.5));
        engine.set_cpu_usage_percent(Some(f32::INFINITY));
        assert_eq!(engine.status().cpu_usage_percent, None);
    }

    #[test]
    fn persistence_failure_rolls_back_a_mutating_command() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("missing/game_state.json");
        let engine = MindEngine::persistent(
            20,
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7777),
            path,
        )
        .unwrap();
        for _ in 0..200 {
            engine.game().advance_tick(20);
        }
        let before = engine.status();
        let result = engine.execute_dsl("ACTIVATE LOGIC_CORE");
        let after = engine.status();

        assert!(!result.ok);
        assert_eq!(result.diagnostic.unwrap().code, "persistence_failed");
        assert!(!after.progression.logic_core_active);
        assert_eq!(
            after.resources.power_milliunits,
            before.resources.power_milliunits
        );
        assert_eq!(after.health, HealthState::Degraded);
    }
}
