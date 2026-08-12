use crate::protocol::{
    ActiveConstructionSnapshot, ActiveExtractionSnapshot, ActiveMissionSnapshot,
    ActiveResearchSnapshot, ActivityKind, ActivityRecord, AtmosphericSeverity, DepositSnapshot,
    ExtractionProbeSnapshot, ExtractionRouteSnapshot, ExtractorState, MissionReport,
    MissionReportOutcome, ObjectiveSnapshot, OnboardingCompletionResult, OnboardingSnapshot,
    OperationalProfile, OperationalProfileSnapshot, PlanetActionResult, PlanetSnapshot,
    RegionKnowledge, RegionPosition, RegionSnapshot, ResearchOptionSnapshot, ResearchProject,
    ResearchSnapshot, ResourceKind, ResourceStock, RiskLevel, ScoutSnapshot, ScoutState,
    SurveyConfidence, SurveyQueueSnapshot, TemperatureBand, TerrainClass,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fs, io, io::Write, path::Path};
use tempfile::NamedTempFile;
use thiserror::Error;
use uuid::Uuid;

const SAVE_SCHEMA_VERSION: u32 = 5;
const RESETTABLE_SCHEMA_VERSIONS: [u32; 4] = [1, 2, 3, 4];
const WORLD_GENERATION_VERSION: u32 = 2;
const LATITUDE_BANDS: u8 = 10;
const LONGITUDE_SECTORS: u8 = 20;
const REGION_COUNT: usize = (LATITUDE_BANDS as usize) * (LONGITUDE_SECTORS as usize);
const CORE_LATITUDE_BAND: u8 = 4;
const CORE_LONGITUDE_SECTOR: u8 = 0;
const SCOUT_RANGE_HOPS: u8 = 2;
const TRAVEL_MS_PER_HOP: u64 = 15_000;
const SURVEY_MS: u64 = 30_000;
const REPORT_CAPACITY: usize = 64;
const EXTRACTION_MS: u64 = 30_000;
const EXTRACTOR_REBUILD_MS: u64 = 60_000;
const EXTRACTION_CAPACITY: u32 = 4;
const REFINING_MS_PER_UNIT: u64 = 5_000;
const RELAY_CONSTRUCTION_MS: u64 = 90_000;
const RELAY_RANGE_HOPS: u8 = 2;
const RELAY_COST: ResourceStock = ResourceStock {
    structural: 8,
    conductive: 6,
    silicate: 6,
};
const RESEARCH_ANALYSIS_COST: u32 = 3;
const RESEARCH_DURATION_MS: u64 = 120_000;
const SURVEY_QUEUE_CAPACITY: usize = 8;
const ACTIVITY_CAPACITY: usize = 64;

pub const ONBOARDING_BAND_SLOTS: u8 = 12;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GameState {
    onboarding_complete: bool,
    state_revision: u64,
    world: WorldState,
    scout: ScoutStateData,
    active_mission: Option<MissionState>,
    recent_reports: Vec<MissionReport>,
    next_mission_id: u64,
    #[serde(default)]
    active_extraction: Option<ExtractionMissionState>,
    #[serde(default = "extractor_available_default")]
    extractor_available: bool,
    #[serde(default)]
    extractor_rebuild_remaining_ms: u64,
    #[serde(default)]
    raw_inventory: ResourceStock,
    #[serde(default)]
    refined_inventory: ResourceStock,
    #[serde(default)]
    refining_remaining_ms: u64,
    #[serde(default)]
    active_construction: Option<ConstructionState>,
    #[serde(default)]
    relay_region_id: Option<String>,
    reach_milestone_complete: bool,
    #[serde(default)]
    alpha_complete: bool,
    analysis_units: u32,
    completed_research: Vec<ResearchProject>,
    active_research: Option<ResearchState>,
    operational_profile: OperationalProfile,
    survey_queue: Vec<String>,
    proven_extraction_region_ids: Vec<String>,
    extraction_route_region_id: Option<String>,
    extraction_route_paused_reason: Option<String>,
    automated_survey_completed: bool,
    automated_extraction_completed: bool,
    activity_log: Vec<ActivityRecord>,
    next_activity_id: u64,
    acknowledged_activity_id: u64,
    #[serde(default)]
    random_state: u64,
    #[serde(skip)]
    dirty: bool,
    #[serde(skip)]
    generation: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct WorldState {
    seed: u64,
    generation_version: u32,
    core_region_id: String,
    regions: Vec<RegionState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct RegionState {
    id: String,
    designation: String,
    position: RegionPosition,
    terrain: TerrainClass,
    temperature: TemperatureBand,
    atmospheric_severity: AtmosphericSeverity,
    knowledge: RegionKnowledge,
    #[serde(default)]
    deposit: Option<DepositState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct DepositState {
    resource: ResourceKind,
    remaining: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ScoutStateData {
    id: String,
    designation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct MissionState {
    mission_id: u64,
    target_region_id: String,
    phase: ScoutState,
    remaining_ms: u64,
    phase_total_ms: u64,
    travel_ms: u64,
    survey_required: bool,
    survey_completed: bool,
    auto_return: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ExtractionMissionState {
    mission_id: u64,
    target_region_id: String,
    phase: ExtractorState,
    remaining_ms: u64,
    phase_total_ms: u64,
    travel_ms: u64,
    risk: RiskLevel,
    will_fail: bool,
    cargo: Option<(ResourceKind, u32)>,
    automated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ConstructionState {
    region_id: String,
    remaining_ms: u64,
    phase_total_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ResearchState {
    project: ResearchProject,
    remaining_ms: u64,
    phase_total_ms: u64,
}

fn extractor_available_default() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveEnvelope {
    schema_version: u32,
    saved_at: DateTime<Utc>,
    state: GameState,
}

#[derive(Debug, Deserialize)]
struct VersionEnvelope {
    schema_version: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadDisposition {
    Missing,
    Current,
    PreviousReset,
}

#[derive(Debug)]
pub struct LoadedGame {
    pub state: GameState,
    pub disposition: LoadDisposition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameAdvanceEvent {
    RegionDetected,
    SurveyCompleted,
    ScoutReturned,
    ExtractionCompleted,
    ExtractorLost,
    ExtractorRebuilt,
    RelayActivated,
    AlphaMilestoneCompleted,
    AlphaSliceCompleted,
    ResearchCompleted,
    AutomationPaused,
}

impl Default for GameState {
    fn default() -> Self {
        let bytes = Uuid::new_v4().into_bytes();
        let seed = u64::from_le_bytes(bytes[..8].try_into().expect("UUID contains eight bytes"));
        Self::with_seed(seed)
    }
}

impl GameState {
    pub fn with_seed(seed: u64) -> Self {
        let world = WorldState::generate(seed);
        Self {
            onboarding_complete: false,
            state_revision: 0,
            world,
            scout: ScoutStateData {
                id: "scout_01".into(),
                designation: "Scout 01".into(),
            },
            active_mission: None,
            recent_reports: Vec::new(),
            next_mission_id: 1,
            active_extraction: None,
            extractor_available: true,
            extractor_rebuild_remaining_ms: 0,
            raw_inventory: ResourceStock::default(),
            refined_inventory: ResourceStock::default(),
            refining_remaining_ms: 0,
            active_construction: None,
            relay_region_id: None,
            reach_milestone_complete: false,
            alpha_complete: false,
            analysis_units: 0,
            completed_research: Vec::new(),
            active_research: None,
            operational_profile: OperationalProfile::Balanced,
            survey_queue: Vec::new(),
            proven_extraction_region_ids: Vec::new(),
            extraction_route_region_id: None,
            extraction_route_paused_reason: None,
            automated_survey_completed: false,
            automated_extraction_completed: false,
            activity_log: Vec::new(),
            next_activity_id: 1,
            acknowledged_activity_id: 0,
            random_state: seed ^ 0xA17A_5EED_D15C_0FFE,
            dirty: true,
            generation: 0,
        }
    }

    pub fn load(path: &Path) -> Result<LoadedGame, GameError> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(LoadedGame {
                    state: Self::default(),
                    disposition: LoadDisposition::Missing,
                });
            }
            Err(error) => return Err(GameError::Io(error)),
        };
        let value: Value = serde_json::from_slice(&bytes)?;
        let version: VersionEnvelope = serde_json::from_value(value.clone())?;
        if RESETTABLE_SCHEMA_VERSIONS.contains(&version.schema_version) {
            let mut state = Self::default();
            state.save(path)?;
            state.mark_saved(0);
            return Ok(LoadedGame {
                state,
                disposition: LoadDisposition::PreviousReset,
            });
        }
        if version.schema_version != SAVE_SCHEMA_VERSION {
            return Err(GameError::UnsupportedVersion(version.schema_version));
        }
        let envelope: SaveEnvelope = serde_json::from_value(value)?;
        let mut state = envelope.state;
        state.validate()?;
        state.dirty = false;
        state.generation = 0;
        Ok(LoadedGame {
            state,
            disposition: LoadDisposition::Current,
        })
    }

    pub fn save(&self, path: &Path) -> Result<(), GameError> {
        let parent = path.parent().ok_or(GameError::MissingParent)?;
        let mut temporary = NamedTempFile::new_in(parent)?;
        serde_json::to_writer_pretty(
            &mut temporary,
            &SaveEnvelope {
                schema_version: SAVE_SCHEMA_VERSION,
                saved_at: Utc::now(),
                state: self.clone(),
            },
        )?;
        temporary.write_all(b"\n")?;
        temporary.as_file_mut().sync_all()?;
        temporary
            .persist(path)
            .map_err(|error| GameError::Io(error.error))?;
        Ok(())
    }

    pub fn onboarding(&self) -> OnboardingSnapshot {
        OnboardingSnapshot {
            complete: self.onboarding_complete,
        }
    }

    pub fn complete_onboarding(&mut self, band_positions: [u8; 3]) -> OnboardingCompletionResult {
        if band_positions
            .iter()
            .any(|position| *position >= ONBOARDING_BAND_SLOTS)
        {
            return self.onboarding_rejected("invalid_alignment");
        }
        if band_positions[1..]
            .iter()
            .any(|position| *position != band_positions[0])
        {
            return self.onboarding_rejected("not_coherent");
        }
        if !self.onboarding_complete {
            self.onboarding_complete = true;
            self.changed();
        }
        OnboardingCompletionResult {
            ok: true,
            code: None,
            state_revision: self.state_revision,
            completed: true,
        }
    }

    pub fn planet_snapshot(&self) -> Option<PlanetSnapshot> {
        self.onboarding_complete.then(|| {
            let phase = self
                .active_mission
                .as_ref()
                .map_or(ScoutState::Ready, |mission| mission.phase);
            let region_id = match self.active_mission.as_ref() {
                Some(mission)
                    if matches!(
                        mission.phase,
                        ScoutState::Surveying | ScoutState::Holding | ScoutState::Returning
                    ) =>
                {
                    mission.target_region_id.clone()
                }
                _ => self.world.core_region_id.clone(),
            };
            PlanetSnapshot {
                revision: self.state_revision,
                latitude_bands: LATITUDE_BANDS,
                longitude_sectors: LONGITUDE_SECTORS,
                core_region_id: self.world.core_region_id.clone(),
                regions: self
                    .world
                    .regions
                    .iter()
                    .map(|region| self.region_snapshot(region))
                    .collect(),
                scout: ScoutSnapshot {
                    id: self.scout.id.clone(),
                    designation: self.scout.designation.clone(),
                    state: phase,
                    region_id,
                },
                active_mission: self
                    .active_mission
                    .as_ref()
                    .map(|mission| ActiveMissionSnapshot {
                        mission_id: mission.mission_id,
                        target_region_id: mission.target_region_id.clone(),
                        phase: mission.phase,
                        remaining_ms: mission.remaining_ms,
                        phase_total_ms: mission.phase_total_ms,
                        automated: mission.auto_return,
                    }),
                recent_reports: self.recent_reports.clone(),
                extractor: ExtractionProbeSnapshot {
                    designation: "Extractor 01".into(),
                    state: self.extractor_state(),
                    rebuild_remaining_ms: self.extractor_rebuild_remaining_ms,
                },
                active_extraction: self.active_extraction.as_ref().map(|mission| {
                    ActiveExtractionSnapshot {
                        mission_id: mission.mission_id,
                        target_region_id: mission.target_region_id.clone(),
                        phase: mission.phase,
                        remaining_ms: mission.remaining_ms,
                        phase_total_ms: mission.phase_total_ms,
                        risk: mission.risk,
                        cargo_capacity: self.extraction_capacity(),
                        automated: mission.automated,
                    }
                }),
                raw_inventory: self.raw_inventory,
                refined_inventory: self.refined_inventory,
                refining_remaining_ms: self.refining_remaining_ms,
                active_construction: self.active_construction.as_ref().map(|construction| {
                    ActiveConstructionSnapshot {
                        region_id: construction.region_id.clone(),
                        remaining_ms: construction.remaining_ms,
                        phase_total_ms: construction.phase_total_ms,
                    }
                }),
                relay_region_id: self.relay_region_id.clone(),
                research: self.research_snapshot(),
                survey_queue: SurveyQueueSnapshot {
                    unlocked: self.automation_unlocked(),
                    capacity: SURVEY_QUEUE_CAPACITY,
                    queued_region_ids: self.survey_queue.clone(),
                },
                extraction_route: ExtractionRouteSnapshot {
                    unlocked: self.automation_unlocked(),
                    region_id: self.extraction_route_region_id.clone(),
                    paused_reason: self.extraction_route_paused_reason.clone(),
                    proven_region_ids: self.proven_extraction_region_ids.clone(),
                },
                unreviewed_activity: self
                    .activity_log
                    .iter()
                    .filter(|record| record.activity_id > self.acknowledged_activity_id)
                    .cloned()
                    .collect(),
                objective: self.objective_snapshot(),
            }
        })
    }

    pub fn deploy_scout(&mut self, region_id: &str) -> PlanetActionResult {
        if !self.onboarding_complete {
            return self.action_rejected("onboarding_required");
        }
        let Some(region) = self
            .world
            .regions
            .iter()
            .find(|region| region.id == region_id)
        else {
            return self.action_rejected("unknown_region");
        };
        if region.id == self.world.core_region_id {
            return self.action_rejected("region_out_of_range");
        }
        let distance = hop_distance(&region.position);
        if !self.region_within_range(&region.position) {
            return self.action_rejected("region_out_of_range");
        }
        if self.active_mission.is_some() {
            return self.action_rejected("scout_unavailable");
        }
        let travel_ms = u64::from(distance) * TRAVEL_MS_PER_HOP;
        let mission_id = self.next_mission_id;
        self.next_mission_id += 1;
        self.active_mission = Some(MissionState {
            mission_id,
            target_region_id: region_id.to_owned(),
            phase: ScoutState::Outbound,
            remaining_ms: travel_ms,
            phase_total_ms: travel_ms,
            travel_ms,
            survey_required: region.knowledge < RegionKnowledge::Surveyed,
            survey_completed: region.knowledge >= RegionKnowledge::Surveyed,
            auto_return: false,
        });
        self.changed();
        PlanetActionResult {
            ok: true,
            code: None,
            revision: self.state_revision,
            mission_id: Some(mission_id),
        }
    }

    pub fn recall_scout(&mut self) -> PlanetActionResult {
        if !self.onboarding_complete {
            return self.action_rejected("onboarding_required");
        }
        let Some(mut mission) = self.active_mission.take() else {
            return self.action_rejected("no_active_deployment");
        };
        if mission.phase == ScoutState::Returning {
            self.active_mission = Some(mission);
            return self.action_rejected("already_returning");
        }
        if mission.phase == ScoutState::Outbound {
            let elapsed = mission.phase_total_ms.saturating_sub(mission.remaining_ms);
            mission.phase_total_ms = elapsed.max(1);
            mission.remaining_ms = elapsed.max(1);
        } else {
            if let Some(region) = self
                .world
                .regions
                .iter_mut()
                .find(|region| region.id == mission.target_region_id)
            {
                if region.knowledge == RegionKnowledge::Observed {
                    region.knowledge = RegionKnowledge::Surveyed;
                }
            }
            mission.phase_total_ms = mission.travel_ms;
            mission.remaining_ms = mission.travel_ms;
        }
        mission.phase = ScoutState::Returning;
        let mission_id = mission.mission_id;
        self.active_mission = Some(mission);
        self.changed();
        PlanetActionResult {
            ok: true,
            code: None,
            revision: self.state_revision,
            mission_id: Some(mission_id),
        }
    }

    pub fn deploy_extractor(&mut self, region_id: &str) -> PlanetActionResult {
        if !self.onboarding_complete {
            return self.action_rejected("onboarding_required");
        }
        let Some(region) = self
            .world
            .regions
            .iter()
            .find(|region| region.id == region_id)
        else {
            return self.action_rejected("unknown_region");
        };
        if region.knowledge < RegionKnowledge::Surveyed {
            return self.action_rejected("survey_required");
        }
        if !self.region_within_range(&region.position) {
            return self.action_rejected("region_out_of_range");
        }
        let Some(deposit) = region.deposit.as_ref() else {
            return self.action_rejected("no_deposit");
        };
        if deposit.remaining == 0 {
            return self.action_rejected("deposit_depleted");
        }
        if !self.extractor_available || self.active_extraction.is_some() {
            return self.action_rejected("extractor_unavailable");
        }
        let travel_ms = u64::from(hop_distance(&region.position)) * TRAVEL_MS_PER_HOP;
        let risk = self.effective_risk(region.atmospheric_severity);
        let will_fail = self.next_random_percent() < risk_percent(risk);
        let mission_id = self.next_mission_id;
        self.next_mission_id += 1;
        self.active_extraction = Some(ExtractionMissionState {
            mission_id,
            target_region_id: region_id.to_owned(),
            phase: ExtractorState::Outbound,
            remaining_ms: travel_ms,
            phase_total_ms: travel_ms,
            travel_ms,
            risk,
            will_fail,
            cargo: None,
            automated: false,
        });
        self.changed();
        self.action_accepted(Some(mission_id))
    }

    pub fn construct_relay(&mut self, region_id: &str) -> PlanetActionResult {
        if !self.onboarding_complete {
            return self.action_rejected("onboarding_required");
        }
        let Some(region) = self
            .world
            .regions
            .iter()
            .find(|region| region.id == region_id)
        else {
            return self.action_rejected("unknown_region");
        };
        if region.id == self.world.core_region_id {
            return self.action_rejected("invalid_relay_site");
        }
        if region.knowledge < RegionKnowledge::Surveyed {
            return self.action_rejected("survey_required");
        }
        if !self.region_within_range(&region.position) {
            return self.action_rejected("region_out_of_range");
        }
        if self.relay_region_id.is_some() || self.active_construction.is_some() {
            return self.action_rejected("relay_already_committed");
        }
        if !stock_contains(self.refined_inventory, RELAY_COST) {
            return self.action_rejected("insufficient_materials");
        }
        subtract_stock(&mut self.refined_inventory, RELAY_COST);
        self.active_construction = Some(ConstructionState {
            region_id: region_id.to_owned(),
            remaining_ms: RELAY_CONSTRUCTION_MS,
            phase_total_ms: RELAY_CONSTRUCTION_MS,
        });
        self.changed();
        self.action_accepted(None)
    }

    pub fn start_research(&mut self, project: ResearchProject) -> PlanetActionResult {
        if !self.onboarding_complete {
            return self.action_rejected("onboarding_required");
        }
        if !self.reach_milestone_complete {
            return self.action_rejected("research_locked");
        }
        if self.active_research.is_some() {
            return self.action_rejected("research_in_progress");
        }
        if self.completed_research.contains(&project) {
            return self.action_rejected("research_completed");
        }
        if self.analysis_units < RESEARCH_ANALYSIS_COST {
            return self.action_rejected("insufficient_analysis");
        }
        self.analysis_units -= RESEARCH_ANALYSIS_COST;
        self.active_research = Some(ResearchState {
            project,
            remaining_ms: RESEARCH_DURATION_MS,
            phase_total_ms: RESEARCH_DURATION_MS,
        });
        self.push_activity(
            ActivityKind::Research,
            None,
            format!("Research started: {}.", research_title(project)),
        );
        self.changed();
        self.action_accepted(None)
    }

    pub fn activate_profile(&mut self, profile: OperationalProfile) -> PlanetActionResult {
        if !self.profile_unlocked(profile) {
            return self.action_rejected("profile_locked");
        }
        if self.active_mission.is_some() || self.active_extraction.is_some() {
            return self.action_rejected("field_operation_active");
        }
        if self.operational_profile != profile {
            self.operational_profile = profile;
            self.push_activity(
                ActivityKind::Research,
                None,
                format!("Operational profile activated: {}.", profile_title(profile)),
            );
            self.changed();
        }
        self.action_accepted(None)
    }

    pub fn queue_survey(&mut self, region_id: &str) -> PlanetActionResult {
        if !self.automation_unlocked() {
            return self.action_rejected("automation_locked");
        }
        let Some(region) = self
            .world
            .regions
            .iter()
            .find(|region| region.id == region_id)
        else {
            return self.action_rejected("unknown_region");
        };
        if region.id == self.world.core_region_id
            || region.knowledge >= RegionKnowledge::Surveyed
            || !self.region_within_range(&region.position)
        {
            return self.action_rejected("survey_queue_ineligible");
        }
        if self.survey_queue.iter().any(|queued| queued == region_id)
            || self
                .active_mission
                .as_ref()
                .is_some_and(|mission| mission.target_region_id == region_id)
        {
            return self.action_rejected("survey_already_queued");
        }
        if self.survey_queue.len() >= SURVEY_QUEUE_CAPACITY {
            return self.action_rejected("survey_queue_full");
        }
        self.survey_queue.push(region_id.to_owned());
        self.push_activity(
            ActivityKind::Automation,
            Some(region_id.to_owned()),
            format!("Automated survey queued for {}.", region.designation),
        );
        self.changed();
        self.action_accepted(None)
    }

    pub fn remove_queued_survey(&mut self, region_id: &str) -> PlanetActionResult {
        let Some(index) = self
            .survey_queue
            .iter()
            .position(|queued| queued == region_id)
        else {
            return self.action_rejected("survey_not_queued");
        };
        self.survey_queue.remove(index);
        let designation = self.region_designation(region_id);
        self.push_activity(
            ActivityKind::Automation,
            Some(region_id.to_owned()),
            format!("Queued survey removed from {designation}."),
        );
        self.changed();
        self.action_accepted(None)
    }

    pub fn set_extraction_route(&mut self, region_id: &str) -> PlanetActionResult {
        if !self.automation_unlocked() {
            return self.action_rejected("automation_locked");
        }
        if !self
            .proven_extraction_region_ids
            .iter()
            .any(|proven| proven == region_id)
        {
            return self.action_rejected("route_not_proven");
        }
        let Some(region) = self
            .world
            .regions
            .iter()
            .find(|region| region.id == region_id)
        else {
            return self.action_rejected("unknown_region");
        };
        if !self.region_within_range(&region.position)
            || region
                .deposit
                .as_ref()
                .is_none_or(|deposit| deposit.remaining == 0)
        {
            return self.action_rejected("route_unavailable");
        }
        self.extraction_route_region_id = Some(region_id.to_owned());
        self.extraction_route_paused_reason = None;
        self.push_activity(
            ActivityKind::Automation,
            Some(region_id.to_owned()),
            format!(
                "Proven extraction route activated at {}.",
                region.designation
            ),
        );
        self.changed();
        self.action_accepted(None)
    }

    pub fn clear_extraction_route(&mut self) -> PlanetActionResult {
        let Some(region_id) = self.extraction_route_region_id.take() else {
            return self.action_rejected("no_extraction_route");
        };
        self.extraction_route_paused_reason = None;
        let designation = self.region_designation(&region_id);
        self.push_activity(
            ActivityKind::Automation,
            Some(region_id),
            format!("Automated extraction route cleared from {designation}."),
        );
        self.changed();
        self.action_accepted(None)
    }

    pub fn acknowledge_activity(&mut self, through_activity_id: u64) -> PlanetActionResult {
        let latest = self
            .activity_log
            .last()
            .map_or(self.acknowledged_activity_id, |record| record.activity_id);
        let acknowledged = through_activity_id.min(latest);
        if acknowledged > self.acknowledged_activity_id {
            self.acknowledged_activity_id = acknowledged;
            self.changed();
        }
        self.action_accepted(None)
    }

    pub fn advance(&mut self, elapsed_ms: u64) -> Vec<GameAdvanceEvent> {
        if self.active_mission.is_none() {
            self.dispatch_next_queued_survey();
        }
        if self.active_extraction.is_none()
            && self.extractor_available
            && self.extraction_route_paused_reason.is_none()
        {
            self.dispatch_extraction_route();
        }
        let mut events = self.advance_scout(elapsed_ms);
        if events.contains(&GameAdvanceEvent::SurveyCompleted)
            && !self.reach_milestone_complete
            && self.relay_region_id.is_some()
            && self.active_mission.as_ref().is_some_and(|mission| {
                self.world
                    .regions
                    .iter()
                    .find(|region| region.id == mission.target_region_id)
                    .is_some_and(|region| hop_distance(&region.position) > SCOUT_RANGE_HOPS)
            })
        {
            self.reach_milestone_complete = true;
            self.push_activity(
                ActivityKind::Milestone,
                None,
                "Extended survey reach proven; research planning is now available.".into(),
            );
            self.changed();
            events.push(GameAdvanceEvent::AlphaMilestoneCompleted);
        }
        events.extend(self.advance_research(elapsed_ms));
        self.advance_refining(elapsed_ms);
        if !self.extractor_available && self.extractor_rebuild_remaining_ms > 0 {
            self.extractor_rebuild_remaining_ms = self
                .extractor_rebuild_remaining_ms
                .saturating_sub(elapsed_ms);
            if self.extractor_rebuild_remaining_ms == 0 {
                self.extractor_available = true;
                self.push_activity(
                    ActivityKind::Recovery,
                    None,
                    "Replacement extractor completed at the core.".into(),
                );
                events.push(GameAdvanceEvent::ExtractorRebuilt);
            }
            self.changed();
        }
        events.extend(self.advance_extraction(elapsed_ms));
        if let Some(mut construction) = self.active_construction.take() {
            construction.remaining_ms = construction.remaining_ms.saturating_sub(elapsed_ms);
            if construction.remaining_ms == 0 {
                let designation = self.region_designation(&construction.region_id);
                self.relay_region_id = Some(construction.region_id.clone());
                self.push_activity(
                    ActivityKind::Construction,
                    Some(construction.region_id),
                    format!("Range relay activated at {designation}."),
                );
                events.push(GameAdvanceEvent::RelayActivated);
            } else {
                self.active_construction = Some(construction);
            }
            self.changed();
        }
        events
    }

    fn advance_scout(&mut self, elapsed_ms: u64) -> Vec<GameAdvanceEvent> {
        let Some(mut mission) = self.active_mission.take() else {
            return Vec::new();
        };
        if mission.phase == ScoutState::Holding || elapsed_ms == 0 {
            self.active_mission = Some(mission);
            return Vec::new();
        }

        let mut remaining_elapsed = elapsed_ms;
        let mut events = Vec::new();
        while remaining_elapsed > 0 && mission.phase != ScoutState::Holding {
            let consumed = remaining_elapsed.min(mission.remaining_ms);
            mission.remaining_ms -= consumed;
            remaining_elapsed -= consumed;
            if mission.remaining_ms > 0 {
                break;
            }
            match mission.phase {
                ScoutState::Outbound => {
                    if let Some(region) = self
                        .world
                        .regions
                        .iter_mut()
                        .find(|region| region.id == mission.target_region_id)
                    {
                        if region.knowledge == RegionKnowledge::Unknown {
                            region.knowledge = RegionKnowledge::Detected;
                            events.push(GameAdvanceEvent::RegionDetected);
                        }
                        if mission.survey_required {
                            mission.phase = ScoutState::Surveying;
                            mission.remaining_ms = self.survey_duration_ms();
                            mission.phase_total_ms = self.survey_duration_ms();
                        } else {
                            region.knowledge = RegionKnowledge::Observed;
                            mission.phase = ScoutState::Holding;
                        }
                    }
                }
                ScoutState::Surveying => {
                    if let Some(region) = self
                        .world
                        .regions
                        .iter_mut()
                        .find(|region| region.id == mission.target_region_id)
                    {
                        region.knowledge = RegionKnowledge::Observed;
                    }
                    mission.survey_completed = true;
                    self.analysis_units = self.analysis_units.saturating_add(1);
                    self.push_report(MissionReport {
                        mission_id: mission.mission_id,
                        region_id: mission.target_region_id.clone(),
                        outcome: MissionReportOutcome::SurveyCompleted,
                    });
                    let designation = self.region_designation(&mission.target_region_id);
                    self.push_activity(
                        ActivityKind::Survey,
                        Some(mission.target_region_id.clone()),
                        format!("Survey completed at {designation}; one analysis unit recorded."),
                    );
                    if mission.auto_return {
                        mission.phase = ScoutState::Returning;
                        mission.remaining_ms = mission.travel_ms;
                        mission.phase_total_ms = mission.travel_ms;
                    } else {
                        mission.phase = ScoutState::Holding;
                    }
                    events.push(GameAdvanceEvent::SurveyCompleted);
                }
                ScoutState::Returning => {
                    self.push_report(MissionReport {
                        mission_id: mission.mission_id,
                        region_id: mission.target_region_id.clone(),
                        outcome: if mission.survey_completed {
                            MissionReportOutcome::Returned
                        } else {
                            MissionReportOutcome::Recalled
                        },
                    });
                    if mission.auto_return && mission.survey_completed {
                        self.automated_survey_completed = true;
                    }
                    events.push(GameAdvanceEvent::ScoutReturned);
                    if self.refresh_alpha_completion() {
                        events.push(GameAdvanceEvent::AlphaSliceCompleted);
                    }
                    self.changed();
                    return events;
                }
                ScoutState::Ready | ScoutState::Holding => break,
            }
        }
        self.active_mission = Some(mission);
        self.changed();
        events
    }

    fn advance_extraction(&mut self, elapsed_ms: u64) -> Vec<GameAdvanceEvent> {
        let Some(mut mission) = self.active_extraction.take() else {
            return Vec::new();
        };
        let mut elapsed = elapsed_ms;
        let mut events = Vec::new();
        while elapsed > 0 {
            let consumed = elapsed.min(mission.remaining_ms);
            mission.remaining_ms -= consumed;
            elapsed -= consumed;
            if mission.remaining_ms > 0 {
                break;
            }
            match mission.phase {
                ExtractorState::Outbound => {
                    mission.phase = ExtractorState::Extracting;
                    mission.remaining_ms = EXTRACTION_MS;
                    mission.phase_total_ms = EXTRACTION_MS;
                }
                ExtractorState::Extracting if mission.will_fail => {
                    self.extractor_available = false;
                    self.extractor_rebuild_remaining_ms = EXTRACTOR_REBUILD_MS;
                    self.push_report(MissionReport {
                        mission_id: mission.mission_id,
                        region_id: mission.target_region_id.clone(),
                        outcome: MissionReportOutcome::ExtractorLost,
                    });
                    let designation = self.region_designation(&mission.target_region_id);
                    self.push_activity(
                        ActivityKind::Loss,
                        Some(mission.target_region_id.clone()),
                        format!("Extractor and cargo were lost near {designation}."),
                    );
                    if mission.automated {
                        self.extraction_route_paused_reason = Some("probe_loss".into());
                        events.push(GameAdvanceEvent::AutomationPaused);
                    }
                    self.changed();
                    events.push(GameAdvanceEvent::ExtractorLost);
                    return events;
                }
                ExtractorState::Extracting => {
                    let capacity = self.extraction_capacity();
                    if let Some(deposit) = self
                        .world
                        .regions
                        .iter_mut()
                        .find(|region| region.id == mission.target_region_id)
                        .and_then(|region| region.deposit.as_mut())
                    {
                        let amount = deposit.remaining.min(capacity);
                        deposit.remaining -= amount;
                        mission.cargo = Some((deposit.resource, amount));
                    }
                    mission.phase = ExtractorState::Returning;
                    mission.remaining_ms = mission.travel_ms;
                    mission.phase_total_ms = mission.travel_ms;
                }
                ExtractorState::Returning => {
                    if let Some((resource, amount)) = mission.cargo {
                        add_stock(&mut self.raw_inventory, resource, amount);
                        let designation = self.region_designation(&mission.target_region_id);
                        self.push_activity(
                            ActivityKind::Extraction,
                            Some(mission.target_region_id.clone()),
                            format!(
                                "Extractor returned {amount} {} units from {designation}.",
                                resource_label(resource)
                            ),
                        );
                        if !self
                            .proven_extraction_region_ids
                            .iter()
                            .any(|region_id| region_id == &mission.target_region_id)
                        {
                            self.proven_extraction_region_ids
                                .push(mission.target_region_id.clone());
                        }
                    }
                    self.push_report(MissionReport {
                        mission_id: mission.mission_id,
                        region_id: mission.target_region_id.clone(),
                        outcome: MissionReportOutcome::ExtractionCompleted,
                    });
                    if mission.automated {
                        self.automated_extraction_completed = true;
                    }
                    self.changed();
                    events.push(GameAdvanceEvent::ExtractionCompleted);
                    if self.refresh_alpha_completion() {
                        events.push(GameAdvanceEvent::AlphaSliceCompleted);
                    }
                    return events;
                }
                ExtractorState::Ready | ExtractorState::Rebuilding => break,
            }
        }
        self.active_extraction = Some(mission);
        self.changed();
        events
    }

    fn advance_refining(&mut self, elapsed_ms: u64) {
        let mut elapsed = elapsed_ms;
        let mut changed = false;
        while elapsed > 0 {
            if self.refining_remaining_ms == 0 {
                if first_stock_kind(self.raw_inventory).is_none() {
                    break;
                }
                self.refining_remaining_ms = self.refining_ms_per_unit();
            }
            let consumed = elapsed.min(self.refining_remaining_ms);
            self.refining_remaining_ms -= consumed;
            elapsed -= consumed;
            changed = true;
            if self.refining_remaining_ms == 0 {
                if let Some(resource) = first_stock_kind(self.raw_inventory) {
                    remove_one(&mut self.raw_inventory, resource);
                    add_stock(&mut self.refined_inventory, resource, 1);
                }
            }
        }
        if changed {
            self.changed();
        }
    }

    fn advance_research(&mut self, elapsed_ms: u64) -> Vec<GameAdvanceEvent> {
        let Some(mut research) = self.active_research.take() else {
            return Vec::new();
        };
        research.remaining_ms = research.remaining_ms.saturating_sub(elapsed_ms);
        if research.remaining_ms > 0 {
            self.active_research = Some(research);
            self.changed();
            return Vec::new();
        }
        if !self.completed_research.contains(&research.project) {
            self.completed_research.push(research.project);
        }
        self.push_activity(
            ActivityKind::Research,
            None,
            format!("Research completed: {}.", research_title(research.project)),
        );
        self.changed();
        vec![GameAdvanceEvent::ResearchCompleted]
    }

    fn dispatch_next_queued_survey(&mut self) {
        while !self.survey_queue.is_empty() {
            let region_id = self.survey_queue.remove(0);
            let Some(region) = self
                .world
                .regions
                .iter()
                .find(|region| region.id == region_id)
            else {
                continue;
            };
            if region.knowledge >= RegionKnowledge::Surveyed
                || !self.region_within_range(&region.position)
            {
                continue;
            }
            let travel_ms = u64::from(hop_distance(&region.position)) * TRAVEL_MS_PER_HOP;
            let mission_id = self.next_mission_id;
            self.next_mission_id += 1;
            self.active_mission = Some(MissionState {
                mission_id,
                target_region_id: region_id,
                phase: ScoutState::Outbound,
                remaining_ms: travel_ms,
                phase_total_ms: travel_ms,
                travel_ms,
                survey_required: true,
                survey_completed: false,
                auto_return: true,
            });
            self.changed();
            return;
        }
    }

    fn dispatch_extraction_route(&mut self) {
        let Some(region_id) = self.extraction_route_region_id.clone() else {
            return;
        };
        let Some(region) = self
            .world
            .regions
            .iter()
            .find(|region| region.id == region_id)
        else {
            self.pause_extraction_route("region_unavailable");
            return;
        };
        if !self.region_within_range(&region.position) {
            self.pause_extraction_route("region_out_of_range");
            return;
        }
        if region
            .deposit
            .as_ref()
            .is_none_or(|deposit| deposit.remaining == 0)
        {
            self.pause_extraction_route("deposit_depleted");
            return;
        }
        let travel_ms = u64::from(hop_distance(&region.position)) * TRAVEL_MS_PER_HOP;
        let risk = self.effective_risk(region.atmospheric_severity);
        let will_fail = self.next_random_percent() < risk_percent(risk);
        let mission_id = self.next_mission_id;
        self.next_mission_id += 1;
        self.active_extraction = Some(ExtractionMissionState {
            mission_id,
            target_region_id: region_id,
            phase: ExtractorState::Outbound,
            remaining_ms: travel_ms,
            phase_total_ms: travel_ms,
            travel_ms,
            risk,
            will_fail,
            cargo: None,
            automated: true,
        });
        self.changed();
    }

    fn pause_extraction_route(&mut self, reason: &str) {
        if self.extraction_route_paused_reason.as_deref() == Some(reason) {
            return;
        }
        self.extraction_route_paused_reason = Some(reason.to_owned());
        self.push_activity(
            ActivityKind::Automation,
            self.extraction_route_region_id.clone(),
            format!("Automated extraction paused: {}.", reason.replace('_', " ")),
        );
        self.changed();
    }

    pub fn state_revision(&self) -> u64 {
        self.state_revision
    }

    pub fn dirty_generation(&self) -> Option<u64> {
        self.dirty.then_some(self.generation)
    }

    pub fn mark_saved(&mut self, generation: u64) {
        if self.generation == generation {
            self.dirty = false;
        }
    }

    fn region_snapshot(&self, region: &RegionState) -> RegionSnapshot {
        let detected = region.knowledge >= RegionKnowledge::Detected;
        let surveyed = region.knowledge >= RegionKnowledge::Surveyed;
        RegionSnapshot {
            id: region.id.clone(),
            designation: region.designation.clone(),
            position: region.position.clone(),
            core: region.id == self.world.core_region_id,
            knowledge: region.knowledge,
            terrain: detected.then_some(region.terrain),
            temperature: surveyed.then_some(region.temperature),
            atmospheric_severity: surveyed.then_some(region.atmospheric_severity),
            confidence: match region.knowledge {
                RegionKnowledge::Unknown => None,
                RegionKnowledge::Detected => Some(SurveyConfidence::Low),
                RegionKnowledge::Surveyed => Some(SurveyConfidence::Medium),
                RegionKnowledge::Observed => Some(SurveyConfidence::High),
            },
            hop_distance_from_core: hop_distance(&region.position),
            within_scout_range: region.id != self.world.core_region_id
                && self.region_within_range(&region.position),
            deposit: surveyed
                .then_some(region.deposit.as_ref())
                .flatten()
                .map(|deposit| DepositSnapshot {
                    resource: deposit.resource,
                    remaining: deposit.remaining,
                    risk: self.effective_risk(region.atmospheric_severity),
                }),
            relay: self.relay_region_id.as_deref() == Some(region.id.as_str()),
        }
    }

    fn extractor_state(&self) -> ExtractorState {
        self.active_extraction.as_ref().map_or_else(
            || {
                if self.extractor_available {
                    ExtractorState::Ready
                } else {
                    ExtractorState::Rebuilding
                }
            },
            |mission| mission.phase,
        )
    }

    fn objective_snapshot(&self) -> ObjectiveSnapshot {
        if self.alpha_complete {
            return ObjectiveSnapshot {
                title: "ALPHA OPERATIONS STABLE".into(),
                guidance: "A specialization and an earned automated operation are proven. Continue developing the network.".into(),
                complete: true,
            };
        }
        if let Some(research) = &self.active_research {
            return ObjectiveSnapshot {
                title: "RESEARCH IN PROGRESS".into(),
                guidance: format!(
                    "{} is resolving. Field operations may continue.",
                    research_title(research.project)
                ),
                complete: false,
            };
        }
        if self.reach_milestone_complete && self.completed_research.is_empty() {
            return ObjectiveSnapshot {
                title: "CHOOSE A RESEARCH DIRECTION".into(),
                guidance: "Open Research and commit analysis to reach, resilience, or throughput."
                    .into(),
                complete: false,
            };
        }
        if self.reach_milestone_complete && self.operational_profile == OperationalProfile::Balanced
        {
            return ObjectiveSnapshot {
                title: "ACTIVATE A SPECIALIZATION".into(),
                guidance: "Open Research and activate the profile unlocked by completed work."
                    .into(),
                complete: false,
            };
        }
        if self.reach_milestone_complete {
            return ObjectiveSnapshot {
                title: "PROVE EARNED AUTOMATION".into(),
                guidance:
                    "Queue an eligible survey or activate a previously proven extraction route."
                        .into(),
                complete: false,
            };
        }
        if self.relay_region_id.is_some() {
            return ObjectiveSnapshot {
                title: "PROVE EXTENDED REACH".into(),
                guidance: "Survey a region that lies beyond the core's original two-hop range."
                    .into(),
                complete: false,
            };
        }
        if self.active_construction.is_some() {
            return ObjectiveSnapshot {
                title: "RELAY CONSTRUCTION".into(),
                guidance: "Construction is autonomous. Exploration and extraction may continue."
                    .into(),
                complete: false,
            };
        }
        if stock_contains(self.refined_inventory, RELAY_COST) {
            return ObjectiveSnapshot {
                title: "ESTABLISH A RELAY".into(),
                guidance: "Open a surveyed reachable region and press B to begin construction."
                    .into(),
                complete: false,
            };
        }
        if self.world.regions.iter().any(|region| {
            region.knowledge >= RegionKnowledge::Surveyed
                && region
                    .deposit
                    .as_ref()
                    .is_some_and(|deposit| deposit.remaining > 0)
        }) {
            return ObjectiveSnapshot {
                title: "GATHER RELAY MATERIALS".into(),
                guidance: format!(
                    "Extract and refine S {}/8  C {}/6  Si {}/6.",
                    self.refined_inventory.structural,
                    self.refined_inventory.conductive,
                    self.refined_inventory.silicate
                ),
                complete: false,
            };
        }
        ObjectiveSnapshot {
            title: "RESOLVE LOCAL GEOLOGY".into(),
            guidance:
                "Survey nearby regions to locate structural, conductive, and silicate deposits."
                    .into(),
            complete: false,
        }
    }

    fn region_within_range(&self, position: &RegionPosition) -> bool {
        if hop_distance(position) <= self.scout_range_hops() {
            return true;
        }
        self.relay_region_id
            .as_ref()
            .and_then(|id| self.world.regions.iter().find(|region| &region.id == id))
            .is_some_and(|relay| {
                graph_distance(position, &relay.position) <= self.relay_range_hops()
            })
    }

    fn research_snapshot(&self) -> ResearchSnapshot {
        let unlocked = self.reach_milestone_complete;
        ResearchSnapshot {
            unlocked,
            analysis_units: self.analysis_units,
            active: self
                .active_research
                .as_ref()
                .map(|research| ActiveResearchSnapshot {
                    project: research.project,
                    remaining_ms: research.remaining_ms,
                    phase_total_ms: research.phase_total_ms,
                }),
            options: research_projects()
                .into_iter()
                .map(|project| ResearchOptionSnapshot {
                    project,
                    title: research_title(project).into(),
                    summary: research_summary(project).into(),
                    analysis_cost: RESEARCH_ANALYSIS_COST,
                    duration_ms: RESEARCH_DURATION_MS,
                    available: unlocked
                        && self.active_research.is_none()
                        && self.analysis_units >= RESEARCH_ANALYSIS_COST
                        && !self.completed_research.contains(&project),
                    completed: self.completed_research.contains(&project),
                })
                .collect(),
            profiles: operational_profiles()
                .into_iter()
                .map(|profile| OperationalProfileSnapshot {
                    profile,
                    title: profile_title(profile).into(),
                    summary: profile_summary(profile).into(),
                    unlocked: self.profile_unlocked(profile),
                    active: self.operational_profile == profile,
                })
                .collect(),
        }
    }

    fn automation_unlocked(&self) -> bool {
        !self.completed_research.is_empty()
    }

    fn profile_unlocked(&self, profile: OperationalProfile) -> bool {
        match profile {
            OperationalProfile::Balanced => true,
            OperationalProfile::Reach => self
                .completed_research
                .contains(&ResearchProject::ExtendedTelemetry),
            OperationalProfile::Resilience => self
                .completed_research
                .contains(&ResearchProject::FieldHardening),
            OperationalProfile::Throughput => self
                .completed_research
                .contains(&ResearchProject::ParallelProcessing),
        }
    }

    fn scout_range_hops(&self) -> u8 {
        SCOUT_RANGE_HOPS + u8::from(self.operational_profile == OperationalProfile::Reach)
    }

    fn relay_range_hops(&self) -> u8 {
        RELAY_RANGE_HOPS + u8::from(self.operational_profile == OperationalProfile::Reach)
    }

    fn survey_duration_ms(&self) -> u64 {
        if self.operational_profile == OperationalProfile::Reach {
            20_000
        } else {
            SURVEY_MS
        }
    }

    fn effective_risk(&self, severity: AtmosphericSeverity) -> RiskLevel {
        let risk = risk_for(severity);
        if self.operational_profile != OperationalProfile::Resilience {
            return risk;
        }
        match risk {
            RiskLevel::High => RiskLevel::Moderate,
            RiskLevel::Moderate | RiskLevel::Low => RiskLevel::Low,
        }
    }

    fn extraction_capacity(&self) -> u32 {
        EXTRACTION_CAPACITY
            + if self.operational_profile == OperationalProfile::Throughput {
                2
            } else {
                0
            }
    }

    fn refining_ms_per_unit(&self) -> u64 {
        if self.operational_profile == OperationalProfile::Throughput {
            3_000
        } else {
            REFINING_MS_PER_UNIT
        }
    }

    fn refresh_alpha_completion(&mut self) -> bool {
        if self.alpha_complete
            || self.operational_profile == OperationalProfile::Balanced
            || !(self.automated_survey_completed || self.automated_extraction_completed)
        {
            return false;
        }
        self.alpha_complete = true;
        self.push_activity(
            ActivityKind::Milestone,
            None,
            "First-session operations milestone completed.".into(),
        );
        self.changed();
        true
    }

    fn region_designation(&self, region_id: &str) -> String {
        self.world
            .regions
            .iter()
            .find(|region| region.id == region_id)
            .map_or_else(|| region_id.to_owned(), |region| region.designation.clone())
    }

    fn push_activity(&mut self, kind: ActivityKind, region_id: Option<String>, summary: String) {
        let record = ActivityRecord {
            activity_id: self.next_activity_id,
            kind,
            region_id,
            summary,
        };
        self.next_activity_id = self.next_activity_id.saturating_add(1);
        if self.activity_log.len() == ACTIVITY_CAPACITY {
            self.activity_log.remove(0);
        }
        self.activity_log.push(record);
    }

    fn next_random_percent(&mut self) -> u32 {
        let mut random = SplitMix64(self.random_state);
        let value = random.next_u32() % 100;
        self.random_state = random.0;
        value
    }

    fn changed(&mut self) {
        self.state_revision = self.state_revision.saturating_add(1);
        self.dirty = true;
        self.generation = self.generation.saturating_add(1);
    }

    fn push_report(&mut self, report: MissionReport) {
        if self.recent_reports.len() == REPORT_CAPACITY {
            self.recent_reports.remove(0);
        }
        self.recent_reports.push(report);
    }

    fn onboarding_rejected(&self, code: &str) -> OnboardingCompletionResult {
        OnboardingCompletionResult {
            ok: false,
            code: Some(code.to_owned()),
            state_revision: self.state_revision,
            completed: self.onboarding_complete,
        }
    }

    fn action_rejected(&self, code: &str) -> PlanetActionResult {
        PlanetActionResult {
            ok: false,
            code: Some(code.to_owned()),
            revision: self.state_revision,
            mission_id: self
                .active_mission
                .as_ref()
                .map(|mission| mission.mission_id),
        }
    }

    fn action_accepted(&self, mission_id: Option<u64>) -> PlanetActionResult {
        PlanetActionResult {
            ok: true,
            code: None,
            revision: self.state_revision,
            mission_id,
        }
    }

    fn validate(&self) -> Result<(), GameError> {
        if self.world.generation_version != WORLD_GENERATION_VERSION {
            return Err(GameError::Invariant("unsupported world generation version"));
        }
        if self.world.regions.len() != REGION_COUNT {
            return Err(GameError::Invariant("planet must contain 200 regions"));
        }
        for latitude_band in 0..LATITUDE_BANDS {
            for longitude_sector in 0..LONGITUDE_SECTORS {
                let matches = self
                    .world
                    .regions
                    .iter()
                    .filter(|region| {
                        region.position.latitude_band == latitude_band
                            && region.position.longitude_sector == longitude_sector
                    })
                    .count();
                if matches != 1 {
                    return Err(GameError::Invariant("region positions must be unique"));
                }
            }
        }
        let Some(core) = self
            .world
            .regions
            .iter()
            .find(|region| region.id == self.world.core_region_id)
        else {
            return Err(GameError::Invariant("core region must exist"));
        };
        if core.knowledge != RegionKnowledge::Observed {
            return Err(GameError::Invariant("core region must remain observed"));
        }
        if let Some(mission) = &self.active_mission {
            if mission.phase == ScoutState::Ready
                || !self
                    .world
                    .regions
                    .iter()
                    .any(|region| region.id == mission.target_region_id)
                || mission.travel_ms == 0
                || (mission.phase != ScoutState::Holding && mission.remaining_ms == 0)
            {
                return Err(GameError::Invariant("active mission is inconsistent"));
            }
        }
        if let Some(mission) = &self.active_extraction {
            if !self.extractor_available
                || !self
                    .world
                    .regions
                    .iter()
                    .any(|region| region.id == mission.target_region_id)
                || !matches!(
                    mission.phase,
                    ExtractorState::Outbound
                        | ExtractorState::Extracting
                        | ExtractorState::Returning
                )
                || mission.remaining_ms == 0
                || mission.travel_ms == 0
            {
                return Err(GameError::Invariant("active extraction is inconsistent"));
            }
        }
        if !self.extractor_available && self.extractor_rebuild_remaining_ms == 0 {
            return Err(GameError::Invariant(
                "unavailable extractor must be rebuilding",
            ));
        }
        if self.extractor_available && self.extractor_rebuild_remaining_ms > 0 {
            return Err(GameError::Invariant(
                "available extractor cannot be rebuilding",
            ));
        }
        if let Some(region_id) = &self.relay_region_id {
            if region_id == &self.world.core_region_id
                || !self
                    .world
                    .regions
                    .iter()
                    .any(|region| &region.id == region_id)
            {
                return Err(GameError::Invariant("relay region is inconsistent"));
            }
        }
        if let Some(construction) = &self.active_construction {
            if self.relay_region_id.is_some()
                || construction.remaining_ms == 0
                || !self
                    .world
                    .regions
                    .iter()
                    .any(|region| region.id == construction.region_id)
            {
                return Err(GameError::Invariant("relay construction is inconsistent"));
            }
        }
        if self.random_state == 0 {
            return Err(GameError::Invariant("random state must be initialized"));
        }
        if self.recent_reports.len() > REPORT_CAPACITY {
            return Err(GameError::Invariant("mission report capacity exceeded"));
        }
        if self.completed_research.len() > research_projects().len()
            || (!self.completed_research.is_empty() && !self.reach_milestone_complete)
            || self.completed_research.iter().any(|project| {
                self.completed_research
                    .iter()
                    .filter(|candidate| *candidate == project)
                    .count()
                    != 1
            })
        {
            return Err(GameError::Invariant("completed research is inconsistent"));
        }
        if self.active_research.as_ref().is_some_and(|research| {
            !self.reach_milestone_complete
                || research.remaining_ms == 0
                || research.phase_total_ms != RESEARCH_DURATION_MS
                || self.completed_research.contains(&research.project)
        }) {
            return Err(GameError::Invariant("active research is inconsistent"));
        }
        if !self.profile_unlocked(self.operational_profile) {
            return Err(GameError::Invariant("active profile is locked"));
        }
        if self.survey_queue.len() > SURVEY_QUEUE_CAPACITY
            || self.survey_queue.iter().any(|region_id| {
                !self
                    .world
                    .regions
                    .iter()
                    .any(|region| &region.id == region_id)
            })
            || self.survey_queue.iter().any(|region_id| {
                self.survey_queue
                    .iter()
                    .filter(|candidate| *candidate == region_id)
                    .count()
                    != 1
            })
        {
            return Err(GameError::Invariant("survey queue is inconsistent"));
        }
        if self.proven_extraction_region_ids.iter().any(|region_id| {
            self.proven_extraction_region_ids
                .iter()
                .filter(|candidate| *candidate == region_id)
                .count()
                != 1
                || !self.world.regions.iter().any(|region| {
                    &region.id == region_id
                        && region.knowledge >= RegionKnowledge::Surveyed
                        && region.deposit.is_some()
                })
        }) {
            return Err(GameError::Invariant(
                "proven extraction routes are inconsistent",
            ));
        }
        if self
            .extraction_route_region_id
            .as_ref()
            .is_some_and(|region_id| {
                !self
                    .proven_extraction_region_ids
                    .iter()
                    .any(|proven| proven == region_id)
            })
            || (self.extraction_route_region_id.is_none()
                && self.extraction_route_paused_reason.is_some())
        {
            return Err(GameError::Invariant(
                "active extraction route is inconsistent",
            ));
        }
        if self
            .active_mission
            .as_ref()
            .is_some_and(|mission| mission.auto_return && !self.automation_unlocked())
            || self.active_extraction.as_ref().is_some_and(|mission| {
                mission.automated
                    && (self.extraction_route_region_id.as_deref()
                        != Some(mission.target_region_id.as_str())
                        || !self.automation_unlocked())
            })
        {
            return Err(GameError::Invariant("automated mission is inconsistent"));
        }
        if self.reach_milestone_complete && self.relay_region_id.is_none()
            || self.alpha_complete
                && (!self.reach_milestone_complete
                    || self.operational_profile == OperationalProfile::Balanced
                    || !(self.automated_survey_completed || self.automated_extraction_completed))
        {
            return Err(GameError::Invariant("alpha progression is inconsistent"));
        }
        if self.activity_log.len() > ACTIVITY_CAPACITY
            || self
                .activity_log
                .windows(2)
                .any(|records| records[0].activity_id >= records[1].activity_id)
            || self
                .activity_log
                .last()
                .is_some_and(|record| self.next_activity_id <= record.activity_id)
            || self.acknowledged_activity_id >= self.next_activity_id
        {
            return Err(GameError::Invariant("activity log is inconsistent"));
        }
        Ok(())
    }
}

impl WorldState {
    fn generate(seed: u64) -> Self {
        let mut random = SplitMix64(seed);
        let mut regions = Vec::with_capacity(REGION_COUNT);
        for latitude_band in 0..LATITUDE_BANDS {
            for longitude_sector in 0..LONGITUDE_SECTORS {
                let index = usize::from(latitude_band) * usize::from(LONGITUDE_SECTORS)
                    + usize::from(longitude_sector);
                let terrain = generate_terrain(latitude_band, random.next_u32() % 100);
                let temperature = generate_temperature(latitude_band, random.next_u32() % 100);
                let atmospheric_severity = match random.next_u32() % 10 {
                    0..=4 => AtmosphericSeverity::Stable,
                    5..=8 => AtmosphericSeverity::Variable,
                    _ => AtmosphericSeverity::Severe,
                };
                let core = latitude_band == CORE_LATITUDE_BAND
                    && longitude_sector == CORE_LONGITUDE_SECTOR;
                let deposit_roll = random.next_u32();
                let resource = match (latitude_band, longitude_sector) {
                    (CORE_LATITUDE_BAND, 1) => ResourceKind::Structural,
                    (CORE_LATITUDE_BAND, sector)
                        if sector == LONGITUDE_SECTORS.saturating_sub(1) =>
                    {
                        ResourceKind::Conductive
                    }
                    (band, CORE_LONGITUDE_SECTOR)
                        if band == CORE_LATITUDE_BAND.saturating_sub(1) =>
                    {
                        ResourceKind::Silicate
                    }
                    _ => match deposit_roll % 3 {
                        0 => ResourceKind::Structural,
                        1 => ResourceKind::Conductive,
                        _ => ResourceKind::Silicate,
                    },
                };
                regions.push(RegionState {
                    id: format!("region_{:03}", index + 1),
                    designation: format!("R-{:03}", index + 1),
                    position: RegionPosition {
                        latitude_band,
                        longitude_sector,
                    },
                    terrain,
                    temperature,
                    atmospheric_severity,
                    knowledge: if core {
                        RegionKnowledge::Observed
                    } else {
                        RegionKnowledge::Unknown
                    },
                    deposit: (!core).then_some(DepositState {
                        resource,
                        remaining: 8 + ((deposit_roll / 3) % 5) * 4,
                    }),
                });
            }
        }
        let core_region_id = regions
            .iter()
            .find(|region| {
                region.position.latitude_band == CORE_LATITUDE_BAND
                    && region.position.longitude_sector == CORE_LONGITUDE_SECTOR
            })
            .expect("generated world contains core position")
            .id
            .clone();
        Self {
            seed,
            generation_version: WORLD_GENERATION_VERSION,
            core_region_id,
            regions,
        }
    }
}

fn hop_distance(position: &RegionPosition) -> u8 {
    let latitude = position.latitude_band.abs_diff(CORE_LATITUDE_BAND);
    let longitude = position.longitude_sector.abs_diff(CORE_LONGITUDE_SECTOR);
    latitude + longitude.min(LONGITUDE_SECTORS - longitude)
}

fn graph_distance(first: &RegionPosition, second: &RegionPosition) -> u8 {
    let latitude = first.latitude_band.abs_diff(second.latitude_band);
    let longitude = first.longitude_sector.abs_diff(second.longitude_sector);
    latitude + longitude.min(LONGITUDE_SECTORS - longitude)
}

fn risk_for(severity: AtmosphericSeverity) -> RiskLevel {
    match severity {
        AtmosphericSeverity::Stable => RiskLevel::Low,
        AtmosphericSeverity::Variable => RiskLevel::Moderate,
        AtmosphericSeverity::Severe => RiskLevel::High,
    }
}

fn risk_percent(risk: RiskLevel) -> u32 {
    match risk {
        RiskLevel::Low => 5,
        RiskLevel::Moderate => 15,
        RiskLevel::High => 30,
    }
}

fn research_projects() -> [ResearchProject; 3] {
    [
        ResearchProject::ExtendedTelemetry,
        ResearchProject::FieldHardening,
        ResearchProject::ParallelProcessing,
    ]
}

fn research_title(project: ResearchProject) -> &'static str {
    match project {
        ResearchProject::ExtendedTelemetry => "Extended Telemetry",
        ResearchProject::FieldHardening => "Field Hardening",
        ResearchProject::ParallelProcessing => "Parallel Processing",
    }
}

fn research_summary(project: ResearchProject) -> &'static str {
    match project {
        ResearchProject::ExtendedTelemetry => {
            "Unlocks Reach profile: +1 operational hop and faster surveys."
        }
        ResearchProject::FieldHardening => {
            "Unlocks Resilience profile: reduces published extraction risk by one tier."
        }
        ResearchProject::ParallelProcessing => {
            "Unlocks Throughput profile: +2 cargo and faster refining."
        }
    }
}

fn operational_profiles() -> [OperationalProfile; 4] {
    [
        OperationalProfile::Balanced,
        OperationalProfile::Reach,
        OperationalProfile::Resilience,
        OperationalProfile::Throughput,
    ]
}

fn profile_title(profile: OperationalProfile) -> &'static str {
    match profile {
        OperationalProfile::Balanced => "Balanced",
        OperationalProfile::Reach => "Reach",
        OperationalProfile::Resilience => "Resilience",
        OperationalProfile::Throughput => "Throughput",
    }
}

fn profile_summary(profile: OperationalProfile) -> &'static str {
    match profile {
        OperationalProfile::Balanced => "Baseline survey, extraction, and refining behavior.",
        OperationalProfile::Reach => "+1 operational hop; focused surveys resolve in 20 seconds.",
        OperationalProfile::Resilience => "Published extraction risk is reduced by one tier.",
        OperationalProfile::Throughput => {
            "+2 extractor capacity; refining takes 3 seconds per unit."
        }
    }
}

fn resource_label(resource: ResourceKind) -> &'static str {
    match resource {
        ResourceKind::Structural => "structural",
        ResourceKind::Conductive => "conductive",
        ResourceKind::Silicate => "silicate",
    }
}

fn stock_contains(stock: ResourceStock, required: ResourceStock) -> bool {
    stock.structural >= required.structural
        && stock.conductive >= required.conductive
        && stock.silicate >= required.silicate
}

fn subtract_stock(stock: &mut ResourceStock, amount: ResourceStock) {
    stock.structural -= amount.structural;
    stock.conductive -= amount.conductive;
    stock.silicate -= amount.silicate;
}

fn add_stock(stock: &mut ResourceStock, resource: ResourceKind, amount: u32) {
    match resource {
        ResourceKind::Structural => stock.structural += amount,
        ResourceKind::Conductive => stock.conductive += amount,
        ResourceKind::Silicate => stock.silicate += amount,
    }
}

#[cfg(test)]
fn stock_amount(stock: ResourceStock, resource: ResourceKind) -> u32 {
    match resource {
        ResourceKind::Structural => stock.structural,
        ResourceKind::Conductive => stock.conductive,
        ResourceKind::Silicate => stock.silicate,
    }
}

fn remove_one(stock: &mut ResourceStock, resource: ResourceKind) {
    match resource {
        ResourceKind::Structural => stock.structural -= 1,
        ResourceKind::Conductive => stock.conductive -= 1,
        ResourceKind::Silicate => stock.silicate -= 1,
    }
}

fn first_stock_kind(stock: ResourceStock) -> Option<ResourceKind> {
    if stock.structural > 0 {
        Some(ResourceKind::Structural)
    } else if stock.conductive > 0 {
        Some(ResourceKind::Conductive)
    } else if stock.silicate > 0 {
        Some(ResourceKind::Silicate)
    } else {
        None
    }
}

fn generate_terrain(latitude_band: u8, roll: u32) -> TerrainClass {
    if (latitude_band == 0 || latitude_band == LATITUDE_BANDS - 1) && roll < 45 {
        return TerrainClass::Polar;
    }
    match roll % 6 {
        0 => TerrainClass::Basin,
        1 => TerrainClass::Plain,
        2 => TerrainClass::Highland,
        3 => TerrainClass::Ridge,
        4 => TerrainClass::Fractured,
        _ => TerrainClass::Plain,
    }
}

fn generate_temperature(latitude_band: u8, roll: u32) -> TemperatureBand {
    let equator_low = LATITUDE_BANDS / 2 - 1;
    let equator_high = LATITUDE_BANDS / 2;
    let distance_from_equator = latitude_band
        .abs_diff(equator_low)
        .min(latitude_band.abs_diff(equator_high));
    match distance_from_equator {
        4.. if roll < 70 => TemperatureBand::Frigid,
        4.. => TemperatureBand::Cold,
        3 if roll < 35 => TemperatureBand::Cold,
        0..=1 if roll > 80 => TemperatureBand::Hot,
        _ => TemperatureBand::Temperate,
    }
}

struct SplitMix64(u64);

impl SplitMix64 {
    fn next_u32(&mut self) -> u32 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((value ^ (value >> 31)) >> 32) as u32
    }
}

#[derive(Debug, Error)]
pub enum GameError {
    #[error("game-state I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("invalid game-state JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("game-state path has no parent directory")]
    MissingParent,
    #[error("unsupported game state schema version {0}")]
    UnsupportedVersion(u32),
    #[error("game-state invariant failed: {0}")]
    Invariant(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn awaken(state: &mut GameState) {
        assert!(state.complete_onboarding([4, 4, 4]).ok);
    }

    #[test]
    fn deterministic_world_has_expected_topology_and_hidden_projection() {
        let mut first = GameState::with_seed(42);
        let mut second = GameState::with_seed(42);
        awaken(&mut first);
        awaken(&mut second);
        assert_eq!(first.world, second.world);
        let snapshot = first.planet_snapshot().unwrap();
        assert_eq!(snapshot.latitude_bands, 10);
        assert_eq!(snapshot.longitude_sectors, 20);
        assert_eq!(snapshot.regions.len(), 200);
        assert_eq!(
            snapshot
                .regions
                .iter()
                .filter(|region| region.knowledge == RegionKnowledge::Observed)
                .count(),
            1
        );
        let unknown = snapshot
            .regions
            .iter()
            .find(|region| region.knowledge == RegionKnowledge::Unknown)
            .unwrap();
        assert!(unknown.terrain.is_none());
        assert!(unknown.temperature.is_none());
        assert!(unknown.deposit.is_none());
        let json = serde_json::to_value(unknown).unwrap();
        assert!(json.get("terrain").is_none());
        assert!(json.get("temperature").is_none());
        assert!(json.get("deposit").is_none());
    }

    #[test]
    fn initial_reach_always_contains_materials_for_the_first_relay() {
        for seed in 0..128 {
            let state = GameState::with_seed(seed);
            let mut available = ResourceStock::default();
            for region in state
                .world
                .regions
                .iter()
                .filter(|region| !region.id.eq(&state.world.core_region_id))
                .filter(|region| hop_distance(&region.position) <= SCOUT_RANGE_HOPS)
            {
                let deposit = region.deposit.as_ref().unwrap();
                add_stock(&mut available, deposit.resource, deposit.remaining);
            }
            assert!(stock_contains(available, RELAY_COST), "seed {seed}");
        }
    }

    #[test]
    fn mission_detects_surveys_holds_and_returns() {
        let mut state = GameState::with_seed(7);
        awaken(&mut state);
        let target = state
            .planet_snapshot()
            .unwrap()
            .regions
            .into_iter()
            .find(|region| region.hop_distance_from_core == 1)
            .unwrap();
        assert!(state.deploy_scout(&target.id).ok);
        assert_eq!(
            state.advance(15_000),
            vec![GameAdvanceEvent::RegionDetected]
        );
        let snapshot = state.planet_snapshot().unwrap();
        assert_eq!(
            snapshot.active_mission.unwrap().phase,
            ScoutState::Surveying
        );
        assert_eq!(
            snapshot
                .regions
                .iter()
                .find(|region| region.id == target.id)
                .unwrap()
                .knowledge,
            RegionKnowledge::Detected
        );
        assert_eq!(
            state.advance(30_000),
            vec![GameAdvanceEvent::SurveyCompleted]
        );
        let snapshot = state.planet_snapshot().unwrap();
        assert_eq!(snapshot.scout.state, ScoutState::Holding);
        assert_eq!(
            snapshot
                .regions
                .iter()
                .find(|region| region.id == target.id)
                .unwrap()
                .knowledge,
            RegionKnowledge::Observed
        );
        assert!(state.recall_scout().ok);
        assert_eq!(state.advance(15_000), vec![GameAdvanceEvent::ScoutReturned]);
        let snapshot = state.planet_snapshot().unwrap();
        assert_eq!(snapshot.scout.state, ScoutState::Ready);
        assert_eq!(
            snapshot
                .regions
                .iter()
                .find(|region| region.id == target.id)
                .unwrap()
                .knowledge,
            RegionKnowledge::Surveyed
        );
    }

    #[test]
    fn early_recall_preserves_only_arrival_detection() {
        let mut state = GameState::with_seed(9);
        awaken(&mut state);
        let target = state
            .planet_snapshot()
            .unwrap()
            .regions
            .into_iter()
            .find(|region| region.hop_distance_from_core == 1)
            .unwrap();
        state.deploy_scout(&target.id);
        state.advance(15_000);
        state.advance(5_000);
        state.recall_scout();
        state.advance(15_000);
        let region = state
            .planet_snapshot()
            .unwrap()
            .regions
            .into_iter()
            .find(|region| region.id == target.id)
            .unwrap();
        assert_eq!(region.knowledge, RegionKnowledge::Detected);
        assert!(region.terrain.is_some());
        assert!(region.temperature.is_none());
    }

    #[test]
    fn two_hop_timing_and_rule_rejections_are_explicit() {
        let mut state = GameState::with_seed(11);
        assert_eq!(
            state.deploy_scout("region_001").code.as_deref(),
            Some("onboarding_required")
        );
        awaken(&mut state);
        assert_eq!(
            state.deploy_scout("missing").code.as_deref(),
            Some("unknown_region")
        );
        let snapshot = state.planet_snapshot().unwrap();
        let distant = snapshot
            .regions
            .iter()
            .find(|region| region.hop_distance_from_core > 2)
            .unwrap();
        assert_eq!(
            state.deploy_scout(&distant.id).code.as_deref(),
            Some("region_out_of_range")
        );
        let target = snapshot
            .regions
            .iter()
            .find(|region| region.hop_distance_from_core == 2)
            .unwrap();
        assert!(state.deploy_scout(&target.id).ok);
        assert_eq!(
            state.deploy_scout(&target.id).code.as_deref(),
            Some("scout_unavailable")
        );
        state.advance(29_999);
        assert_eq!(
            state.planet_snapshot().unwrap().scout.state,
            ScoutState::Outbound
        );
        assert_eq!(state.advance(1), vec![GameAdvanceEvent::RegionDetected]);
        state.advance(30_000);
        assert!(state.recall_scout().ok);
        assert_eq!(
            state.recall_scout().code.as_deref(),
            Some("already_returning")
        );
    }

    #[test]
    fn surveyed_region_can_be_observed_again_after_return() {
        let mut state = GameState::with_seed(17);
        awaken(&mut state);
        let target = state
            .planet_snapshot()
            .unwrap()
            .regions
            .into_iter()
            .find(|region| region.hop_distance_from_core == 1)
            .unwrap();
        state.deploy_scout(&target.id);
        state.advance(45_000);
        state.recall_scout();
        state.advance(15_000);
        state.deploy_scout(&target.id);
        let events = state.advance(15_000);
        assert!(events.is_empty());
        let snapshot = state.planet_snapshot().unwrap();
        assert_eq!(snapshot.scout.state, ScoutState::Holding);
        assert_eq!(
            snapshot
                .regions
                .iter()
                .find(|region| region.id == target.id)
                .unwrap()
                .knowledge,
            RegionKnowledge::Observed
        );
    }

    #[test]
    fn persists_schema_five_and_resets_previous_alpha_saves() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("game_state.json");
        let mut state = GameState::with_seed(13);
        awaken(&mut state);
        state.save(&path).unwrap();
        assert_eq!(
            GameState::load(&path).unwrap().disposition,
            LoadDisposition::Current
        );

        for version in RESETTABLE_SCHEMA_VERSIONS {
            let mut previous: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            previous["schema_version"] = Value::from(version);
            fs::write(&path, serde_json::to_vec_pretty(&previous).unwrap()).unwrap();
            let reset = GameState::load(&path).unwrap();
            assert_eq!(reset.disposition, LoadDisposition::PreviousReset);
            assert!(!reset.state.onboarding().complete);
            let replacement: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            assert_eq!(replacement["schema_version"], SAVE_SCHEMA_VERSION);
            assert_eq!(
                replacement["state"]["world"]["regions"]
                    .as_array()
                    .unwrap()
                    .len(),
                REGION_COUNT
            );
        }
    }

    #[test]
    fn extraction_loss_is_fixed_at_acceptance_and_rebuilds_without_softlock() {
        let mut state = GameState::with_seed(21);
        awaken(&mut state);
        let target = state
            .planet_snapshot()
            .unwrap()
            .regions
            .into_iter()
            .find(|region| region.hop_distance_from_core == 1)
            .unwrap();
        assert!(state.deploy_scout(&target.id).ok);
        state.advance(45_000);
        assert!(state.deploy_extractor(&target.id).ok);
        state.active_extraction.as_mut().unwrap().will_fail = true;
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("game_state.json");
        state.save(&path).unwrap();
        let restored = GameState::load(&path).unwrap().state;
        assert!(restored.active_extraction.unwrap().will_fail);
        let events = state.advance(45_000);
        assert!(events.contains(&GameAdvanceEvent::ExtractorLost));
        let snapshot = state.planet_snapshot().unwrap();
        assert_eq!(snapshot.extractor.state, ExtractorState::Rebuilding);
        assert!(
            state.deploy_extractor(&target.id).code.as_deref() == Some("extractor_unavailable")
        );
        let events = state.advance(EXTRACTOR_REBUILD_MS);
        assert!(events.contains(&GameAdvanceEvent::ExtractorRebuilt));
        assert_eq!(
            state.planet_snapshot().unwrap().extractor.state,
            ExtractorState::Ready
        );
    }

    #[test]
    fn successful_extraction_conserves_material_through_refining() {
        let mut state = GameState::with_seed(31);
        awaken(&mut state);
        let target = state
            .planet_snapshot()
            .unwrap()
            .regions
            .into_iter()
            .find(|region| region.hop_distance_from_core == 1)
            .unwrap();
        assert!(state.deploy_scout(&target.id).ok);
        state.advance(45_000);
        let before = state
            .world
            .regions
            .iter()
            .find(|region| region.id == target.id)
            .unwrap()
            .deposit
            .clone()
            .unwrap();
        assert!(state.deploy_extractor(&target.id).ok);
        state.active_extraction.as_mut().unwrap().will_fail = false;
        state.advance(15_000);
        state.advance(EXTRACTION_MS);
        let events = state.advance(15_000);
        assert!(events.contains(&GameAdvanceEvent::ExtractionCompleted));
        assert_eq!(
            stock_amount(state.raw_inventory, before.resource),
            EXTRACTION_CAPACITY
        );
        assert_eq!(
            state
                .world
                .regions
                .iter()
                .find(|region| region.id == target.id)
                .unwrap()
                .deposit
                .as_ref()
                .unwrap()
                .remaining,
            before.remaining - EXTRACTION_CAPACITY
        );
        state.advance(REFINING_MS_PER_UNIT * u64::from(EXTRACTION_CAPACITY));
        assert_eq!(stock_amount(state.raw_inventory, before.resource), 0);
        assert_eq!(
            stock_amount(state.refined_inventory, before.resource),
            EXTRACTION_CAPACITY
        );
    }

    #[test]
    fn relay_extends_reach_and_distant_survey_unlocks_research() {
        let mut state = GameState::with_seed(22);
        awaken(&mut state);
        let relay_site = state
            .planet_snapshot()
            .unwrap()
            .regions
            .into_iter()
            .find(|region| region.hop_distance_from_core == 2)
            .unwrap();
        assert!(state.deploy_scout(&relay_site.id).ok);
        state.advance(60_000);
        state.refined_inventory = RELAY_COST;
        assert!(state.construct_relay(&relay_site.id).ok);
        state.advance(RELAY_CONSTRUCTION_MS);
        let distant = state
            .planet_snapshot()
            .unwrap()
            .regions
            .into_iter()
            .find(|region| region.hop_distance_from_core > 2 && region.within_scout_range)
            .unwrap();
        state.recall_scout();
        state.advance(30_000);
        assert!(state.deploy_scout(&distant.id).ok);
        let events = state
            .advance(u64::from(distant.hop_distance_from_core) * TRAVEL_MS_PER_HOP + SURVEY_MS);
        assert!(events.contains(&GameAdvanceEvent::AlphaMilestoneCompleted));
        let snapshot = state.planet_snapshot().unwrap();
        assert!(snapshot.research.unlocked);
        assert!(!snapshot.objective.complete);
        assert_eq!(snapshot.objective.title, "CHOOSE A RESEARCH DIRECTION");
    }

    #[test]
    fn research_unlocks_a_distinct_profile_and_persists() {
        let mut state = GameState::with_seed(33);
        awaken(&mut state);
        state.reach_milestone_complete = true;
        state.relay_region_id = Some(
            state
                .world
                .regions
                .iter()
                .find(|region| hop_distance(&region.position) == 2)
                .unwrap()
                .id
                .clone(),
        );
        state.analysis_units = RESEARCH_ANALYSIS_COST;

        assert!(state.start_research(ResearchProject::ExtendedTelemetry).ok);
        assert_eq!(state.analysis_units, 0);
        assert_eq!(
            state.advance(RESEARCH_DURATION_MS),
            vec![GameAdvanceEvent::ResearchCompleted]
        );
        assert!(state.activate_profile(OperationalProfile::Reach).ok);

        let snapshot = state.planet_snapshot().unwrap();
        assert!(snapshot.survey_queue.unlocked);
        assert!(snapshot
            .research
            .profiles
            .iter()
            .any(|profile| profile.profile == OperationalProfile::Reach && profile.active));
        assert!(snapshot
            .regions
            .iter()
            .any(|region| { region.hop_distance_from_core == 3 && region.within_scout_range }));

        let temp = TempDir::new().unwrap();
        let path = temp.path().join("game_state.json");
        state.save(&path).unwrap();
        let restored = GameState::load(&path).unwrap().state;
        assert!(restored
            .completed_research
            .contains(&ResearchProject::ExtendedTelemetry));
        assert_eq!(restored.operational_profile, OperationalProfile::Reach);
    }

    #[test]
    fn earned_survey_automation_returns_and_completes_the_slice() {
        let mut state = GameState::with_seed(35);
        awaken(&mut state);
        state.reach_milestone_complete = true;
        state
            .completed_research
            .push(ResearchProject::ExtendedTelemetry);
        assert!(state.activate_profile(OperationalProfile::Reach).ok);
        let target = state
            .planet_snapshot()
            .unwrap()
            .regions
            .into_iter()
            .find(|region| {
                region.knowledge == RegionKnowledge::Unknown && region.hop_distance_from_core == 1
            })
            .unwrap();

        assert!(state.queue_survey(&target.id).ok);
        let events = state
            .advance(u64::from(target.hop_distance_from_core) * TRAVEL_MS_PER_HOP * 2 + 20_000);
        assert!(events.contains(&GameAdvanceEvent::SurveyCompleted));
        assert!(events.contains(&GameAdvanceEvent::ScoutReturned));
        assert!(events.contains(&GameAdvanceEvent::AlphaSliceCompleted));
        let snapshot = state.planet_snapshot().unwrap();
        assert_eq!(snapshot.scout.state, ScoutState::Ready);
        assert!(snapshot.objective.complete);
    }

    #[test]
    fn extraction_automation_requires_a_proven_route() {
        let mut state = GameState::with_seed(37);
        awaken(&mut state);
        state.reach_milestone_complete = true;
        state
            .completed_research
            .push(ResearchProject::FieldHardening);
        assert!(state.activate_profile(OperationalProfile::Resilience).ok);
        let target = state
            .planet_snapshot()
            .unwrap()
            .regions
            .into_iter()
            .find(|region| region.hop_distance_from_core == 1)
            .unwrap();

        assert_eq!(
            state.set_extraction_route(&target.id).code.as_deref(),
            Some("route_not_proven")
        );
        assert!(state.deploy_scout(&target.id).ok);
        state.advance(TRAVEL_MS_PER_HOP + SURVEY_MS);
        assert!(state.recall_scout().ok);
        state.advance(TRAVEL_MS_PER_HOP);
        assert!(state.deploy_extractor(&target.id).ok);
        state.active_extraction.as_mut().unwrap().will_fail = false;
        state.advance(TRAVEL_MS_PER_HOP + EXTRACTION_MS + TRAVEL_MS_PER_HOP);
        assert!(state.set_extraction_route(&target.id).ok);
        state.advance(0);
        let mission = state.active_extraction.as_mut().unwrap();
        assert!(mission.automated);
        mission.will_fail = false;
        let events = state.advance(TRAVEL_MS_PER_HOP + EXTRACTION_MS + TRAVEL_MS_PER_HOP);
        assert!(events.contains(&GameAdvanceEvent::ExtractionCompleted));
        assert!(events.contains(&GameAdvanceEvent::AlphaSliceCompleted));
    }

    #[test]
    fn a_fresh_world_can_complete_the_alpha_arc_without_low_value_grinding() {
        let mut state = GameState::with_seed(51);
        awaken(&mut state);
        let mut elapsed_ms = 0_u64;
        let mut sources = [None, None, None];
        let reachable = state
            .planet_snapshot()
            .unwrap()
            .regions
            .into_iter()
            .filter(|region| region.within_scout_range && !region.core)
            .map(|region| region.id)
            .collect::<Vec<_>>();

        for region_id in reachable {
            let hops = state
                .planet_snapshot()
                .unwrap()
                .regions
                .iter()
                .find(|region| region.id == region_id)
                .unwrap()
                .hop_distance_from_core;
            assert!(state.deploy_scout(&region_id).ok);
            state.advance(u64::from(hops) * TRAVEL_MS_PER_HOP);
            state.advance(SURVEY_MS);
            assert!(state.recall_scout().ok);
            state.advance(u64::from(hops) * TRAVEL_MS_PER_HOP);
            elapsed_ms += u64::from(hops) * TRAVEL_MS_PER_HOP * 2 + SURVEY_MS;
            let region = state
                .planet_snapshot()
                .unwrap()
                .regions
                .into_iter()
                .find(|region| region.id == region_id)
                .unwrap();
            if let Some(deposit) = region.deposit {
                let index = match deposit.resource {
                    ResourceKind::Structural => 0,
                    ResourceKind::Conductive => 1,
                    ResourceKind::Silicate => 2,
                };
                sources[index].get_or_insert(region.id);
            }
            if sources.iter().all(Option::is_some) && state.analysis_units >= 3 {
                break;
            }
        }
        assert!(sources.iter().all(Option::is_some));

        for (index, required) in [8_u32, 6, 6].into_iter().enumerate() {
            let region_id = sources[index].as_ref().unwrap();
            let mut attempts = 0;
            while match index {
                0 => state.refined_inventory.structural,
                1 => state.refined_inventory.conductive,
                _ => state.refined_inventory.silicate,
            } < required
            {
                attempts += 1;
                assert!(attempts < 12, "bounded risk should not stall the alpha arc");
                let hops = state
                    .planet_snapshot()
                    .unwrap()
                    .regions
                    .iter()
                    .find(|region| &region.id == region_id)
                    .unwrap()
                    .hop_distance_from_core;
                assert!(state.deploy_extractor(region_id).ok);
                state.advance(u64::from(hops) * TRAVEL_MS_PER_HOP);
                let events = state.advance(EXTRACTION_MS);
                elapsed_ms += u64::from(hops) * TRAVEL_MS_PER_HOP + EXTRACTION_MS;
                if events.contains(&GameAdvanceEvent::ExtractorLost) {
                    state.advance(EXTRACTOR_REBUILD_MS);
                    elapsed_ms += EXTRACTOR_REBUILD_MS;
                    continue;
                }
                state.advance(u64::from(hops) * TRAVEL_MS_PER_HOP);
                elapsed_ms += u64::from(hops) * TRAVEL_MS_PER_HOP;
                state.advance(REFINING_MS_PER_UNIT * u64::from(EXTRACTION_CAPACITY));
                elapsed_ms += REFINING_MS_PER_UNIT * u64::from(EXTRACTION_CAPACITY);
            }
        }
        assert!(stock_contains(state.refined_inventory, RELAY_COST));

        let relay_site = state
            .planet_snapshot()
            .unwrap()
            .regions
            .into_iter()
            .find(|region| {
                region.hop_distance_from_core == 2 && region.knowledge >= RegionKnowledge::Surveyed
            })
            .unwrap();
        assert!(state.construct_relay(&relay_site.id).ok);
        state.advance(RELAY_CONSTRUCTION_MS);
        elapsed_ms += RELAY_CONSTRUCTION_MS;

        let distant = state
            .planet_snapshot()
            .unwrap()
            .regions
            .into_iter()
            .find(|region| {
                region.hop_distance_from_core > SCOUT_RANGE_HOPS
                    && region.within_scout_range
                    && region.knowledge < RegionKnowledge::Surveyed
            })
            .unwrap();
        let travel_ms = u64::from(distant.hop_distance_from_core) * TRAVEL_MS_PER_HOP;
        assert!(state.deploy_scout(&distant.id).ok);
        state.advance(travel_ms);
        let events = state.advance(SURVEY_MS);
        assert!(events.contains(&GameAdvanceEvent::AlphaMilestoneCompleted));
        assert!(state.recall_scout().ok);
        state.advance(travel_ms);
        elapsed_ms += travel_ms * 2 + SURVEY_MS;

        assert!(state.start_research(ResearchProject::ExtendedTelemetry).ok);
        state.advance(RESEARCH_DURATION_MS);
        elapsed_ms += RESEARCH_DURATION_MS;
        assert!(state.activate_profile(OperationalProfile::Reach).ok);
        let automated_target = state
            .planet_snapshot()
            .unwrap()
            .regions
            .into_iter()
            .find(|region| {
                region.within_scout_range
                    && !region.core
                    && region.knowledge < RegionKnowledge::Surveyed
            })
            .unwrap();
        assert!(state.queue_survey(&automated_target.id).ok);
        let travel_ms = u64::from(automated_target.hop_distance_from_core) * TRAVEL_MS_PER_HOP;
        state.advance(travel_ms);
        state.advance(20_000);
        let events = state.advance(travel_ms);
        elapsed_ms += travel_ms * 2 + 20_000;

        assert!(events.contains(&GameAdvanceEvent::AlphaSliceCompleted));
        assert!(state.planet_snapshot().unwrap().objective.complete);
        assert!(elapsed_ms <= 45 * 60 * 1_000, "elapsed {elapsed_ms}ms");
    }

    #[test]
    fn reviewed_activity_is_not_returned_again() {
        let mut state = GameState::with_seed(39);
        awaken(&mut state);
        state.push_activity(
            ActivityKind::Survey,
            Some("region_082".into()),
            "Survey completed at R-082.".into(),
        );
        let snapshot = state.planet_snapshot().unwrap();
        let latest = snapshot.unreviewed_activity.last().unwrap().activity_id;
        assert!(state.acknowledge_activity(latest).ok);
        assert!(state
            .planet_snapshot()
            .unwrap()
            .unreviewed_activity
            .is_empty());

        let temp = TempDir::new().unwrap();
        let path = temp.path().join("game_state.json");
        state.save(&path).unwrap();
        assert!(GameState::load(&path)
            .unwrap()
            .state
            .planet_snapshot()
            .unwrap()
            .unreviewed_activity
            .is_empty());
    }
}
