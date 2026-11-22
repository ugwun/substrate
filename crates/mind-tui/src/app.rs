use crate::{
    boot::{BootIntroState, BootMode},
    connection::PlanetActionKind,
};
use mind_core::protocol::{
    ArchiveRecordSnapshot, BaseModuleKind, DiagnosticPulseResult, EngineEvent, ExtractorLoadout,
    ExtractorState, NewPlaythroughResult, OnboardingCompletionResult, OperationalProfile,
    PlanetActionResult, PlanetSnapshot, RegionSnapshot, ResearchProject, ResourceStock, ScoutState,
    StatusSnapshot, SubregionKnowledge, SubregionSnapshot,
};
use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};

const EVENT_CACHE_CAPACITY: usize = 256;
const IDLE_DELAY: Duration = Duration::from_secs(30);
pub const LOCAL_FIELD_UNLOCK_GUIDANCE: &str =
    "Build a relay, extend survey range, activate a research profile, then complete one automated operation.";
pub const LOCAL_FIELD_SURVEY_GUIDANCE: &str = "Complete a regional survey here first.";
const IDLE_ROTATION_SPEED: f64 = 0.085;
const CAMERA_RESPONSE: f64 = 7.5;
const MAX_FRAME_DELTA: Duration = Duration::from_millis(250);
const TRANSITION_DURATION: Duration = Duration::from_millis(180);
const DISCOVERY_FLASH_DURATION: Duration = Duration::from_millis(1_200);
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
    Probes,
    Research,
    Archive,
    SystemMap,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionMenuState {
    Selecting,
    ConfirmingNewPlaythrough,
    StartingNewPlaythrough,
    StoppingDaemon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionMenuAction {
    Resume,
    Detach,
    StopDaemon,
    NewPlaythrough,
}

impl SessionMenuAction {
    const ALL: [Self; 4] = [
        Self::Resume,
        Self::Detach,
        Self::StopDaemon,
        Self::NewPlaythrough,
    ];
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextAction {
    Deploy(String),
    Recall,
    Extract(String),
    ConfigureExtractor(ExtractorLoadout),
    ConstructRelay(String),
    QueueSurvey(String),
    RemoveQueuedSurvey(String),
    SetExtractionRoute(String),
    ClearExtractionRoute,
    StartResearch(ResearchProject),
    StartArchiveExperiment,
    ActivateProfile(OperationalProfile),
    SurveySubregion(String),
    ExtractSubregion {
        region_id: String,
        subregion_id: String,
    },
    EstablishBase(String),
    MaintainBase(String),
    ConstructBaseModule {
        subregion_id: String,
        module: BaseModuleKind,
    },
    SetBaseModuleEnabled {
        subregion_id: String,
        module: BaseModuleKind,
        enabled: bool,
    },
}

/// Largest useful scroll offset per panel, recorded by the renderer from real wrapped content so
/// stored offsets never run past the end (PgUp then responds immediately).
#[derive(Debug)]
pub struct ScrollLimits {
    pub help: std::cell::Cell<u16>,
    pub region_panel: std::cell::Cell<u16>,
    pub probe_panel: std::cell::Cell<u16>,
    pub archive_detail: std::cell::Cell<u16>,
    pub overview_status: std::cell::Cell<u16>,
    /// Furthest System event scroll (in shown events) that still fills the pane.
    pub events: std::cell::Cell<u16>,
    /// Visible rows per panel, recorded by the renderer so a page step never skips content.
    pub help_rows: std::cell::Cell<u16>,
    pub region_rows: std::cell::Cell<u16>,
    pub probe_rows: std::cell::Cell<u16>,
    pub archive_rows: std::cell::Cell<u16>,
    pub overview_rows: std::cell::Cell<u16>,
    /// Whole System events visible on the current page.
    pub event_rows: std::cell::Cell<u16>,
    /// Where the next Return Summary page starts (an activity id, or `u64::MAX` for the new
    /// evidence page); 0 when the page on screen is the last one.
    pub summary_next: std::cell::Cell<u64>,
}

impl ScrollLimits {
    /// One page: the visible rows minus one for context, at least one row.
    #[must_use]
    pub fn page(rows: &std::cell::Cell<u16>) -> i16 {
        i16::try_from(rows.get().saturating_sub(1).max(1)).unwrap_or(i16::MAX)
    }
}

impl Default for ScrollLimits {
    fn default() -> Self {
        let open = || std::cell::Cell::new(u16::MAX);
        Self {
            help: open(),
            region_panel: open(),
            probe_panel: open(),
            archive_detail: open(),
            overview_status: open(),
            events: open(),
            help_rows: std::cell::Cell::new(5),
            region_rows: std::cell::Cell::new(5),
            probe_rows: std::cell::Cell::new(5),
            archive_rows: std::cell::Cell::new(5),
            overview_rows: std::cell::Cell::new(5),
            event_rows: std::cell::Cell::new(5),
            summary_next: std::cell::Cell::new(0),
        }
    }
}

fn is_shown_event(event: &EngineEvent) -> bool {
    event.kind != mind_core::protocol::EngineEventKind::ActivityReviewed
}

fn clamped_scroll(current: u16, amount: i16, limit: u16) -> u16 {
    let current = current.min(limit);
    if amount.is_negative() {
        current.saturating_sub(amount.unsigned_abs())
    } else {
        current.saturating_add(amount as u16).min(limit)
    }
}

#[derive(Debug)]
pub struct App {
    pub connection: ConnectionState,
    pub snapshot: Option<StatusSnapshot>,
    pub planet: Option<PlanetSnapshot>,
    pub events: VecDeque<EngineEvent>,
    pub observed_tick_rate: Option<f64>,
    pub last_update: Option<Instant>,
    pub planet_received_at: Option<Instant>,
    pub last_frame_duration_us: u64,
    pub frame_count: u64,
    pub ansi16: bool,
    pub ansi256: bool,
    pub boot_intro: Option<BootIntroState>,
    pub transition_started_at: Option<Instant>,
    pub globe_cache: RefCell<crate::globe::GlobeCache>,
    pub event_scroll: usize,
    pub follow_events: bool,
    pub show_help: bool,
    pub help_scroll: u16,
    pub rig_morph_at: Option<Instant>,
    pub session_menu: Option<SessionMenuState>,
    pub session_menu_selection: usize,
    pub session_menu_message: Option<String>,
    pub notice: Option<String>,
    /// The notice currently on screen and when it first appeared, for expiry.
    notice_shown: Option<(String, Instant)>,
    pub selected_band: usize,
    pub band_positions: [u8; 3],
    pub onboarding_view: OnboardingView,
    pub workspace: Workspace,
    pub selected_region_id: Option<String>,
    pub local_view: bool,
    pub selected_subregion_slot: usize,
    pub region_panel_scroll: u16,
    pub overview_selection: usize,
    pub overview_status_scroll: u16,
    pub probe_selection: usize,
    pub probe_panel_scroll: u16,
    pub research_selection: usize,
    pub archive_selection: usize,
    pub archive_detail_scroll: u16,
    pub scroll_limits: ScrollLimits,
    /// The System Map became available while its introducing transmission was still queued.
    pub system_map_deferred: bool,
    pub show_return_summary: bool,
    /// First activity id on the Return Summary page being shown (0 = from the oldest event,
    /// `u64::MAX` = the new evidence page). Anchoring to an id keeps resizes from skipping events.
    pub return_summary_anchor: u64,
    pub active_transmission_id: Option<String>,
    pub replay_transmission_id: Option<String>,
    pub transmission_opened_at: Option<Instant>,
    pub transmission_revealed: bool,
    pub transmission_scroll: u16,
    pub show_chapter_card: bool,
    pub system_map_opened_at: Option<Instant>,
    pub pending_action: Option<ContextAction>,
    pub globe_yaw: f64,
    pub globe_pitch: f64,
    pub globe_phase: f64,
    previous_tick_sample: Option<(u64, Instant)>,
    last_input: Instant,
    last_animation_frame: Instant,
    globe_target_yaw: f64,
    globe_target_pitch: f64,
    last_seen_chapter: Option<u8>,
    suppress_initial_return_summary: bool,
    pending_archive_ack_id: Option<String>,
    last_auto_acked_activity_id: u64,
    boot_intro_enabled: bool,
    boot_intro_armed: bool,
    pub animations_enabled: bool,
    recent_discoveries: Vec<(String, Instant)>,
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
            planet_received_at: None,
            last_frame_duration_us: 0,
            frame_count: 0,
            ansi16: false,
            ansi256: false,
            boot_intro: None,
            transition_started_at: None,
            globe_cache: RefCell::new(crate::globe::GlobeCache::default()),
            event_scroll: 0,
            follow_events: true,
            show_help: false,
            help_scroll: 0,
            rig_morph_at: None,
            session_menu: None,
            session_menu_selection: 0,
            session_menu_message: None,
            notice: None,
            notice_shown: None,
            selected_band: 0,
            band_positions: [1, 5, 9],
            onboarding_view: OnboardingView::Adjusting,
            workspace: Workspace::Planet,
            selected_region_id: None,
            local_view: false,
            selected_subregion_slot: 0,
            region_panel_scroll: 0,
            overview_selection: 0,
            overview_status_scroll: 0,
            probe_selection: 0,
            probe_panel_scroll: 0,
            research_selection: 0,
            archive_selection: 0,
            archive_detail_scroll: 0,
            scroll_limits: ScrollLimits::default(),
            system_map_deferred: false,
            show_return_summary: false,
            return_summary_anchor: 0,
            active_transmission_id: None,
            replay_transmission_id: None,
            transmission_opened_at: None,
            transmission_revealed: false,
            transmission_scroll: 0,
            show_chapter_card: false,
            system_map_opened_at: None,
            pending_action: None,
            globe_yaw: 0.0,
            globe_pitch: 0.0,
            globe_phase: 0.0,
            previous_tick_sample: None,
            last_input: now,
            last_animation_frame: now,
            globe_target_yaw: 0.0,
            globe_target_pitch: 0.0,
            last_seen_chapter: None,
            suppress_initial_return_summary: false,
            pending_archive_ack_id: None,
            last_auto_acked_activity_id: 0,
            boot_intro_enabled: false,
            boot_intro_armed: false,
            animations_enabled: true,
            recent_discoveries: Vec::new(),
        }
    }
}

impl App {
    /// Clear an action notice a few seconds after it first appeared. Returns when the current
    /// notice expires (for the frame scheduler) and whether a notice was just cleared.
    pub fn expire_notice(&mut self, now: Instant) -> (Option<Instant>, bool) {
        const NOTICE_TTL: Duration = Duration::from_secs(6);
        let Some(text) = self.notice.as_ref() else {
            self.notice_shown = None;
            return (None, false);
        };
        match &self.notice_shown {
            Some((shown, since)) if shown == text => {
                if now.saturating_duration_since(*since) >= NOTICE_TTL {
                    self.notice = None;
                    self.notice_shown = None;
                    (None, true)
                } else {
                    (Some(*since + NOTICE_TTL), false)
                }
            }
            _ => {
                self.notice_shown = Some((text.clone(), now));
                (Some(now + NOTICE_TTL), false)
            }
        }
    }

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
        let returned_to_onboarding = self
            .snapshot
            .as_ref()
            .is_some_and(|current| current.onboarding.complete && !snapshot.onboarding.complete);
        if returned_to_onboarding {
            self.reset_local_playthrough_state();
            self.notice = Some("NEW PLAYTHROUGH READY // COHERENCE REQUIRED".into());
            if self.boot_intro_enabled {
                self.boot_intro = Some(BootIntroState::start(received_at, BootMode::FullBoot));
            }
        } else if self.boot_intro_armed && self.session_menu.is_none() {
            self.boot_intro_armed = false;
            let mode = if snapshot.onboarding.complete {
                BootMode::QuickPowerOn
            } else {
                BootMode::FullBoot
            };
            self.boot_intro = Some(BootIntroState::start(received_at, mode));
        }
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
                if is_shown_event(event) {
                    inserted += 1;
                }
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
        if self
            .planet
            .as_ref()
            .is_some_and(|current| current.revision > planet.revision)
        {
            return;
        }
        let initial_planet = self.planet.is_none();
        let suppress_return_summary = std::mem::take(&mut self.suppress_initial_return_summary)
            || self.onboarding_view == OnboardingView::Submitting;
        let system_map_was_available = self
            .planet
            .as_ref()
            .is_some_and(|planet| planet.system_map.available);
        let system_map_became_available = planet.system_map.available && !system_map_was_available;
        let selected_is_valid = self
            .selected_region_id
            .as_ref()
            .is_some_and(|selected| planet.regions.iter().any(|region| &region.id == selected));
        let selection_changed = !selected_is_valid;
        if !selected_is_valid {
            self.selected_region_id = Some(planet.core_region_id.clone());
            self.local_view = false;
            self.selected_subregion_slot = 0;
        }
        let chapter_advanced = self
            .last_seen_chapter
            .is_some_and(|previous| planet.campaign.chapter_number > previous);
        self.last_seen_chapter = Some(planet.campaign.chapter_number);
        if let Some(pending_id) = self.pending_archive_ack_id.as_deref() {
            if !planet
                .archive
                .unpresented_record_ids
                .iter()
                .any(|id| id == pending_id)
            {
                self.pending_archive_ack_id = None;
            }
        }
        let next_unpresented = planet
            .archive
            .unpresented_record_ids
            .iter()
            .find(|id| self.pending_archive_ack_id.as_deref() != Some(id.as_str()))
            .cloned();
        if next_unpresented != self.active_transmission_id {
            self.active_transmission_id = next_unpresented.clone();
            self.transmission_opened_at = next_unpresented.as_ref().map(|_| received_at);
            self.transmission_revealed = false;
            self.transmission_scroll = 0;
        }
        let mut operation_resolved = false;
        if let Some(previous) = self.planet.as_ref() {
            if previous.extractor.loadout != planet.extractor.loadout {
                self.rig_morph_at = Some(received_at);
            }
            if previous.revision != planet.revision {
                if (previous.network.active_site_survey.is_some()
                    && planet.network.active_site_survey.is_none())
                    || (previous.active_extraction.is_some() && planet.active_extraction.is_none())
                    || (previous.active_construction.is_some()
                        && planet.active_construction.is_none())
                    || (previous.research.active.is_some() && planet.research.active.is_none())
                    || (previous
                        .active_mission
                        .as_ref()
                        .is_some_and(|m| m.phase != ScoutState::Holding)
                        && planet
                            .active_mission
                            .as_ref()
                            .is_none_or(|m| m.phase == ScoutState::Holding))
                {
                    operation_resolved = true;
                }
                let old_regions: HashMap<&str, _> = previous
                    .regions
                    .iter()
                    .map(|r| (r.id.as_str(), r.knowledge))
                    .collect();
                for region in &planet.regions {
                    if old_regions
                        .get(region.id.as_str())
                        .is_some_and(|&old| region.knowledge > old)
                    {
                        self.recent_discoveries
                            .push((region.id.clone(), received_at));
                    }
                }
                let old_subregions: HashMap<&str, SubregionKnowledge> = previous
                    .network
                    .subregions
                    .iter()
                    .map(|s| (s.id.as_str(), s.knowledge))
                    .collect();
                for site in &planet.network.subregions {
                    let increased = old_subregions
                        .get(site.id.as_str())
                        .map_or(site.knowledge > SubregionKnowledge::Unknown, |&old| {
                            site.knowledge > old
                        });
                    if increased {
                        self.recent_discoveries.push((site.id.clone(), received_at));
                    }
                }
            }
        }
        self.recent_discoveries.retain(|(_, at)| {
            received_at.saturating_duration_since(*at) < DISCOVERY_FLASH_DURATION
        });
        self.planet = Some(planet);
        self.planet_received_at = Some(received_at);
        if operation_resolved {
            self.notice = None;
        }
        // Hold the System Map until the final observation's transmission has been presented, so
        // the map never pre-empts the record that introduces it.
        let final_record_pending = self
            .planet
            .as_ref()
            .is_some_and(|planet| !planet.archive.unpresented_record_ids.is_empty());
        if system_map_became_available && final_record_pending {
            self.system_map_deferred = true;
        } else if system_map_became_available || (self.system_map_deferred && !final_record_pending)
        {
            self.system_map_deferred = false;
            self.workspace = Workspace::SystemMap;
            self.system_map_opened_at = Some(received_at);
            self.pending_action = None;
            self.notice = Some("SOLAR HORIZON RESOLVED // SYSTEM MAP OPEN".into());
        } else if chapter_advanced && self.active_transmission_id.is_none() {
            self.show_chapter_card = true;
        }
        if initial_planet
            && !suppress_return_summary
            && self
                .planet
                .as_ref()
                .is_some_and(|planet| !planet.unreviewed_activity.is_empty())
        {
            self.show_return_summary = true;
            self.return_summary_anchor = 0;
        }
        self.clamp_overview_selection();
        self.clamp_archive_selection();
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

    pub fn open_session_menu(&mut self) {
        self.show_help = false;
        self.session_menu = Some(SessionMenuState::Selecting);
        self.session_menu_selection = 0;
        self.session_menu_message = None;
    }

    pub fn close_session_menu(&mut self) {
        if self.session_menu == Some(SessionMenuState::Selecting) {
            let now = Instant::now();
            self.session_menu = None;
            self.session_menu_message = None;
            if self.active_transmission_id.is_some() {
                self.transmission_opened_at = Some(now);
            }
            self.start_armed_boot_intro(now);
        }
    }

    pub fn start_armed_boot_intro(&mut self, now: Instant) {
        if !self.boot_intro_armed {
            return;
        }
        let Some(snapshot) = self.snapshot.as_ref() else {
            return;
        };
        self.boot_intro_armed = false;
        let mode = if snapshot.onboarding.complete {
            BootMode::QuickPowerOn
        } else {
            BootMode::FullBoot
        };
        self.boot_intro = Some(BootIntroState::start(now, mode));
    }

    pub fn move_session_menu_selection(&mut self, delta: i8) {
        let count = SessionMenuAction::ALL.len() as isize;
        self.session_menu_selection =
            (self.session_menu_selection as isize + isize::from(delta)).rem_euclid(count) as usize;
        self.session_menu_message = None;
    }

    pub fn selected_session_menu_action(&self) -> SessionMenuAction {
        SessionMenuAction::ALL[self
            .session_menu_selection
            .min(SessionMenuAction::ALL.len() - 1)]
    }

    pub fn confirm_new_playthrough(&mut self) {
        self.session_menu = Some(SessionMenuState::ConfirmingNewPlaythrough);
        self.session_menu_message = None;
    }

    pub fn return_to_session_menu(&mut self) {
        self.session_menu = Some(SessionMenuState::Selecting);
        self.session_menu_message = None;
    }

    pub fn begin_new_playthrough(&mut self) {
        self.session_menu = Some(SessionMenuState::StartingNewPlaythrough);
        self.session_menu_message = None;
    }

    pub fn begin_daemon_stop(&mut self) {
        self.session_menu = Some(SessionMenuState::StoppingDaemon);
        self.session_menu_message = None;
    }

    pub fn session_action_unavailable(&mut self) {
        self.session_menu_message = Some("A live daemon connection is required.".into());
    }

    pub fn new_playthrough_completed(&mut self, result: NewPlaythroughResult) {
        if result.ok {
            self.reset_local_playthrough_state();
            if self.boot_intro_enabled {
                self.boot_intro_armed = true;
            }
            self.notice = Some("NEW PLAYTHROUGH READY // COHERENCE REQUIRED".into());
        } else {
            self.session_menu = Some(SessionMenuState::Selecting);
            self.session_menu_message = Some(format!(
                "Could not create playthrough: {}. Existing progress is unchanged.",
                result.code.as_deref().unwrap_or("unknown_error")
            ));
        }
    }

    pub fn lifecycle_failed(&mut self, error: impl Into<String>) {
        self.session_menu = Some(SessionMenuState::Selecting);
        self.session_menu_message = Some(format!("Lifecycle request failed: {}", error.into()));
    }

    fn reset_local_playthrough_state(&mut self) {
        let connection = self.connection.clone();
        let ansi16 = self.ansi16;
        let ansi256 = self.ansi256;
        let boot_intro_enabled = self.boot_intro_enabled;
        let animations_enabled = self.animations_enabled;
        *self = Self::default();
        self.connection = connection;
        self.ansi16 = ansi16;
        self.ansi256 = ansi256;
        self.boot_intro_enabled = boot_intro_enabled;
        self.animations_enabled = animations_enabled;
    }

    pub fn color_tier(&self, options: crate::ui::RenderOptions) -> crate::raster::ColorTier {
        if options.ascii {
            crate::raster::ColorTier::Ascii
        } else if options.no_color {
            crate::raster::ColorTier::Monochrome
        } else if self.ansi16 {
            crate::raster::ColorTier::Ansi16
        } else if self.ansi256 {
            crate::raster::ColorTier::Ansi256
        } else {
            crate::raster::ColorTier::TrueColor
        }
    }

    pub fn enable_boot_intro(&mut self) {
        self.boot_intro_enabled = true;
        self.boot_intro_armed = true;
    }

    pub fn boot_intro_active(&self, now: Instant) -> bool {
        self.boot_intro.is_some_and(|boot| boot.is_active(now))
    }

    pub fn skip_boot_intro(&mut self) {
        if let Some(boot) = self.boot_intro.as_mut() {
            boot.skip();
        }
        if self.active_transmission_id.is_some() {
            self.transmission_opened_at = Some(Instant::now());
        }
    }

    pub fn active_discovery_flash_ids(&self, now: Instant) -> Vec<&str> {
        self.recent_discoveries
            .iter()
            .filter(|(_, at)| now.saturating_duration_since(*at) < DISCOVERY_FLASH_DURATION)
            .map(|(id, _)| id.as_str())
            .collect()
    }

    pub fn has_discovery_flash(&self, id: &str, now: Instant) -> bool {
        self.recent_discoveries.iter().any(|(entry, at)| {
            entry == id && now.saturating_duration_since(*at) < DISCOVERY_FLASH_DURATION
        })
    }

    pub fn select_probe(&mut self, index: usize) {
        self.probe_selection = index % crate::probes::PROBE_COUNT;
        self.probe_panel_scroll = 0;
        self.notice = None;
    }

    pub fn move_probe_selection(&mut self, delta: i8) {
        let count = crate::probes::PROBE_COUNT as isize;
        self.probe_selection =
            (self.probe_selection as isize + isize::from(delta)).rem_euclid(count) as usize;
        self.probe_panel_scroll = 0;
        self.notice = None;
    }

    pub fn scroll_probe_panel(&mut self, amount: i16) {
        self.probe_panel_scroll = clamped_scroll(
            self.probe_panel_scroll,
            amount,
            self.scroll_limits.probe_panel.get(),
        );
    }

    pub fn focus_selected_probe_region(&mut self) -> bool {
        let Some(planet) = self.planet.as_ref() else {
            return false;
        };
        let target_region = if self.probe_selection % crate::probes::PROBE_COUNT == 1 {
            planet
                .active_extraction
                .as_ref()
                .map(|m| m.target_region_id.clone())
                .unwrap_or_else(|| planet.core_region_id.clone())
        } else {
            planet
                .active_mission
                .as_ref()
                .map(|m| m.target_region_id.clone())
                .unwrap_or_else(|| planet.scout.region_id.clone())
        };
        if planet.regions.iter().any(|r| r.id == target_region) {
            self.selected_region_id = Some(target_region);
            self.workspace = Workspace::Planet;
            self.local_view = false;
            self.pending_action = None;
            self.region_panel_scroll = 0;
            self.focus_selected_region(false);
            true
        } else {
            false
        }
    }

    pub fn trigger_transition(&mut self, now: Instant) {
        if self.animations_enabled {
            self.transition_started_at = Some(now);
        }
    }

    pub fn clear_transition(&mut self) {
        self.transition_started_at = None;
    }

    pub fn transition_progress(&self, now: Instant) -> Option<f64> {
        let started = self.transition_started_at?;
        let elapsed = now.saturating_duration_since(started);
        if elapsed >= TRANSITION_DURATION {
            None
        } else {
            Some((elapsed.as_secs_f64() / TRANSITION_DURATION.as_secs_f64()).clamp(0.0, 1.0))
        }
    }

    pub fn record_frame_metrics(&mut self, duration: Duration) {
        self.last_frame_duration_us = duration.as_micros().min(u128::from(u64::MAX)) as u64;
        self.frame_count = self.frame_count.saturating_add(1);
    }

    /// Presentation-only interpolation for non-pausing daemon timers between 1 Hz pushes.
    /// Never completes an operation locally; clamps at 0 ms so the daemon remains sole authority.
    pub fn display_remaining_ms(&self, remaining_ms: u64, now: Instant) -> u64 {
        if remaining_ms == 0 || !matches!(self.connection, ConnectionState::Connected) {
            return remaining_ms;
        }
        let elapsed_ms = self
            .planet_received_at
            .map_or(0, |received| {
                now.saturating_duration_since(received).as_millis() as u64
            })
            .min(1_500);
        remaining_ms.saturating_sub(elapsed_ms)
    }

    /// Adaptive animation scheduler: 30 FPS during motion, 2-4 FPS for ambient indicators,
    /// `None` (zero redraws) when the screen is static.
    pub fn next_frame_deadline(&self, now: Instant) -> Option<Instant> {
        const ACTIVE_FRAME: Duration = Duration::from_millis(33);
        const WAVEFORM_FRAME: Duration = Duration::from_millis(66);
        const AMBIENT_FRAME: Duration = Duration::from_millis(250);
        const BEACON_FRAME: Duration = Duration::from_millis(350);

        if self.session_menu.is_some() || self.show_help || self.snapshot.is_none() {
            return None;
        }
        if self.boot_intro_active(now) {
            return Some(now + ACTIVE_FRAME);
        }
        if self.show_return_summary || self.show_chapter_card {
            return None;
        }
        if self.transition_progress(now).is_some() {
            return Some(now + ACTIVE_FRAME);
        }
        if self.onboarding_pending() {
            return Some(now + ACTIVE_FRAME);
        }
        if self.active_transmission_record().is_some() {
            if !self.transmission_revealed {
                let elapsed = self
                    .transmission_opened_at
                    .map_or(Duration::from_secs(10), |opened| {
                        now.saturating_duration_since(opened)
                    });
                if elapsed < Duration::from_millis(2_500) {
                    return Some(now + ACTIVE_FRAME);
                }
            }
            return Some(now + AMBIENT_FRAME);
        }
        if !matches!(self.connection, ConnectionState::Connected) {
            return None;
        }

        match self.workspace {
            Workspace::Planet => {
                let planet = self.planet.as_ref()?;
                if !self.active_discovery_flash_ids(now).is_empty() {
                    return Some(now + ACTIVE_FRAME);
                }
                if self.local_view {
                    let active_local = planet.network.active_site_survey.is_some()
                        || !planet.network.active_projects.is_empty()
                        || planet.active_extraction.is_some()
                        || !planet.network.bases.is_empty();
                    return active_local.then_some(now + AMBIENT_FRAME);
                }
                let idle_at = self.last_input + IDLE_DELAY;
                let idle_rotating = now >= idle_at;
                let yaw_delta = ((self.globe_target_yaw - self.globe_yaw + std::f64::consts::PI)
                    .rem_euclid(std::f64::consts::TAU)
                    - std::f64::consts::PI)
                    .abs();
                let pitch_delta = (self.globe_target_pitch - self.globe_pitch).abs();
                let camera_easing = yaw_delta > 0.002 || pitch_delta > 0.002;
                let globe_motion = planet
                    .active_mission
                    .as_ref()
                    .is_some_and(|m| m.phase != ScoutState::Holding)
                    || planet.active_extraction.is_some()
                    || planet.active_construction.is_some()
                    || !planet.network.active_projects.is_empty();
                if idle_rotating || camera_easing || globe_motion {
                    return Some(now + ACTIVE_FRAME);
                }
                if planet.campaign.lead_region_id.is_some() {
                    return Some((now + BEACON_FRAME).min(idle_at));
                }
                Some(idle_at)
            }
            Workspace::SystemMap => {
                let pullback_active = self.system_map_opened_at.is_some_and(|opened| {
                    now.saturating_duration_since(opened) < Duration::from_millis(2_700)
                });
                if pullback_active {
                    Some(now + ACTIVE_FRAME)
                } else {
                    Some(now + BEACON_FRAME)
                }
            }
            Workspace::Archive => {
                let running = self
                    .planet
                    .as_ref()
                    .and_then(|planet| planet.archive.experiment.as_ref())
                    .is_some_and(|exp| exp.active && exp.running);
                running.then_some(now + WAVEFORM_FRAME)
            }
            Workspace::Probes => {
                if self.rig_morph_progress(now).is_some() {
                    Some(now + ACTIVE_FRAME)
                } else {
                    Some(now + AMBIENT_FRAME)
                }
            }
            Workspace::Overview | Workspace::Research => {
                let planet = self.planet.as_ref()?;
                let active_timers = planet.active_mission.is_some()
                    || planet.active_extraction.is_some()
                    || planet.active_construction.is_some()
                    || planet.research.active.is_some()
                    || !planet.network.active_projects.is_empty()
                    || planet.refining_remaining_ms > 0
                    || planet.extractor.rebuild_remaining_ms > 0;
                active_timers.then_some(now + AMBIENT_FRAME)
            }
            Workspace::System => None,
        }
    }

    pub fn advance_globe_animation(&mut self, now: Instant) {
        let elapsed = now
            .saturating_duration_since(self.last_animation_frame)
            .min(MAX_FRAME_DELTA)
            .as_secs_f64();
        self.last_animation_frame = now;
        self.globe_phase = (self.globe_phase + elapsed * 0.42).rem_euclid(1.0);
        if (self.session_menu.is_some() || self.boot_intro_active(now) || self.show_return_summary)
            && self.active_transmission_id.is_some()
        {
            self.transmission_opened_at = Some(now);
        }
        if self.workspace != Workspace::Planet
            || self.show_help
            || self.show_return_summary
            || self.show_chapter_card
            || self.active_transmission_id.is_some()
            || self.session_menu.is_some()
            || self.pending_action.is_some()
            || !matches!(self.connection, ConnectionState::Connected)
        {
            return;
        }
        if now.saturating_duration_since(self.last_input) >= IDLE_DELAY {
            self.globe_yaw = normalize_angle(self.globe_yaw + IDLE_ROTATION_SPEED * elapsed);
            self.globe_pitch = ease(self.globe_pitch, self.globe_target_pitch * 0.45, elapsed);
        } else {
            self.globe_yaw = ease_angle(self.globe_yaw, self.globe_target_yaw, elapsed);
            self.globe_pitch = ease(self.globe_pitch, self.globe_target_pitch, elapsed);
        }
    }

    /// Workspaces progressively unfold as the player encounters their corresponding systems.
    pub fn visible_workspaces(&self) -> Vec<Workspace> {
        let mut list = vec![Workspace::Planet];
        let Some(planet) = self.planet.as_ref() else {
            if self.workspace != Workspace::Planet
                && self.workspace != Workspace::Archive
                && self.workspace != Workspace::System
            {
                list.push(self.workspace);
            }
            list.push(Workspace::Archive);
            list.push(Workspace::System);
            return list;
        };
        let surveyed_count = planet
            .regions
            .iter()
            .filter(|r| r.knowledge >= mind_core::protocol::RegionKnowledge::Surveyed)
            .count();
        let has_stock = planet.raw_inventory.structural > 0
            || planet.raw_inventory.conductive > 0
            || planet.raw_inventory.silicate > 0
            || planet.refined_inventory.structural > 0
            || planet.refined_inventory.conductive > 0
            || planet.refined_inventory.silicate > 0;
        let has_ops = planet.active_mission.is_some()
            || planet.active_extraction.is_some()
            || planet.active_construction.is_some()
            || !planet.network.active_projects.is_empty()
            || !planet.recent_reports.is_empty();

        if self.workspace == Workspace::Overview || has_ops || has_stock || surveyed_count > 1 {
            list.push(Workspace::Overview);
        }
        if self.workspace == Workspace::Probes
            || has_ops
            || surveyed_count > 1
            || planet.extractor.loadout != ExtractorLoadout::Balanced
            || planet.extractor.rebuild_remaining_ms > 0
        {
            list.push(Workspace::Probes);
        }
        if self.workspace == Workspace::Research
            || planet.research.unlocked
            || planet.research.active.is_some()
        {
            list.push(Workspace::Research);
        }
        list.push(Workspace::Archive);
        if self.workspace == Workspace::SystemMap || planet.system_map.available {
            list.push(Workspace::SystemMap);
        }
        list.push(Workspace::System);
        list
    }

    pub fn select_visible_workspace(&mut self, one_based_index: usize) -> bool {
        let visible = self.visible_workspaces();
        let Some(&target) = one_based_index
            .checked_sub(1)
            .and_then(|idx| visible.get(idx))
        else {
            return false;
        };
        if self.workspace == target {
            return false;
        }
        self.workspace = target;
        self.pending_action = None;
        self.notice = None;
        true
    }

    pub fn cycle_workspace(&mut self, reverse: bool) {
        let visible = self.visible_workspaces();
        let current_idx = visible
            .iter()
            .position(|&ws| ws == self.workspace)
            .unwrap_or(0);
        let next_idx = if reverse {
            if current_idx == 0 {
                visible.len() - 1
            } else {
                current_idx - 1
            }
        } else {
            (current_idx + 1) % visible.len()
        };
        self.workspace = visible[next_idx];
        self.pending_action = None;
        self.notice = None;
    }

    pub fn move_selection(&mut self, latitude_delta: i8, longitude_delta: i8) {
        let Some(current) = self.selected_region().cloned() else {
            return;
        };
        let latitude = (i16::from(current.position.latitude_band) + i16::from(latitude_delta))
            .clamp(
                0,
                i16::from(
                    self.planet
                        .as_ref()
                        .map_or(1, |planet| planet.latitude_bands)
                        .saturating_sub(1),
                ),
            ) as u8;
        let longitude = (i16::from(current.position.longitude_sector) + i16::from(longitude_delta))
            .rem_euclid(i16::from(
                self.planet
                    .as_ref()
                    .map_or(1, |planet| planet.longitude_sectors),
            )) as u8;
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
        self.local_view = false;
        self.selected_subregion_slot = 0;
        self.notice = None;
    }

    pub fn enter_local_view(&mut self) {
        if self.selected_subregions().is_empty() {
            self.notice = Some(format!(
                "LOCAL FIELD UNAVAILABLE // {LOCAL_FIELD_SURVEY_GUIDANCE}"
            ));
            return;
        }
        self.local_view = true;
        self.selected_subregion_slot = self
            .selected_subregion_slot
            .min(self.selected_subregions().len().saturating_sub(1));
        if self
            .selected_subregion()
            .is_some_and(|site| site.knowledge == SubregionKnowledge::Unknown)
        {
            self.selected_subregion_slot = self
                .selected_subregions()
                .iter()
                .position(|site| site.knowledge > SubregionKnowledge::Unknown)
                .unwrap_or(0);
        }
        self.region_panel_scroll = 0;
        self.notice = None;
    }

    pub fn leave_local_view(&mut self) {
        self.local_view = false;
        self.pending_action = None;
        self.region_panel_scroll = 0;
    }

    pub fn move_subregion_selection(&mut self, row_delta: i8, column_delta: i8) {
        let sites = self.selected_subregions();
        if sites.is_empty() {
            self.selected_subregion_slot = 0;
            return;
        }
        let Some(current) = sites.get(self.selected_subregion_slot) else {
            self.selected_subregion_slot = 0;
            return;
        };
        let rows = self
            .planet
            .as_ref()
            .map_or(1, |planet| planet.network.subregion_rows);
        let columns = self
            .planet
            .as_ref()
            .map_or(1, |planet| planet.network.subregion_columns);
        let row = (i16::from(current.row) + i16::from(row_delta)).rem_euclid(i16::from(rows)) as u8;
        let column = (i16::from(current.column) + i16::from(column_delta))
            .rem_euclid(i16::from(columns)) as u8;
        if let Some(index) = sites
            .iter()
            .position(|site| site.row == row && site.column == column)
        {
            self.selected_subregion_slot = index;
        }
        self.pending_action = None;
        self.region_panel_scroll = 0;
    }

    pub fn selected_region(&self) -> Option<&RegionSnapshot> {
        let selected = self.selected_region_id.as_ref()?;
        self.planet
            .as_ref()?
            .regions
            .iter()
            .find(|region| &region.id == selected)
    }

    pub fn selected_subregions(&self) -> Vec<&SubregionSnapshot> {
        let Some(region_id) = self.selected_region_id.as_deref() else {
            return Vec::new();
        };
        self.planet
            .as_ref()
            .map(|planet| {
                let mut sites: Vec<_> = planet
                    .network
                    .subregions
                    .iter()
                    .filter(|subregion| subregion.region_id == region_id)
                    .collect();
                sites.sort_by_key(|site| (site.row, site.column));
                sites
            })
            .unwrap_or_default()
    }

    pub fn selected_subregion(&self) -> Option<&SubregionSnapshot> {
        self.selected_subregions()
            .get(self.selected_subregion_slot)
            .copied()
    }

    pub fn local_primary_action(&self) -> Option<ContextAction> {
        if self.workspace != Workspace::Planet || !self.local_view {
            return None;
        }
        self.subregion_survey_action()
            .or_else(|| self.subregion_extraction_action())
            .or_else(|| self.base_establishment_action())
    }

    pub fn subregion_survey_action(&self) -> Option<ContextAction> {
        let planet = self.planet.as_ref()?;
        let region = self.selected_region()?;
        let site = self.selected_subregion()?;
        let scout_present = planet.active_mission.as_ref().is_some_and(|mission| {
            mission.target_region_id == region.id && mission.phase == ScoutState::Holding
        });
        (self.local_view
            && site.knowledge == SubregionKnowledge::Detected
            && scout_present
            && planet.network.active_site_survey.is_none())
        .then(|| ContextAction::SurveySubregion(site.id.clone()))
    }

    pub fn subregion_extraction_action(&self) -> Option<ContextAction> {
        let planet = self.planet.as_ref()?;
        let region = self.selected_region()?;
        let site = self.selected_subregion()?;
        (self.local_view
            && planet.network.unlocked
            && site.knowledge >= SubregionKnowledge::Surveyed
            && site
                .deposit
                .as_ref()
                .is_some_and(|deposit| deposit.remaining > 0)
            && planet.extractor.state == mind_core::protocol::ExtractorState::Ready
            && region.within_scout_range)
            .then(|| ContextAction::ExtractSubregion {
                region_id: region.id.clone(),
                subregion_id: site.id.clone(),
            })
    }

    pub fn base_establishment_action(&self) -> Option<ContextAction> {
        let planet = self.planet.as_ref()?;
        let region = self.selected_region()?;
        let site = self.selected_subregion()?;
        let stock = planet.refined_inventory;
        (self.local_view
            && planet.network.unlocked
            && site.knowledge >= SubregionKnowledge::Surveyed
            && region.within_scout_range
            && site.base_id.is_none()
            && !planet
                .network
                .bases
                .iter()
                .any(|base| base.region_id == region.id)
            && !planet
                .network
                .active_projects
                .iter()
                .any(|project| project.region_id == region.id || project.subregion_id == site.id)
            && planet.network.active_projects.len() < planet.network.project_capacity
            && stock.structural >= 10
            && stock.conductive >= 6
            && stock.silicate >= 4)
            .then(|| ContextAction::EstablishBase(site.id.clone()))
    }

    pub fn base_module_action(&self, module: BaseModuleKind) -> Option<ContextAction> {
        let planet = self.planet.as_ref()?;
        let site = self.selected_subregion()?;
        let base = planet
            .network
            .bases
            .iter()
            .find(|base| base.subregion_id == site.id)?;
        let no_project = !planet
            .network
            .active_projects
            .iter()
            .any(|project| project.region_id == base.region_id);
        let project_slot = planet.network.active_projects.len() < planet.network.project_capacity;
        let has = |kind| base.modules.iter().any(|built| built.kind == kind);
        let prerequisites = match module {
            BaseModuleKind::PowerArray => true,
            BaseModuleKind::Communications | BaseModuleKind::Depot => {
                has(BaseModuleKind::PowerArray)
            }
            BaseModuleKind::Workshop | BaseModuleKind::Laboratory => {
                has(BaseModuleKind::PowerArray) && has(BaseModuleKind::Communications)
            }
            BaseModuleKind::Observatory => {
                has(BaseModuleKind::PowerArray)
                    && has(BaseModuleKind::Communications)
                    && planet
                        .research
                        .options
                        .iter()
                        .filter(|option| option.completed)
                        .count()
                        >= 2
                    && planet.network.horizon.surveyed_subregions
                        >= planet.network.horizon.required_subregions
                    && planet.network.bases.iter().any(|candidate| {
                        candidate
                            .modules
                            .iter()
                            .any(|built| built.kind == BaseModuleKind::Laboratory && built.active)
                    })
            }
        };
        let cost = module.cost();
        (!has(module)
            && base.integrity > 20
            && no_project
            && project_slot
            && prerequisites
            && stock_contains(planet.refined_inventory, cost))
        .then(|| ContextAction::ConstructBaseModule {
            subregion_id: site.id.clone(),
            module,
        })
    }

    pub fn base_module_mode_action(&self, module: BaseModuleKind) -> Option<ContextAction> {
        let planet = self.planet.as_ref()?;
        let site = self.selected_subregion()?;
        let built = planet
            .network
            .bases
            .iter()
            .find(|base| base.subregion_id == site.id)?
            .modules
            .iter()
            .find(|built| built.kind == module)?;
        Some(ContextAction::SetBaseModuleEnabled {
            subregion_id: site.id.clone(),
            module,
            enabled: !built.enabled,
        })
    }

    pub fn base_maintenance_action(&self) -> Option<ContextAction> {
        let planet = self.planet.as_ref()?;
        let site = self.selected_subregion()?;
        let base = planet
            .network
            .bases
            .iter()
            .find(|base| base.subregion_id == site.id)?;
        let no_project = !planet
            .network
            .active_projects
            .iter()
            .any(|project| project.region_id == base.region_id);
        let project_slot = planet.network.active_projects.len() < planet.network.project_capacity;
        let stock = planet.refined_inventory;
        (base.integrity < 100
            && no_project
            && project_slot
            && stock.structural >= 4
            && stock.conductive >= 2)
            .then(|| ContextAction::MaintainBase(site.id.clone()))
    }

    pub fn primary_region_action(&self) -> Option<ContextAction> {
        let planet = self.planet.as_ref()?;
        if self.workspace != Workspace::Planet {
            return None;
        }
        if self.local_view {
            return self.local_primary_action();
        }
        let region = self.selected_region()?;
        if region
            .deposit
            .as_ref()
            .is_some_and(|deposit| deposit.remaining > 0)
            && planet.extractor.state == mind_core::protocol::ExtractorState::Ready
            && region.within_scout_range
        {
            Some(ContextAction::Extract(region.id.clone()))
        } else if self.scout_can_deploy_to(planet, region) {
            Some(ContextAction::Deploy(region.id.clone()))
        } else {
            self.construction_action()
        }
    }

    pub fn scout_action(&self) -> Option<ContextAction> {
        let planet = self.planet.as_ref()?;
        let region = self.selected_region()?;
        (self.workspace == Workspace::Planet && self.scout_can_deploy_to(planet, region))
            .then(|| ContextAction::Deploy(region.id.clone()))
    }

    fn scout_can_deploy_to(&self, planet: &PlanetSnapshot, region: &RegionSnapshot) -> bool {
        if !region.within_scout_range || region.core {
            return false;
        }
        if planet.scout.state == ScoutState::Ready {
            return true;
        }
        planet.scout.state == ScoutState::Holding
            && planet.network.active_site_survey.is_none()
            && planet
                .active_mission
                .as_ref()
                .is_some_and(|mission| mission.target_region_id != region.id)
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

    pub fn extractor_loadout_action(&self) -> Option<ContextAction> {
        let planet = self.planet.as_ref()?;
        let allowed_workspace = match self.workspace {
            Workspace::Probes => true,
            Workspace::Planet => self.selected_region().is_some_and(|r| {
                r.core
                    || r.deposit.is_some()
                    || planet
                        .active_extraction
                        .as_ref()
                        .is_some_and(|m| m.target_region_id == r.id)
            }),
            _ => false,
        };
        (allowed_workspace
            && planet.extractor.state == ExtractorState::Ready
            && planet.active_extraction.is_none())
        .then(|| ContextAction::ConfigureExtractor(planet.extractor.loadout.next()))
    }

    pub fn planet_action_completed(&mut self, kind: PlanetActionKind, result: PlanetActionResult) {
        // Background auto-acknowledgement of live-watched activity is silent: it must not replace
        // the result notice of the player's last action or cancel an open confirmation.
        if kind == PlanetActionKind::Review && result.ok {
            return;
        }
        self.pending_action = None;
        self.notice = Some(if result.ok {
            match kind {
                PlanetActionKind::Deploy => "SCOUT DEPLOYED".into(),
                PlanetActionKind::Recall => "SCOUT RECALL INITIATED".into(),
                PlanetActionKind::Extract => "EXTRACTOR DEPLOYED".into(),
                PlanetActionKind::Loadout => "EXTRACTION RIG CONFIGURED".into(),
                PlanetActionKind::Construct => "RELAY CONSTRUCTION STARTED".into(),
                PlanetActionKind::Research => "RESEARCH COMMITTED".into(),
                PlanetActionKind::Archive => "ARCHIVE COMPARISON STARTED".into(),
                PlanetActionKind::Profile => "OPERATIONAL PROFILE ACTIVATED".into(),
                PlanetActionKind::Automation => {
                    if self
                        .planet
                        .as_ref()
                        .is_some_and(|p| p.survey_queue.unlocked)
                    {
                        "AUTOMATION UPDATED".into()
                    } else {
                        "SURVEY WAYPOINT UPDATED".into()
                    }
                }
                PlanetActionKind::Review => "ACTIVITY REVIEWED".into(),
                PlanetActionKind::ArchivePresentation => "OBSERVATION FILED IN ARCHIVE".into(),
                PlanetActionKind::LocalSurvey => "LOCAL SURVEY STARTED".into(),
                PlanetActionKind::BaseProject => "BASE PROJECT STARTED".into(),
                PlanetActionKind::ModuleMode => "MODULE POWER MODE UPDATED".into(),
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
            // A stale notice (for example an earlier "unavailable" message) must not linger
            // beside a fresh confirmation.
            self.notice = None;
            self.pending_action = Some(action);
            false
        }
    }

    pub fn cancel_confirmation_action(&mut self) {
        self.pending_action = None;
    }

    pub fn scroll_region_panel(&mut self, amount: i16) {
        self.region_panel_scroll = clamped_scroll(
            self.region_panel_scroll,
            amount,
            self.scroll_limits.region_panel.get(),
        );
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

    pub fn scroll_overview_status(&mut self, amount: i16) {
        self.overview_status_scroll = clamped_scroll(
            self.overview_status_scroll,
            amount,
            self.scroll_limits.overview_status.get(),
        );
    }

    pub fn move_research_selection(&mut self, delta: i8) {
        let count = self.research_entry_count();
        if count == 0 {
            self.research_selection = 0;
            return;
        }
        self.research_selection = (self.research_selection as isize + isize::from(delta))
            .clamp(0, count.saturating_sub(1) as isize) as usize;
    }

    pub fn selected_research_action(&self) -> Option<ContextAction> {
        let research = &self.planet.as_ref()?.research;
        if self.workspace != Workspace::Research || !research.unlocked {
            return None;
        }
        if self.research_selection < research.options.len() {
            let option = &research.options[self.research_selection];
            return option
                .available
                .then_some(ContextAction::StartResearch(option.project));
        }
        let profile = research
            .profiles
            .get(self.research_selection - research.options.len())?;
        let field_operation_active = self.planet.as_ref().is_some_and(|planet| {
            planet.active_mission.is_some() || planet.active_extraction.is_some()
        });
        (profile.unlocked && !profile.active && !field_operation_active)
            .then_some(ContextAction::ActivateProfile(profile.profile))
    }

    pub fn automation_action(&self) -> Option<ContextAction> {
        let planet = self.planet.as_ref()?;
        let region = self.selected_region()?;
        if self.workspace != Workspace::Planet {
            return None;
        }
        if planet
            .survey_queue
            .queued_region_ids
            .iter()
            .any(|queued| queued == &region.id)
        {
            return Some(ContextAction::RemoveQueuedSurvey(region.id.clone()));
        }
        let already_scout_target = planet
            .active_mission
            .as_ref()
            .is_some_and(|mission| mission.target_region_id == region.id);
        if !region.core
            && region.knowledge < mind_core::protocol::RegionKnowledge::Surveyed
            && region.within_scout_range
            && !already_scout_target
            && planet.survey_queue.queued_region_ids.len() < planet.survey_queue.capacity
        {
            return Some(ContextAction::QueueSurvey(region.id.clone()));
        }
        if !planet.extraction_route.unlocked {
            return None;
        }
        if planet.extraction_route.region_id.as_deref() == Some(region.id.as_str()) {
            if planet.extraction_route.paused_reason.as_deref() == Some("probe_loss")
                && region.within_scout_range
                && region
                    .deposit
                    .as_ref()
                    .is_some_and(|deposit| deposit.remaining > 0)
            {
                return Some(ContextAction::SetExtractionRoute(region.id.clone()));
            }
            return Some(ContextAction::ClearExtractionRoute);
        }
        planet
            .extraction_route
            .proven_region_ids
            .iter()
            .any(|proven| proven == &region.id)
            .then(|| ContextAction::SetExtractionRoute(region.id.clone()))
    }

    /// Shortest live completion countdown or ready-decision badge for the companion status strip.
    pub fn next_event_badge(&self, now: Instant) -> Option<(String, bool)> {
        let planet = self.planet.as_ref()?;
        if !planet.archive.unpresented_record_ids.is_empty() {
            return Some((
                match planet.archive.unpresented_record_ids.len() {
                    1 => "1 DISCOVERY READY".to_owned(),
                    count => format!("{count} DISCOVERIES READY"),
                },
                true,
            ));
        }
        let region_label = |region_id: &str| -> String {
            planet
                .regions
                .iter()
                .find(|r| r.id == region_id)
                .map_or_else(|| region_id.to_owned(), |r| r.designation.clone())
        };
        let mut timers: Vec<(u64, String)> = Vec::new();
        if let Some(mission) = &planet.active_mission {
            if mission.phase != ScoutState::Holding {
                let rem = self.display_remaining_ms(mission.remaining_ms, now);
                timers.push((
                    rem,
                    format!(
                        "NEXT: {} {}s",
                        region_label(&mission.target_region_id),
                        rem.div_ceil(1_000)
                    ),
                ));
            }
        }
        if let Some(ext) = &planet.active_extraction {
            let rem = self.display_remaining_ms(ext.remaining_ms, now);
            timers.push((
                rem,
                format!(
                    "NEXT: EXT {} {}s",
                    region_label(&ext.target_region_id),
                    rem.div_ceil(1_000)
                ),
            ));
        }
        if let Some(site) = &planet.network.active_site_survey {
            let rem = self.display_remaining_ms(site.remaining_ms, now);
            timers.push((rem, format!("NEXT: SCAN {}s", rem.div_ceil(1_000))));
        }
        if let Some(build) = &planet.active_construction {
            let rem = self.display_remaining_ms(build.remaining_ms, now);
            timers.push((rem, format!("NEXT: RELAY {}s", rem.div_ceil(1_000))));
        }
        if let Some(res) = &planet.research.active {
            let rem = self.display_remaining_ms(res.remaining_ms, now);
            let title = planet
                .research
                .options
                .iter()
                .find(|o| o.project == res.project)
                .map_or("RESEARCH", |o| o.title.as_str());
            timers.push((
                rem,
                format!(
                    "NEXT: RESEARCH {} {}s",
                    title.to_uppercase(),
                    rem.div_ceil(1_000)
                ),
            ));
        }
        for proj in &planet.network.active_projects {
            let rem = self.display_remaining_ms(proj.remaining_ms, now);
            timers.push((rem, format!("NEXT: PROJECT {}s", rem.div_ceil(1_000))));
        }
        if planet.extractor.rebuild_remaining_ms > 0 {
            let rem = self.display_remaining_ms(planet.extractor.rebuild_remaining_ms, now);
            timers.push((rem, format!("NEXT: REBUILD {}s", rem.div_ceil(1_000))));
        }
        if let Some(exp) = &planet.archive.experiment {
            if exp.active && exp.running {
                let rem = self.display_remaining_ms(exp.remaining_ms, now);
                timers.push((rem, format!("NEXT: ARCHIVE {}s", rem.div_ceil(1_000))));
            }
        }
        timers.sort_by_key(|(ms, _)| *ms);
        if let Some((_, first)) = timers.first() {
            let label = if timers.len() > 1 {
                format!("{first} (+{})", timers.len() - 1)
            } else {
                first.clone()
            };
            return Some((label, false));
        }
        Some(("READY FOR ORDERS".to_owned(), true))
    }

    /// Short OSC 0 terminal title reflecting current daemon state (used when `--title` is enabled).
    pub fn companion_window_title(&self, now: Instant) -> String {
        if self.onboarding_pending() {
            return "SUBSTRATE · Coherence".to_owned();
        }
        let Some(planet) = self.planet.as_ref() else {
            return "SUBSTRATE".to_owned();
        };
        let chapter = crate::transmission::roman_chapter(planet.campaign.chapter_number);
        let badge = self
            .next_event_badge(now)
            .map_or_else(|| "ONLINE".to_owned(), |(label, _)| label);
        format!("SUBSTRATE · CH {chapter} · {badge}")
    }

    /// Monotonic milestone/completion counters used by opt-in `--bell` notifications.
    /// Uses `max(mission_id)` from `recent_reports` rather than `.len()` so notifications keep
    /// working even after the 64-entry report ring saturates.
    pub fn bell_milestone_signature(&self) -> Option<(usize, usize, usize, u64)> {
        let planet = self.planet.as_ref()?;
        let surveyed = planet
            .regions
            .iter()
            .filter(|r| r.knowledge >= mind_core::protocol::RegionKnowledge::Surveyed)
            .count();
        let archived = planet.archive.records.len();
        let research = planet
            .research
            .options
            .iter()
            .filter(|o| o.completed)
            .count();
        let latest_report_id = planet
            .recent_reports
            .iter()
            .map(|r| r.mission_id)
            .max()
            .unwrap_or(0);
        Some((surveyed, archived, research, latest_report_id))
    }

    /// Returns the latest unreviewed activity ID to acknowledge while the player is actively
    /// watching a live non-modal workspace, so Return Summary only reports away-time events.
    pub fn pending_live_activity_ack(&mut self, now: Instant) -> Option<u64> {
        if !matches!(self.connection, ConnectionState::Connected)
            || self.session_menu.is_some()
            || self.show_return_summary
            || self.boot_intro_active(now)
            || self.active_transmission_record().is_some()
        {
            return None;
        }
        let latest_id = self
            .planet
            .as_ref()?
            .unreviewed_activity
            .last()
            .map(|record| record.activity_id)?;
        if latest_id > self.last_auto_acked_activity_id {
            self.last_auto_acked_activity_id = latest_id;
            Some(latest_id)
        } else {
            None
        }
    }

    pub fn scroll_transmission(&mut self, delta: i16) {
        self.transmission_scroll = if delta.is_negative() {
            self.transmission_scroll
                .saturating_sub(delta.unsigned_abs())
        } else {
            self.transmission_scroll
                .saturating_add(delta as u16)
                .min(64)
        };
    }

    /// Step to the next Return Summary page; false once the last page is showing, when Enter
    /// or Esc acknowledges every away event.
    pub fn advance_return_summary(&mut self) -> bool {
        let next = self.scroll_limits.summary_next.get();
        if next == 0 {
            return false;
        }
        self.return_summary_anchor = next;
        self.scroll_limits.summary_next.set(0);
        true
    }

    pub fn dismiss_return_summary(&mut self) -> Option<u64> {
        self.show_return_summary = false;
        self.return_summary_anchor = 0;
        self.scroll_limits.summary_next.set(0);
        if self.active_transmission_id.is_some() {
            self.transmission_opened_at = Some(Instant::now());
            self.transmission_revealed = false;
        }
        self.planet
            .as_ref()?
            .unreviewed_activity
            .last()
            .map(|record| record.activity_id)
    }

    pub fn active_transmission_record(&self) -> Option<(usize, &ArchiveRecordSnapshot)> {
        if self.show_return_summary {
            return None;
        }
        let record_id = self
            .replay_transmission_id
            .as_deref()
            .or(self.active_transmission_id.as_deref())?;
        let records = &self.planet.as_ref()?.archive.records;
        records
            .iter()
            .enumerate()
            .find(|(_, record)| record.record_id == record_id)
    }

    pub fn open_archive_replay(&mut self, now: Instant) -> bool {
        let Some(record_id) = self.planet.as_ref().and_then(|planet| {
            planet
                .archive
                .records
                .get(
                    self.archive_selection
                        .min(planet.archive.records.len().saturating_sub(1)),
                )
                .map(|r| r.record_id.clone())
        }) else {
            return false;
        };
        self.replay_transmission_id = Some(record_id);
        self.transmission_opened_at = Some(now);
        self.transmission_revealed = false;
        self.transmission_scroll = 0;
        true
    }

    pub fn reveal_or_dismiss_transmission(&mut self, now: Instant) -> Option<String> {
        let elapsed = self
            .transmission_opened_at
            .map_or(Duration::from_secs(10), |opened| {
                now.saturating_duration_since(opened)
            });
        if self.replay_transmission_id.is_some() {
            if !self.transmission_revealed && elapsed < Duration::from_millis(2_400) {
                self.transmission_revealed = true;
                return None;
            }
            self.replay_transmission_id = None;
            self.transmission_opened_at = None;
            self.transmission_revealed = false;
            self.transmission_scroll = 0;
            return None;
        }
        let record_id = self.active_transmission_id.clone()?;
        if !self.transmission_revealed && elapsed < Duration::from_millis(2_400) {
            self.transmission_revealed = true;
            return None;
        }
        self.active_transmission_id = None;
        self.transmission_opened_at = None;
        self.transmission_revealed = false;
        self.transmission_scroll = 0;
        self.pending_archive_ack_id = Some(record_id.clone());
        Some(record_id)
    }

    pub fn clear_active_transmission_locally(&mut self) {
        self.active_transmission_id = None;
        self.replay_transmission_id = None;
        self.transmission_opened_at = None;
        self.transmission_revealed = false;
        self.transmission_scroll = 0;
    }

    pub fn toggle_chapter_card(&mut self) {
        self.show_chapter_card = !self.show_chapter_card;
    }

    pub fn focus_campaign_lead(&mut self) -> bool {
        // Inside 4x4 local view, focus the actionable sector in this region first (e.g. a
        // connected node needing an Observatory/Lab, or a Detected frontier sector needing [S]).
        // When the campaign lead points at another region, or at a regional deposit (X acts
        // from the globe), leave the local field and focus the lead region.
        // With no published lead, do not invent one from local frontiers.
        let lead_is_here = self.planet.as_ref().is_some_and(|planet| {
            planet.campaign.next_action_key.as_deref() != Some("X")
                && planet
                    .campaign
                    .lead_region_id
                    .as_deref()
                    .is_some_and(|lead| Some(lead) == self.selected_region_id.as_deref())
        });
        if self.workspace == Workspace::Planet && self.local_view && lead_is_here {
            if let Some((slot, label)) = self.best_local_lead_slot() {
                self.selected_subregion_slot = slot;
                self.pending_action = None;
                self.region_panel_scroll = 0;
                self.notice = Some(format!("FOCUSED LOCAL LEAD // {label}"));
                return true;
            }
        }
        let Some((lead_id, designation)) = self.planet.as_ref().and_then(|planet| {
            let lead_id = planet.campaign.lead_region_id.as_ref()?;
            let designation = planet
                .regions
                .iter()
                .find(|region| &region.id == lead_id)
                .map_or(lead_id.as_str(), |region| region.designation.as_str())
                .to_owned();
            Some((lead_id.clone(), designation))
        }) else {
            self.notice = Some("NO SPECIFIC REGION LEAD // CHECK CHAPTER GUIDANCE".into());
            return false;
        };
        self.workspace = Workspace::Planet;
        self.selected_region_id = Some(lead_id);
        self.local_view = false;
        self.pending_action = None;
        self.region_panel_scroll = 0;
        self.focus_selected_region(false);
        self.notice = Some(format!("FOCUSED CAMPAIGN LEAD // {designation}"));
        true
    }

    pub fn scroll_help(&mut self, delta: isize) {
        let amount = i16::try_from(delta).unwrap_or(if delta < 0 { i16::MIN } else { i16::MAX });
        self.help_scroll = clamped_scroll(self.help_scroll, amount, self.scroll_limits.help.get());
    }

    pub fn rig_morph_progress(&self, now: Instant) -> Option<f32> {
        let started = self.rig_morph_at?;
        let elapsed_ms = now.saturating_duration_since(started).as_millis() as f32;
        if elapsed_ms < 360.0 {
            Some((elapsed_ms / 360.0).clamp(0.0, 1.0))
        } else {
            None
        }
    }

    fn best_local_lead_slot(&self) -> Option<(usize, String)> {
        let planet = self.planet.as_ref()?;
        let region_id = self.selected_region_id.as_deref()?;
        // IMPORTANT: selected_subregion_slot indexes the row/column-sorted `selected_subregions()` list,
        // NOT the persisted `site.slot` ordering.
        let sites = self.selected_subregions();
        if sites.is_empty() {
            return None;
        }
        // A daemon-published lead sector (for example an offline node) wins outright.
        if let Some(target) = planet.campaign.lead_subregion_id.as_deref() {
            if let Some((idx, site)) = sites.iter().enumerate().find(|(_, s)| s.id == target) {
                return Some((idx, site.designation.clone()));
            }
        }
        let lead_kind = planet.campaign.lead_kind;
        // Prefer an established node in this region when campaign asks for node/module work.
        if lead_kind == Some(mind_core::protocol::CampaignLeadKind::Node)
            || matches!(
                planet.campaign.next_action_key.as_deref(),
                Some("O" | "L" | "P" | "C" | "D" | "W" | "M")
            )
        {
            if let Some(base) = planet
                .network
                .bases
                .iter()
                .find(|b| b.region_id == region_id)
            {
                if let Some((idx, site)) = sites
                    .iter()
                    .enumerate()
                    .find(|(_, s)| s.id == base.subregion_id)
                {
                    return Some((idx, site.designation.clone()));
                }
            }
        }
        // Otherwise prefer a Detected frontier sector (sheltered shelf first for Ch I).
        if let Some((idx, site)) = sites
            .iter()
            .enumerate()
            .filter(|(_, s)| s.knowledge == SubregionKnowledge::Detected)
            .max_by_key(|(_, s)| {
                u8::from(s.class == Some(mind_core::protocol::SubregionClass::ShelteredShelf))
            })
        {
            return Some((idx, site.designation.clone()));
        }
        None
    }

    pub fn move_archive_selection(&mut self, delta: isize) {
        let count = self
            .planet
            .as_ref()
            .map_or(0, |planet| planet.archive.records.len());
        if count == 0 {
            self.archive_selection = 0;
            return;
        }
        self.archive_selection = self
            .archive_selection
            .saturating_add_signed(delta)
            .min(count - 1);
        self.archive_detail_scroll = 0;
    }

    pub fn scroll_archive_detail(&mut self, amount: i16) {
        self.archive_detail_scroll = clamped_scroll(
            self.archive_detail_scroll,
            amount,
            self.scroll_limits.archive_detail.get(),
        );
    }

    pub fn archive_experiment_action(&self) -> Option<ContextAction> {
        let experiment = self.planet.as_ref()?.archive.experiment.as_ref()?;
        (self.workspace == Workspace::Archive && experiment.available && !experiment.active)
            .then_some(ContextAction::StartArchiveExperiment)
    }

    fn research_entry_count(&self) -> usize {
        self.planet.as_ref().map_or(0, |planet| {
            planet.research.options.len() + planet.research.profiles.len()
        })
    }

    fn clamp_archive_selection(&mut self) {
        let count = self
            .planet
            .as_ref()
            .map_or(0, |planet| planet.archive.records.len());
        self.archive_selection = self.archive_selection.min(count.saturating_sub(1));
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
                .network
                .active_projects
                .iter()
                .map(|project| project.region_id.as_str()),
        );
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
            self.suppress_initial_return_summary = true;
        } else {
            self.notice = Some(format!(
                "ONBOARDING NOT COMPLETED: {}",
                result.code.as_deref().unwrap_or("unknown_error")
            ));
            self.suppress_initial_return_summary = false;
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

    /// Events the System pane lists; routine acknowledgements are bookkeeping, not events.
    pub fn shown_events(&self) -> impl DoubleEndedIterator<Item = &EngineEvent> {
        self.events.iter().filter(|event| is_shown_event(event))
    }

    /// Furthest scroll that still fills the event pane, as recorded by the renderer.
    fn max_scroll(&self) -> usize {
        let shown = self.shown_events().count();
        usize::from(self.scroll_limits.events.get()).min(shown.saturating_sub(1))
    }

    /// One page of System events: the whole events on screen minus one, at least one.
    #[must_use]
    pub fn event_page(&self) -> usize {
        usize::from(self.scroll_limits.event_rows.get().saturating_sub(1).max(1))
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
        let Some(planet) = self.planet.as_ref() else {
            return;
        };
        let latitude_bands = planet.latitude_bands;
        let longitude_sectors = planet.longitude_sectors;
        let Some(region) = self.selected_region() else {
            return;
        };
        let longitude_sector = region.position.longitude_sector;
        let latitude_band = region.position.latitude_band;
        self.globe_target_yaw =
            f64::from(longitude_sector) * std::f64::consts::TAU / f64::from(longitude_sectors);
        let latitude = std::f64::consts::FRAC_PI_2
            - (f64::from(latitude_band) + 0.5) * std::f64::consts::PI / f64::from(latitude_bands);
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

fn stock_contains(stock: ResourceStock, required: ResourceStock) -> bool {
    stock.structural >= required.structural
        && stock.conductive >= required.conductive
        && stock.silicate >= required.silicate
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
        engine.acknowledge_archive_presentation("observation_001");
        let start = Instant::now();
        let mut app = App::default();
        app.apply_snapshot(engine.status(), start);
        app.apply_planet(engine.planet().unwrap(), start);
        app.connected();
        app.show_return_summary = false;
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
        assert_eq!(app.workspace, Workspace::Archive);

        // The map waits for its introducing transmission, then opens.
        let mut completed = engine.planet().unwrap();
        completed.system_map.available = true;
        completed.archive.unpresented_record_ids = vec!["observation_005".into()];
        app.apply_planet(completed.clone(), Instant::now());
        assert_eq!(app.workspace, Workspace::Archive);
        completed.archive.unpresented_record_ids.clear();
        app.apply_planet(completed, Instant::now());
        assert_eq!(app.workspace, Workspace::SystemMap);
        app.cycle_workspace(false);
        assert_eq!(app.workspace, Workspace::System);
        app.cycle_workspace(true);
        assert_eq!(app.workspace, Workspace::SystemMap);
    }

    #[test]
    fn stale_planet_notifications_cannot_regress_authoritative_presentation() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([2, 2, 2]);
        let current = engine.planet().unwrap();
        let current_revision = current.revision;
        let current_objective = current.objective.title.clone();
        let mut app = App::default();
        app.apply_planet(current, Instant::now());

        let mut stale = engine.planet().unwrap();
        stale.revision = current_revision.saturating_sub(1);
        stale.objective.title = "STALE DATA".into();
        app.apply_planet(stale, Instant::now());

        assert_eq!(app.planet.as_ref().unwrap().revision, current_revision);
        assert_eq!(
            app.planet.as_ref().unwrap().objective.title,
            current_objective
        );
    }

    #[test]
    fn authoritative_return_to_onboarding_clears_previous_playthrough_views() {
        let established = mind_core::engine::MindEngine::new(20);
        established.complete_onboarding([2, 2, 2]);
        established.trigger_diagnostic_pulse();
        let fresh = mind_core::engine::MindEngine::new(20);
        let now = Instant::now();
        let mut app = App::default();
        app.apply_snapshot(established.status(), now);
        app.apply_planet(established.planet().unwrap(), now);
        app.workspace = Workspace::System;
        app.pending_action = Some(ContextAction::Recall);

        app.apply_snapshot(fresh.status(), now + Duration::from_millis(50));

        assert!(app.onboarding_pending());
        assert!(app.planet.is_none());
        assert!(app.events.iter().all(|event| {
            event.kind != mind_core::protocol::EngineEventKind::OnboardingCompleted
        }));
        assert_eq!(app.workspace, Workspace::Planet);
        assert!(app.pending_action.is_none());
        assert_eq!(
            app.notice.as_deref(),
            Some("NEW PLAYTHROUGH READY // COHERENCE REQUIRED")
        );
    }

    #[test]
    fn failed_new_playthrough_keeps_menu_open_and_explains_rollback() {
        let mut app = App::default();
        app.begin_new_playthrough();

        app.new_playthrough_completed(NewPlaythroughResult {
            ok: false,
            code: Some("persistence_failed".into()),
        });

        assert_eq!(app.session_menu, Some(SessionMenuState::Selecting));
        assert!(app
            .session_menu_message
            .as_deref()
            .is_some_and(|message| message.contains("unchanged")));
    }

    #[test]
    fn local_field_gate_explains_regional_survey_requirement_and_opens_after_survey() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([2, 2, 2]);
        let now = Instant::now();
        let mut app = App::default();
        app.apply_snapshot(engine.status(), now);
        app.apply_planet(engine.planet().unwrap(), now);

        // Core has no local field until a non-core region is surveyed, or selecting an unknown region explains the requirement.
        let unknown = engine
            .planet()
            .unwrap()
            .regions
            .into_iter()
            .find(|region| !region.core)
            .unwrap();
        app.selected_region_id = Some(unknown.id);
        app.enter_local_view();
        assert!(!app.local_view);
        assert_eq!(
            app.notice.as_deref(),
            Some("LOCAL FIELD UNAVAILABLE // Complete a regional survey here first.")
        );
    }

    #[test]
    fn live_onboarding_completion_opens_transmission_without_return_summary() {
        let engine = mind_core::engine::MindEngine::new(20);
        let now = Instant::now();
        let mut app = App::default();
        app.apply_snapshot(engine.status(), now);
        app.begin_submission();
        let result = engine.complete_onboarding([2, 2, 2]);
        app.onboarding_completed(result);
        app.apply_snapshot(engine.status(), now);
        app.apply_planet(engine.planet().unwrap(), now);

        assert!(!app.show_return_summary);
        assert_eq!(
            app.active_transmission_id.as_deref(),
            Some("observation_001")
        );
    }

    #[test]
    fn dismissed_transmission_ignores_stale_unpresented_push_until_confirmed() {
        let engine = mind_core::engine::MindEngine::new(20);
        let now = Instant::now();
        engine.complete_onboarding([2, 2, 2]);
        let snapshot = engine.planet().unwrap();
        let mut app = App::default();
        app.apply_snapshot(engine.status(), now);
        app.apply_planet(snapshot.clone(), now);
        app.show_return_summary = false;
        app.transmission_revealed = true;

        let acked = app.reveal_or_dismiss_transmission(now);
        assert_eq!(acked.as_deref(), Some("observation_001"));
        assert!(app.active_transmission_id.is_none());

        // A concurrent snapshot push that still lists observation_001 does not flicker-reopen it.
        app.apply_planet(snapshot, now);
        assert!(app.active_transmission_id.is_none());

        // Once the daemon confirms acknowledgement, pending suppression clears cleanly.
        engine.acknowledge_archive_presentation("observation_001");
        app.apply_planet(engine.planet().unwrap(), now);
        assert!(app.active_transmission_id.is_none());
    }

    #[test]
    fn adaptive_frame_scheduler_sleeps_when_static_and_uses_30fps_when_animating() {
        let engine = mind_core::engine::MindEngine::new(20);
        let now = Instant::now();
        let mut app = App::default();
        app.apply_snapshot(engine.status(), now);
        app.connected();

        // Session menu is completely static -> zero animation redraws.
        app.open_session_menu();
        assert!(app.next_frame_deadline(now).is_none());
        app.close_session_menu();

        // Pending Coherence onboarding runs live oscilloscope at ~30 FPS.
        let onboarding_deadline = app.next_frame_deadline(now).unwrap();
        assert!(onboarding_deadline.saturating_duration_since(now) <= Duration::from_millis(40));

        // Complete onboarding, acknowledge transmission, switch to static System / Overview.
        engine.complete_onboarding([2, 2, 2]);
        engine.acknowledge_archive_presentation("observation_001");
        app.apply_snapshot(engine.status(), now);
        app.apply_planet(engine.planet().unwrap(), now);
        app.show_return_summary = false;

        app.workspace = Workspace::System;
        assert!(app.next_frame_deadline(now).is_none());
        app.workspace = Workspace::Overview;
        assert!(app.next_frame_deadline(now).is_none());

        // On Planet while easing camera after navigation -> 30 FPS.
        app.workspace = Workspace::Planet;
        app.move_selection(0, 1);
        let easing_deadline = app.next_frame_deadline(now).unwrap();
        assert!(easing_deadline.saturating_duration_since(now) <= Duration::from_millis(40));
    }
}
