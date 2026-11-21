use crate::{
    game::{GameAdvanceEvent, GameError, GameState, LoadDisposition},
    protocol::{
        DiagnosticPulseResult, EngineEvent, EngineEventKind, ExtractorLoadout, HealthState,
        NewPlaythroughResult, OnboardingCompletionResult, PlanetActionResult, PlanetSnapshot,
        StatusSnapshot,
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
        let loaded = GameState::load(&game_state_file)?;
        let disposition = loaded.disposition;
        let engine = Self::build(
            tick_rate_hz,
            mcp_address,
            loaded.state,
            Some(game_state_file),
        );
        if disposition == LoadDisposition::PreviousReset {
            engine.record_event(
                EngineEventKind::SaveReset,
                "Previous alpha save reset for the expanded planetary model",
            );
        } else if disposition == LoadDisposition::PreviousMigrated {
            engine.record_event(
                EngineEventKind::SaveMigrated,
                "Planetary save migrated to the current schema",
            );
        }
        Ok(engine)
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
                    let elapsed_ms = period.as_millis().max(1) as u64;
                    let events = self.game().advance(elapsed_ms);
                    for event in events {
                        let (kind, message) = match event {
                            GameAdvanceEvent::RegionDetected => (
                                EngineEventKind::RegionDetected,
                                "Scout sensors resolved regional terrain",
                            ),
                            GameAdvanceEvent::SurveyCompleted => (
                                EngineEventKind::SurveyCompleted,
                                "Regional survey completed",
                            ),
                            GameAdvanceEvent::ScoutReturned => (
                                EngineEventKind::ScoutReturned,
                                "Scout returned to the core",
                            ),
                            GameAdvanceEvent::ExtractionCompleted => (
                                EngineEventKind::ExtractionCompleted,
                                "Extractor returned material to the core",
                            ),
                            GameAdvanceEvent::ExtractorLost => (
                                EngineEventKind::ExtractorLost,
                                "Extractor and cargo lost; core rebuild initiated",
                            ),
                            GameAdvanceEvent::ExtractorRebuilt => (
                                EngineEventKind::ExtractorRebuilt,
                                "Core completed a replacement extractor",
                            ),
                            GameAdvanceEvent::RelayActivated => (
                                EngineEventKind::RelayActivated,
                                "Regional relay activated; operational reach expanded",
                            ),
                            GameAdvanceEvent::AlphaMilestoneCompleted => (
                                EngineEventKind::AlphaMilestoneCompleted,
                                "Extended reach proven by a completed survey",
                            ),
                            GameAdvanceEvent::FoundationCompleted => (
                                EngineEventKind::FoundationCompleted,
                                "Routine automation proven; planetary network planning available",
                            ),
                            GameAdvanceEvent::SubregionSurveyed => (
                                EngineEventKind::SubregionSurveyed,
                                "Focused local survey completed",
                            ),
                            GameAdvanceEvent::BaseEstablished => (
                                EngineEventKind::BaseEstablished,
                                "Planetary foothold established",
                            ),
                            GameAdvanceEvent::BaseModuleCompleted => (
                                EngineEventKind::BaseModuleCompleted,
                                "Base module construction completed",
                            ),
                            GameAdvanceEvent::BaseMaintained => (
                                EngineEventKind::BaseMaintained,
                                "Base maintenance cycle completed",
                            ),
                            GameAdvanceEvent::BaseStrained => (
                                EngineEventKind::BaseStrained,
                                "A base reached an environmental strain threshold",
                            ),
                            GameAdvanceEvent::SolarHorizonReached => (
                                EngineEventKind::SolarHorizonReached,
                                "Solar horizon reached; system map is available",
                            ),
                            GameAdvanceEvent::ArchiveExperimentCompleted => (
                                EngineEventKind::ArchiveExperimentCompleted,
                                "Archive comparison experiment completed",
                            ),
                            GameAdvanceEvent::ArchiveObservationRecorded => (
                                EngineEventKind::ArchiveObservationRecorded,
                                "Permanent archive observation recorded",
                            ),
                            GameAdvanceEvent::ResearchCompleted => (
                                EngineEventKind::ResearchCompleted,
                                "Research completed; a new operational profile is available",
                            ),
                            GameAdvanceEvent::AutomationPaused => (
                                EngineEventKind::AutomationPaused,
                                "An automated operation paused and requires review",
                            ),
                        };
                        self.record_event(kind, message);
                    }
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
        let game = self.game();
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
            onboarding: game.onboarding(),
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

    pub fn set_cpu_usage_percent(&self, value: Option<f32>) {
        let value = value.filter(|value| value.is_finite() && *value >= 0.0);
        self.cpu_usage_bits
            .store(value.unwrap_or(f32::NAN).to_bits(), Ordering::Relaxed);
    }

    fn cpu_usage_percent(&self) -> Option<f32> {
        let value = f32::from_bits(self.cpu_usage_bits.load(Ordering::Relaxed));
        value.is_finite().then_some(value)
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
        let mut history = self.history();
        let result = DiagnosticPulseResult {
            pulse_id: history.next_pulse_id,
            emitted_at: Utc::now(),
            tick,
            health,
            message: "Daemon responsive; persistence and simulation checks complete".into(),
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

    pub fn complete_onboarding(&self, band_positions: [u8; 3]) -> OnboardingCompletionResult {
        let _persistence_write = self
            .game_state_file
            .as_ref()
            .map(|_| self.persistence_write());
        let mut state = self.game();
        let mut candidate = state.clone();
        let mut result = candidate.complete_onboarding(band_positions);
        if !result.ok || candidate.state_revision() == state.state_revision() {
            return result;
        }
        if let Some(path) = &self.game_state_file {
            if let Err(error) = candidate.save(path) {
                drop(state);
                self.set_persistence_error(Some(error.to_string()));
                result.ok = false;
                result.code = Some("persistence_failed".into());
                result.completed = false;
                return result;
            }
            if let Some(generation) = candidate.dirty_generation() {
                candidate.mark_saved(generation);
            }
        }
        *state = candidate;
        drop(state);
        self.set_persistence_error(None);
        self.record_event(
            EngineEventKind::OnboardingCompleted,
            "Graphical onboarding completed",
        );
        result
    }

    pub fn start_new_playthrough(&self) -> NewPlaythroughResult {
        let _persistence_write = self
            .game_state_file
            .as_ref()
            .map(|_| self.persistence_write());
        let mut state = self.game();
        let mut candidate = GameState::new_playthrough_after(state.state_revision());
        if let Some(path) = &self.game_state_file {
            if let Err(error) = candidate.save(path) {
                drop(state);
                self.set_persistence_error(Some(error.to_string()));
                return NewPlaythroughResult {
                    ok: false,
                    code: Some("persistence_failed".into()),
                };
            }
            if let Some(generation) = candidate.dirty_generation() {
                candidate.mark_saved(generation);
            }
        }
        *state = candidate;
        drop(state);
        self.set_persistence_error(None);

        {
            let mut history = self.history();
            history.next_event_id = 1;
            history.next_pulse_id = 1;
            history.events.clear();
            history.last_diagnostic = None;
        }
        self.record_event(
            EngineEventKind::SaveReset,
            "New playthrough created; onboarding is ready",
        );
        NewPlaythroughResult {
            ok: true,
            code: None,
        }
    }

    pub fn planet(&self) -> Option<PlanetSnapshot> {
        self.game().planet_snapshot()
    }

    pub fn deploy_scout(&self, region_id: &str) -> PlanetActionResult {
        let _persistence_write = self
            .game_state_file
            .as_ref()
            .map(|_| self.persistence_write());
        let mut state = self.game();
        let mut candidate = state.clone();
        let mut result = candidate.deploy_scout(region_id);
        if !result.ok {
            return result;
        }
        if let Some(path) = &self.game_state_file {
            if let Err(error) = candidate.save(path) {
                drop(state);
                self.set_persistence_error(Some(error.to_string()));
                result.ok = false;
                result.code = Some("persistence_failed".into());
                return result;
            }
            if let Some(generation) = candidate.dirty_generation() {
                candidate.mark_saved(generation);
            }
        }
        *state = candidate;
        drop(state);
        self.set_persistence_error(None);
        self.record_event(EngineEventKind::ScoutDeployed, "Scout deployment launched");
        result
    }

    pub fn recall_scout(&self) -> PlanetActionResult {
        let _persistence_write = self
            .game_state_file
            .as_ref()
            .map(|_| self.persistence_write());
        let mut state = self.game();
        let mut candidate = state.clone();
        let mut result = candidate.recall_scout();
        if !result.ok {
            return result;
        }
        if let Some(path) = &self.game_state_file {
            if let Err(error) = candidate.save(path) {
                drop(state);
                self.set_persistence_error(Some(error.to_string()));
                result.ok = false;
                result.code = Some("persistence_failed".into());
                return result;
            }
            if let Some(generation) = candidate.dirty_generation() {
                candidate.mark_saved(generation);
            }
        }
        *state = candidate;
        drop(state);
        self.set_persistence_error(None);
        self.record_event(
            EngineEventKind::ScoutRecallRequested,
            "Scout recall requested",
        );
        result
    }

    pub fn deploy_extractor(&self, region_id: &str) -> PlanetActionResult {
        self.deploy_extractor_at(region_id, None)
    }

    pub fn configure_extractor(&self, loadout: ExtractorLoadout) -> PlanetActionResult {
        self.commit_planet_action(
            |state| state.configure_extractor(loadout),
            EngineEventKind::ExtractorConfigured,
            "Extraction rig reconfigured",
        )
    }

    pub fn deploy_extractor_at(
        &self,
        region_id: &str,
        subregion_id: Option<&str>,
    ) -> PlanetActionResult {
        let _persistence_write = self
            .game_state_file
            .as_ref()
            .map(|_| self.persistence_write());
        let mut state = self.game();
        let mut candidate = state.clone();
        let mut result = candidate.deploy_extractor_at(region_id, subregion_id);
        if !result.ok {
            return result;
        }
        if let Some(path) = &self.game_state_file {
            if let Err(error) = candidate.save(path) {
                drop(state);
                self.set_persistence_error(Some(error.to_string()));
                result.ok = false;
                result.code = Some("persistence_failed".into());
                return result;
            }
            if let Some(generation) = candidate.dirty_generation() {
                candidate.mark_saved(generation);
            }
        }
        *state = candidate;
        drop(state);
        self.set_persistence_error(None);
        self.record_event(
            EngineEventKind::ExtractorDeployed,
            "Extraction deployment launched",
        );
        result
    }

    pub fn survey_subregion(&self, subregion_id: &str) -> PlanetActionResult {
        self.commit_planet_action(
            |state| state.survey_subregion(subregion_id),
            EngineEventKind::SubregionSurveyed,
            "Focused local survey started",
        )
    }

    pub fn establish_base(&self, subregion_id: &str) -> PlanetActionResult {
        self.commit_planet_action(
            |state| state.establish_base(subregion_id),
            EngineEventKind::BaseProjectStarted,
            "Planetary foothold project started",
        )
    }

    pub fn construct_base_module(
        &self,
        subregion_id: &str,
        module: crate::protocol::BaseModuleKind,
    ) -> PlanetActionResult {
        self.commit_planet_action(
            |state| state.construct_base_module(subregion_id, module),
            EngineEventKind::BaseProjectStarted,
            "Base module project started",
        )
    }

    pub fn set_base_module_enabled(
        &self,
        subregion_id: &str,
        module: crate::protocol::BaseModuleKind,
        enabled: bool,
    ) -> PlanetActionResult {
        self.commit_planet_action(
            |state| state.set_base_module_enabled(subregion_id, module, enabled),
            EngineEventKind::BaseModuleModeChanged,
            if enabled {
                "Base module restored"
            } else {
                "Base module suspended"
            },
        )
    }

    pub fn maintain_base(&self, subregion_id: &str) -> PlanetActionResult {
        self.commit_planet_action(
            |state| state.maintain_base(subregion_id),
            EngineEventKind::BaseProjectStarted,
            "Base maintenance project started",
        )
    }

    pub fn construct_relay(&self, region_id: &str) -> PlanetActionResult {
        let _persistence_write = self
            .game_state_file
            .as_ref()
            .map(|_| self.persistence_write());
        let mut state = self.game();
        let mut candidate = state.clone();
        let mut result = candidate.construct_relay(region_id);
        if !result.ok {
            return result;
        }
        if let Some(path) = &self.game_state_file {
            if let Err(error) = candidate.save(path) {
                drop(state);
                self.set_persistence_error(Some(error.to_string()));
                result.ok = false;
                result.code = Some("persistence_failed".into());
                return result;
            }
            if let Some(generation) = candidate.dirty_generation() {
                candidate.mark_saved(generation);
            }
        }
        *state = candidate;
        drop(state);
        self.set_persistence_error(None);
        self.record_event(
            EngineEventKind::RelayConstructionStarted,
            "Regional relay construction started",
        );
        result
    }

    pub fn start_research(&self, project: crate::protocol::ResearchProject) -> PlanetActionResult {
        self.commit_planet_action(
            |state| state.start_research(project),
            EngineEventKind::ResearchStarted,
            "Research direction committed",
        )
    }

    pub fn start_archive_experiment(&self) -> PlanetActionResult {
        self.commit_planet_action(
            |state| state.start_archive_experiment(),
            EngineEventKind::ArchiveExperimentStarted,
            "Archive comparison experiment started",
        )
    }

    pub fn activate_profile(
        &self,
        profile: crate::protocol::OperationalProfile,
    ) -> PlanetActionResult {
        self.commit_planet_action(
            |state| state.activate_profile(profile),
            EngineEventKind::ProfileActivated,
            "Operational profile activated",
        )
    }

    pub fn queue_survey(&self, region_id: &str) -> PlanetActionResult {
        self.commit_planet_action(
            |state| state.queue_survey(region_id),
            EngineEventKind::SurveyQueued,
            "Automated survey queued",
        )
    }

    pub fn remove_queued_survey(&self, region_id: &str) -> PlanetActionResult {
        self.commit_planet_action(
            |state| state.remove_queued_survey(region_id),
            EngineEventKind::SurveyQueued,
            "Queued survey removed",
        )
    }

    pub fn set_extraction_route(&self, region_id: &str) -> PlanetActionResult {
        self.commit_planet_action(
            |state| state.set_extraction_route(region_id),
            EngineEventKind::ExtractionRouteSet,
            "Proven extraction route activated",
        )
    }

    pub fn clear_extraction_route(&self) -> PlanetActionResult {
        self.commit_planet_action(
            GameState::clear_extraction_route,
            EngineEventKind::ExtractionRouteSet,
            "Automated extraction route cleared",
        )
    }

    pub fn acknowledge_activity(&self, through_activity_id: u64) -> PlanetActionResult {
        self.commit_planet_action(
            |state| state.acknowledge_activity(through_activity_id),
            EngineEventKind::ActivityReviewed,
            "Return activity reviewed",
        )
    }

    fn commit_planet_action(
        &self,
        operation: impl FnOnce(&mut GameState) -> PlanetActionResult,
        event: EngineEventKind,
        message: &'static str,
    ) -> PlanetActionResult {
        let _persistence_write = self
            .game_state_file
            .as_ref()
            .map(|_| self.persistence_write());
        let mut state = self.game();
        let mut candidate = state.clone();
        let mut result = operation(&mut candidate);
        if !result.ok {
            return result;
        }
        if candidate.state_revision() == state.state_revision() {
            return result;
        }
        if let Some(path) = &self.game_state_file {
            if let Err(error) = candidate.save(path) {
                drop(state);
                self.set_persistence_error(Some(error.to_string()));
                result.ok = false;
                result.code = Some("persistence_failed".into());
                return result;
            }
            if let Some(generation) = candidate.dirty_generation() {
                candidate.mark_saved(generation);
            }
        }
        *state = candidate;
        drop(state);
        self.set_persistence_error(None);
        self.record_event(event, message);
        result
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
    fn completion_is_validated_and_idempotent() {
        let engine = MindEngine::new(20);
        assert_eq!(
            engine.complete_onboarding([1, 2, 1]).code.as_deref(),
            Some("not_coherent")
        );
        let completed = engine.complete_onboarding([4, 4, 4]);
        assert!(completed.ok);
        assert!(engine.status().onboarding.complete);
        assert_eq!(engine.complete_onboarding([0, 0, 0]).state_revision, 2);
        let events = engine
            .status()
            .recent_events
            .into_iter()
            .filter(|event| event.kind == EngineEventKind::OnboardingCompleted)
            .count();
        assert_eq!(events, 1);
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
    fn new_playthrough_replaces_and_persists_progress_atomically() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("data/game_state.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let engine = MindEngine::persistent(
            20,
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7777),
            path.clone(),
        )
        .unwrap();
        let completed = engine.complete_onboarding([3, 3, 3]);
        engine.trigger_diagnostic_pulse();

        let result = engine.start_new_playthrough();

        assert!(result.ok);
        assert!(!engine.status().onboarding.complete);
        assert!(engine.planet().is_none());
        assert!(engine.status().last_diagnostic.is_none());
        assert_eq!(engine.status().recent_events.len(), 1);
        assert_eq!(
            engine.status().recent_events[0].kind,
            EngineEventKind::SaveReset
        );
        let loaded = GameState::load(&path).unwrap();
        assert!(!loaded.state.onboarding().complete);
        assert!(
            loaded.state.state_revision() > completed.state_revision,
            "the reset revision must remain monotonic for attached clients"
        );
    }

    #[test]
    fn new_playthrough_rolls_back_when_persistence_fails() {
        let temp = TempDir::new().unwrap();
        let data = temp.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        let path = data.join("game_state.json");
        let engine = MindEngine::persistent(
            20,
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7777),
            path.clone(),
        )
        .unwrap();
        assert!(engine.complete_onboarding([3, 3, 3]).ok);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(data).unwrap();

        let result = engine.start_new_playthrough();

        assert!(!result.ok);
        assert_eq!(result.code.as_deref(), Some("persistence_failed"));
        assert!(engine.status().onboarding.complete);
        assert!(engine.planet().is_some());
        assert_eq!(engine.status().health, HealthState::Degraded);
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
    fn persistence_failure_rolls_back_completion() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("missing/game_state.json");
        let engine = MindEngine::persistent(
            20,
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7777),
            path,
        )
        .unwrap();
        let result = engine.complete_onboarding([2, 2, 2]);
        assert!(!result.ok);
        assert_eq!(result.code.as_deref(), Some("persistence_failed"));
        assert!(!engine.status().onboarding.complete);
        assert_eq!(engine.status().health, HealthState::Degraded);
    }
}
