use crate::connection::PlanetActionKind;
use mind_core::protocol::{
    DiagnosticPulseResult, EngineEvent, OnboardingCompletionResult, PlanetActionResult,
    PlanetSnapshot, RegionSnapshot, ScoutState, StatusSnapshot,
};
use std::{collections::VecDeque, time::Duration, time::Instant};

const EVENT_CACHE_CAPACITY: usize = 256;
const IDLE_DELAY: Duration = Duration::from_secs(30);
const IDLE_ROTATION_SPEED: f64 = 0.085;
const CAMERA_RESPONSE: f64 = 7.5;
const MAX_FRAME_DELTA: Duration = Duration::from_millis(250);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Workspace {
    Planet,
    Overview,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextAction {
    Deploy(String),
    Recall,
    Extract(String),
    ConstructRelay(String),
}

#[derive(Debug)]
pub struct App {
    pub connection: ConnectionState,
    pub snapshot: Option<StatusSnapshot>,
    pub planet: Option<PlanetSnapshot>,
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
    pub workspace: Workspace,
    pub selected_region_id: Option<String>,
    pub region_panel_scroll: u16,
    pub overview_selection: usize,
    pub pending_action: Option<ContextAction>,
    pub globe_yaw: f64,
    pub globe_pitch: f64,
    pub globe_phase: f64,
    previous_tick_sample: Option<(u64, Instant)>,
    last_input: Instant,
    last_animation_frame: Instant,
    globe_target_yaw: f64,
    globe_target_pitch: f64,
}

impl Default for App {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            connection: ConnectionState::Connecting,
            snapshot: None,
            planet: None,
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
            workspace: Workspace::Planet,
            selected_region_id: None,
            region_panel_scroll: 0,
            overview_selection: 0,
            pending_action: None,
            globe_yaw: 0.0,
            globe_pitch: 0.0,
            globe_phase: 0.0,
            previous_tick_sample: None,
            last_input: now,
            last_animation_frame: now,
            globe_target_yaw: 0.0,
            globe_target_pitch: 0.0,
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

    pub fn apply_planet(&mut self, planet: PlanetSnapshot, received_at: Instant) {
        let selected_is_valid = self
            .selected_region_id
            .as_ref()
            .is_some_and(|selected| planet.regions.iter().any(|region| &region.id == selected));
        let selection_changed = !selected_is_valid;
        if !selected_is_valid {
            self.selected_region_id = Some(planet.core_region_id.clone());
        }
        self.planet = Some(planet);
        self.clamp_overview_selection();
        if selection_changed {
            self.region_panel_scroll = 0;
            self.focus_selected_region(true);
        }
        self.last_update = Some(received_at);
    }

    pub fn onboarding_pending(&self) -> bool {
        self.snapshot
            .as_ref()
            .is_none_or(|snapshot| !snapshot.onboarding.complete)
    }

    pub fn record_input(&mut self, now: Instant) {
        self.last_input = now;
    }

    pub fn advance_globe_animation(&mut self, now: Instant) {
        let elapsed = now
            .saturating_duration_since(self.last_animation_frame)
            .min(MAX_FRAME_DELTA)
            .as_secs_f64();
        self.last_animation_frame = now;
        if self.workspace != Workspace::Planet
            || self.show_help
            || self.pending_action.is_some()
            || !matches!(self.connection, ConnectionState::Connected)
        {
            return;
        }
        self.globe_phase = (self.globe_phase + elapsed * 0.42).rem_euclid(1.0);
        if now.saturating_duration_since(self.last_input) >= IDLE_DELAY {
            self.globe_yaw = normalize_angle(self.globe_yaw + IDLE_ROTATION_SPEED * elapsed);
            self.globe_pitch = ease(self.globe_pitch, self.globe_target_pitch * 0.45, elapsed);
        } else {
            self.globe_yaw = ease_angle(self.globe_yaw, self.globe_target_yaw, elapsed);
            self.globe_pitch = ease(self.globe_pitch, self.globe_target_pitch, elapsed);
        }
    }

    pub fn cycle_workspace(&mut self, reverse: bool) {
        self.workspace = match (self.workspace, reverse) {
            (Workspace::Planet, false) => Workspace::Overview,
            (Workspace::Overview, false) => Workspace::System,
            (Workspace::System, false) => Workspace::Planet,
            (Workspace::Planet, true) => Workspace::System,
            (Workspace::Overview, true) => Workspace::Planet,
            (Workspace::System, true) => Workspace::Overview,
        };
        self.pending_action = None;
        self.notice = None;
    }

    pub fn move_selection(&mut self, latitude_delta: i8, longitude_delta: i8) {
        let Some(current) = self.selected_region().cloned() else {
            return;
        };
        let latitude = (i16::from(current.position.latitude_band) + i16::from(latitude_delta))
            .clamp(0, 3) as u8;
        let longitude = (i16::from(current.position.longitude_sector) + i16::from(longitude_delta))
            .rem_euclid(6) as u8;
        if let Some(region) = self.planet.as_ref().and_then(|planet| {
            planet.regions.iter().find(|region| {
                region.position.latitude_band == latitude
                    && region.position.longitude_sector == longitude
            })
        }) {
            self.selected_region_id = Some(region.id.clone());
            self.focus_selected_region(false);
        }
        self.region_panel_scroll = 0;
        self.notice = None;
    }

    pub fn selected_region(&self) -> Option<&RegionSnapshot> {
        let selected = self.selected_region_id.as_ref()?;
        self.planet
            .as_ref()?
            .regions
            .iter()
            .find(|region| &region.id == selected)
    }

    pub fn primary_region_action(&self) -> Option<ContextAction> {
        let planet = self.planet.as_ref()?;
        if self.workspace != Workspace::Planet {
            return None;
        }
        let region = self.selected_region()?;
        if self.scout_recall_action().is_some() {
            Some(ContextAction::Recall)
        } else if region
            .deposit
            .as_ref()
            .is_some_and(|deposit| deposit.remaining > 0)
            && planet.extractor.state == mind_core::protocol::ExtractorState::Ready
            && region.within_scout_range
        {
            Some(ContextAction::Extract(region.id.clone()))
        } else if planet.scout.state == ScoutState::Ready
            && region.within_scout_range
            && !region.core
        {
            Some(ContextAction::Deploy(region.id.clone()))
        } else {
            self.construction_action()
        }
    }

    pub fn scout_action(&self) -> Option<ContextAction> {
        let planet = self.planet.as_ref()?;
        let region = self.selected_region()?;
        (self.workspace == Workspace::Planet
            && planet.scout.state == ScoutState::Ready
            && region.within_scout_range
            && !region.core)
            .then(|| ContextAction::Deploy(region.id.clone()))
    }

    pub fn extraction_action(&self) -> Option<ContextAction> {
        let planet = self.planet.as_ref()?;
        let region = self.selected_region()?;
        (self.workspace == Workspace::Planet
            && planet.extractor.state == mind_core::protocol::ExtractorState::Ready
            && region.within_scout_range
            && region
                .deposit
                .as_ref()
                .is_some_and(|deposit| deposit.remaining > 0))
        .then(|| ContextAction::Extract(region.id.clone()))
    }

    pub fn construction_action(&self) -> Option<ContextAction> {
        let planet = self.planet.as_ref()?;
        let region = self.selected_region()?;
        let stock = planet.refined_inventory;
        (self.workspace == Workspace::Planet
            && !region.core
            && region.knowledge >= mind_core::protocol::RegionKnowledge::Surveyed
            && region.within_scout_range
            && planet.relay_region_id.is_none()
            && planet.active_construction.is_none()
            && stock.structural >= 8
            && stock.conductive >= 6
            && stock.silicate >= 6)
            .then(|| ContextAction::ConstructRelay(region.id.clone()))
    }

    pub fn scout_recall_action(&self) -> Option<ContextAction> {
        let planet = self.planet.as_ref()?;
        let region = self.selected_region()?;
        let mission = planet.active_mission.as_ref()?;
        (self.workspace == Workspace::Planet
            && mission.target_region_id == region.id
            && matches!(
                mission.phase,
                ScoutState::Outbound | ScoutState::Surveying | ScoutState::Holding
            ))
        .then_some(ContextAction::Recall)
    }

    pub fn planet_action_completed(&mut self, kind: PlanetActionKind, result: PlanetActionResult) {
        self.pending_action = None;
        self.notice = Some(if result.ok {
            match kind {
                PlanetActionKind::Deploy => "SCOUT DEPLOYED".into(),
                PlanetActionKind::Recall => "SCOUT RECALL INITIATED".into(),
                PlanetActionKind::Extract => "EXTRACTOR DEPLOYED".into(),
                PlanetActionKind::Construct => "RELAY CONSTRUCTION STARTED".into(),
            }
        } else {
            format!(
                "ACTION REJECTED: {}",
                result.code.as_deref().unwrap_or("unknown_error")
            )
        });
    }

    pub fn request_confirmation(&mut self, action: ContextAction) -> bool {
        if self.pending_action.as_ref() == Some(&action) {
            true
        } else {
            self.pending_action = Some(action);
            false
        }
    }

    pub fn cancel_confirmation_action(&mut self) {
        self.pending_action = None;
    }

    pub fn scroll_region_panel(&mut self, amount: i16) {
        self.region_panel_scroll = if amount.is_negative() {
            self.region_panel_scroll
                .saturating_sub(amount.unsigned_abs())
        } else {
            self.region_panel_scroll
                .saturating_add(amount as u16)
                .min(96)
        };
    }

    pub fn move_overview_selection(&mut self, delta: i8) {
        let count = self.overview_region_targets().len();
        if count == 0 {
            self.overview_selection = 0;
            return;
        }
        self.overview_selection = (self.overview_selection as isize + isize::from(delta))
            .clamp(0, count.saturating_sub(1) as isize) as usize;
    }

    pub fn open_selected_overview_region(&mut self) {
        let Some(region_id) = self
            .overview_region_targets()
            .get(self.overview_selection)
            .map(|region_id| (*region_id).to_owned())
        else {
            return;
        };
        self.workspace = Workspace::Planet;
        self.selected_region_id = Some(region_id);
        self.region_panel_scroll = 0;
        self.pending_action = None;
        self.notice = None;
        self.focus_selected_region(false);
    }

    pub fn overview_region_targets(&self) -> Vec<&str> {
        let Some(planet) = &self.planet else {
            return Vec::new();
        };
        let mut targets = Vec::new();
        if let Some(mission) = &planet.active_mission {
            targets.push(mission.target_region_id.as_str());
        }
        if let Some(mission) = &planet.active_extraction {
            targets.push(mission.target_region_id.as_str());
        }
        if let Some(construction) = &planet.active_construction {
            targets.push(construction.region_id.as_str());
        }
        targets.extend(
            planet
                .recent_reports
                .iter()
                .rev()
                .map(|report| report.region_id.as_str()),
        );
        targets
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

    fn clamp_overview_selection(&mut self) {
        self.overview_selection = self
            .overview_selection
            .min(self.overview_region_targets().len().saturating_sub(1));
    }

    fn focus_selected_region(&mut self, immediate: bool) {
        let Some(region) = self.selected_region() else {
            return;
        };
        let longitude_sector = region.position.longitude_sector;
        let latitude_band = region.position.latitude_band;
        self.globe_target_yaw = f64::from(longitude_sector) * std::f64::consts::PI / 3.0;
        let latitude = std::f64::consts::FRAC_PI_2
            - (f64::from(latitude_band) + 0.5) * std::f64::consts::FRAC_PI_4;
        self.globe_target_pitch = (latitude * 0.38).clamp(-0.45, 0.45);
        if immediate {
            self.globe_yaw = self.globe_target_yaw;
            self.globe_pitch = self.globe_target_pitch;
        }
    }
}

fn ease(current: f64, target: f64, elapsed: f64) -> f64 {
    let amount = 1.0 - (-CAMERA_RESPONSE * elapsed).exp();
    current + (target - current) * amount
}

fn ease_angle(current: f64, target: f64, elapsed: f64) -> f64 {
    let delta = (target - current + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU)
        - std::f64::consts::PI;
    normalize_angle(current + delta * (1.0 - (-CAMERA_RESPONSE * elapsed).exp()))
}

fn normalize_angle(angle: f64) -> f64 {
    angle.rem_euclid(std::f64::consts::TAU)
}

#[cfg(test)]
fn angle_distance(left: f64, right: f64) -> f64 {
    ((left - right + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI)
        .abs()
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
        app.select_next_band();
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

    #[test]
    fn globe_waits_thirty_seconds_then_rotates_without_changing_selection() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([2, 2, 2]);
        let start = Instant::now();
        let mut app = App::default();
        app.apply_snapshot(engine.status(), start);
        app.apply_planet(engine.planet().unwrap(), start);
        app.connected();
        app.last_input = start;
        app.last_animation_frame = start;

        let initial_yaw = app.globe_yaw;
        app.move_selection(0, 1);
        let selected = app.selected_region_id.clone();
        app.advance_globe_animation(start + Duration::from_millis(200));
        assert_ne!(app.globe_yaw, initial_yaw);
        assert!(
            angle_distance(app.globe_yaw, app.globe_target_yaw)
                < angle_distance(initial_yaw, app.globe_target_yaw)
        );

        app.globe_yaw = app.globe_target_yaw;
        app.last_animation_frame = start + Duration::from_millis(200);
        let focused_yaw = app.globe_yaw;
        app.advance_globe_animation(start + IDLE_DELAY - Duration::from_millis(1));
        assert_eq!(app.globe_yaw, focused_yaw);
        app.advance_globe_animation(start + IDLE_DELAY + Duration::from_secs(1));
        assert_ne!(app.globe_yaw, focused_yaw);
        assert_eq!(app.selected_region_id, selected);

        app.record_input(start + IDLE_DELAY + Duration::from_secs(1));
        let rotating_yaw = app.globe_yaw;
        app.advance_globe_animation(start + IDLE_DELAY + Duration::from_millis(1_200));
        assert!(
            angle_distance(app.globe_yaw, app.globe_target_yaw)
                < angle_distance(rotating_yaw, app.globe_target_yaw)
        );

        app.pending_action = Some(ContextAction::Recall);
        let confirmation_yaw = app.globe_yaw;
        app.advance_globe_animation(start + IDLE_DELAY * 3);
        assert_eq!(app.globe_yaw, confirmation_yaw);
    }

    #[test]
    fn workspace_cycle_and_overview_selection_stay_bounded() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([2, 2, 2]);
        let target = engine
            .planet()
            .unwrap()
            .regions
            .into_iter()
            .find(|region| region.within_scout_range && !region.core)
            .unwrap();
        engine.deploy_scout(&target.id);

        let mut app = App::default();
        app.apply_planet(engine.planet().unwrap(), Instant::now());
        app.cycle_workspace(false);
        assert_eq!(app.workspace, Workspace::Overview);
        app.move_overview_selection(1);
        assert_eq!(app.overview_selection, 0);
        app.open_selected_overview_region();
        assert_eq!(app.workspace, Workspace::Planet);
        assert_eq!(app.selected_region_id.as_deref(), Some(target.id.as_str()));

        app.cycle_workspace(true);
        assert_eq!(app.workspace, Workspace::System);
        app.cycle_workspace(true);
        assert_eq!(app.workspace, Workspace::Overview);
    }
}
