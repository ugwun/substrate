use crate::protocol::{
    ActiveConstructionSnapshot, ActiveExtractionSnapshot, ActiveMissionSnapshot,
    ActiveResearchSnapshot, ActivityKind, ActivityRecord, ArchiveExperimentSnapshot,
    ArchiveRecordSnapshot, ArchiveSnapshot, AtmosphericSeverity, BaseModuleKind,
    BaseModuleSnapshot, BaseProjectSnapshot, BaseSnapshot, DepositSnapshot,
    ExtractionProbeSnapshot, ExtractionRouteSnapshot, ExtractorLoadout, ExtractorState,
    HorizonProgressSnapshot, MissionReport, MissionReportOutcome, ObjectiveSnapshot,
    OnboardingCompletionResult, OnboardingSnapshot, OperationalProfile, OperationalProfileSnapshot,
    PlanetActionResult, PlanetSnapshot, PlanetaryNetworkSnapshot, RegionKnowledge, RegionPosition,
    RegionSnapshot, ResearchOptionSnapshot, ResearchProject, ResearchSnapshot, ResourceKind,
    ResourceStock, RiskLevel, ScoutSnapshot, ScoutState, SiteSurveySnapshot, SubregionClass,
    SubregionKnowledge, SubregionSnapshot, SurveyConfidence, SurveyQueueSnapshot,
    SystemMapSnapshot, TemperatureBand, TerrainClass,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fs, io, io::Write, path::Path};
use tempfile::NamedTempFile;
use thiserror::Error;
use uuid::Uuid;

const SAVE_SCHEMA_VERSION: u32 = 11;
const MIGRATABLE_SCHEMA_VERSIONS: [u32; 6] = [5, 6, 7, 8, 9, 10];
const RESETTABLE_SCHEMA_VERSIONS: [u32; 4] = [1, 2, 3, 4];
const WORLD_GENERATION_VERSION: u32 = 4;
const LATITUDE_BANDS: u8 = 10;
const LONGITUDE_SECTORS: u8 = 20;
const REGION_COUNT: usize = (LATITUDE_BANDS as usize) * (LONGITUDE_SECTORS as usize);
const CORE_LATITUDE_BAND: u8 = 4;
const CORE_LONGITUDE_SECTOR: u8 = 0;
const SCOUT_RANGE_HOPS: u8 = 2;
const TRAVEL_MS_PER_HOP: u64 = 15_000;
const SURVEY_MS: u64 = 20_000;
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
const RESEARCH_DURATION_MS: u64 = 75_000;
const PRE_AUTOMATION_WAYPOINT_CAPACITY: usize = 1;
const SURVEY_QUEUE_CAPACITY: usize = 8;
const ACTIVITY_CAPACITY: usize = 64;
const SUBREGION_ROWS: u8 = 4;
const SUBREGION_COLUMNS: u8 = 4;
const SUBREGIONS_PER_REGION: u8 = SUBREGION_ROWS * SUBREGION_COLUMNS;
const SITE_SURVEY_MS: u64 = 25_000;
const BASE_PROJECT_CAPACITY: usize = 2;
const MAX_DEPOT_PROJECT_BONUS: usize = 2;
const BASE_COMMUNICATION_RANGE_HOPS: u8 = 3;
const BASE_ESTABLISH_MS: u64 = 150_000;
const BASE_MODULE_MS: u64 = 120_000;
const BASE_ESTABLISH_COST: ResourceStock = ResourceStock {
    structural: 10,
    conductive: 6,
    silicate: 4,
};
const BASE_MAINTENANCE_COST: ResourceStock = ResourceStock {
    structural: 4,
    conductive: 2,
    silicate: 0,
};
const BASE_MAINTENANCE_MS: u64 = 60_000;
const HORIZON_REQUIRED_REGIONS: usize = 18;
const HORIZON_REQUIRED_SUBREGIONS: usize = 12;
const HORIZON_REQUIRED_BASES: usize = 3;
const HORIZON_REQUIRED_REMOTE_BASES: usize = 1;
const HORIZON_REQUIRED_OBSERVATORIES: usize = 2;
const HORIZON_REQUIRED_RESEARCH: usize = 2;
const ARCHIVE_EXPERIMENT_MS: u64 = 75_000;
const ARCHIVE_EXPERIMENT_ANALYSIS: u32 = 2;

const OBSERVATION_001: &str = "observation_001";
const OBSERVATION_002: &str = "observation_002";
const OBSERVATION_003: &str = "observation_003";
const OBSERVATION_004: &str = "observation_004";
const OBSERVATION_005: &str = "observation_005";

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
    extractor_loadout: ExtractorLoadout,
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
    #[serde(default)]
    horizon_reached: bool,
    #[serde(default)]
    active_site_survey: Option<SiteSurveyState>,
    #[serde(default)]
    bases: Vec<BaseState>,
    #[serde(default)]
    base_projects: Vec<BaseProjectState>,
    #[serde(default = "next_identifier_default")]
    next_base_id: u64,
    #[serde(default = "next_identifier_default")]
    next_project_id: u64,
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
    archive_records: Vec<ArchiveRecordState>,
    #[serde(default)]
    presented_archive_count: usize,
    #[serde(default)]
    active_archive_experiment: Option<ArchiveExperimentState>,
    #[serde(default)]
    archive_comparison_region_ids: Vec<String>,
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
    #[serde(default)]
    subregions: Vec<SubregionState>,
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
    #[serde(default)]
    target_subregion_id: Option<String>,
    phase: ExtractorState,
    remaining_ms: u64,
    phase_total_ms: u64,
    travel_ms: u64,
    risk: RiskLevel,
    will_fail: bool,
    #[serde(default)]
    cargo_capacity: u32,
    #[serde(default)]
    extraction_duration_ms: u64,
    #[serde(default)]
    rebuild_duration_ms: u64,
    #[serde(default)]
    loadout: ExtractorLoadout,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct SubregionState {
    id: String,
    designation: String,
    slot: u8,
    #[serde(default)]
    row: u8,
    #[serde(default)]
    column: u8,
    class: SubregionClass,
    knowledge: SubregionKnowledge,
    hazard: RiskLevel,
    energy_yield: u32,
    deposit: Option<DepositState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct SiteSurveyState {
    region_id: String,
    subregion_id: String,
    remaining_ms: u64,
    phase_total_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct BaseState {
    id: String,
    designation: String,
    region_id: String,
    subregion_id: String,
    integrity: u32,
    stored_energy: u32,
    energy_capacity: u32,
    energy_remainder_ms: u64,
    #[serde(default)]
    wear_remainder_ms: u64,
    modules: Vec<BaseModuleKind>,
    #[serde(default)]
    disabled_modules: Vec<BaseModuleKind>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct BaseProjectState {
    project_id: u64,
    region_id: String,
    subregion_id: String,
    module: Option<BaseModuleKind>,
    #[serde(default)]
    maintenance: bool,
    #[serde(default)]
    logistics_supported: bool,
    remaining_ms: u64,
    phase_total_ms: u64,
    cost: ResourceStock,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ArchiveRecordState {
    record_id: String,
    title: String,
    medium: String,
    observation: String,
    interpretation: String,
    region_id: Option<String>,
    discovered_at_revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ArchiveExperimentState {
    region_id: String,
    subregion_id: String,
    remaining_ms: u64,
    phase_total_ms: u64,
}

fn extractor_available_default() -> bool {
    true
}

fn next_identifier_default() -> u64 {
    1
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
    PreviousMigrated,
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
    FoundationCompleted,
    SubregionSurveyed,
    BaseEstablished,
    BaseModuleCompleted,
    BaseMaintained,
    BaseStrained,
    SolarHorizonReached,
    ArchiveExperimentCompleted,
    ArchiveObservationRecorded,
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
            extractor_loadout: ExtractorLoadout::Balanced,
            extractor_rebuild_remaining_ms: 0,
            raw_inventory: ResourceStock::default(),
            refined_inventory: ResourceStock::default(),
            refining_remaining_ms: 0,
            active_construction: None,
            relay_region_id: None,
            reach_milestone_complete: false,
            alpha_complete: false,
            horizon_reached: false,
            active_site_survey: None,
            bases: Vec::new(),
            base_projects: Vec::new(),
            next_base_id: 1,
            next_project_id: 1,
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
            archive_records: Vec::new(),
            presented_archive_count: 0,
            active_archive_experiment: None,
            archive_comparison_region_ids: Vec::new(),
            random_state: seed ^ 0xA17A_5EED_D15C_0FFE,
            dirty: true,
            generation: 0,
        }
    }

    pub(crate) fn new_playthrough_after(previous_revision: u64) -> Self {
        Self {
            state_revision: previous_revision.saturating_add(1),
            ..Self::default()
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
        if !MIGRATABLE_SCHEMA_VERSIONS.contains(&version.schema_version)
            && version.schema_version != SAVE_SCHEMA_VERSION
        {
            return Err(GameError::UnsupportedVersion(version.schema_version));
        }
        let envelope: SaveEnvelope = serde_json::from_value(value)?;
        let mut state = envelope.state;
        let migrated = version.schema_version != SAVE_SCHEMA_VERSION;
        state.ensure_subregions();
        if version.schema_version <= 7 {
            state.migrate_extraction_mission();
        }
        if migrated {
            state.migrate_archive_progress(version.schema_version);
            if version.schema_version <= 10 {
                state.presented_archive_count = state.archive_records.len();
            }
        }
        state.validate()?;
        if migrated {
            state.save(path)?;
        }
        state.dirty = false;
        state.generation = 0;
        Ok(LoadedGame {
            state,
            disposition: if migrated {
                LoadDisposition::PreviousMigrated
            } else {
                LoadDisposition::Current
            },
        })
    }

    fn ensure_subregions(&mut self) {
        for (index, region) in self.world.regions.iter_mut().enumerate() {
            if region.subregions.len() == usize::from(SUBREGIONS_PER_REGION)
                && region
                    .subregions
                    .iter()
                    .all(|site| site.row < SUBREGION_ROWS && site.column < SUBREGION_COLUMNS)
            {
                continue;
            }
            let resource = region.deposit.as_ref().map_or_else(
                || match index % 3 {
                    0 => ResourceKind::Structural,
                    1 => ResourceKind::Conductive,
                    _ => ResourceKind::Silicate,
                },
                |deposit| deposit.resource,
            );
            let mut expanded = generate_subregions(
                self.world.seed,
                index,
                region.terrain,
                region.position.latitude_band,
                resource,
                region.knowledge >= RegionKnowledge::Surveyed,
            );
            for mut existing in std::mem::take(&mut region.subregions) {
                if existing.slot >= SUBREGIONS_PER_REGION {
                    continue;
                }
                let (row, column) = subregion_coordinates(existing.slot);
                existing.row = row;
                existing.column = column;
                if let Some(generated) = expanded
                    .iter_mut()
                    .find(|generated| generated.slot == existing.slot)
                {
                    *generated = existing;
                }
            }
            region.subregions = expanded;
        }
        self.world.generation_version = WORLD_GENERATION_VERSION;
    }

    fn migrate_extraction_mission(&mut self) {
        let Some(region_id) = self
            .active_extraction
            .as_ref()
            .map(|mission| mission.target_region_id.clone())
        else {
            return;
        };
        let capacity = self.extraction_capacity_for(&region_id);
        if let Some(mission) = self.active_extraction.as_mut() {
            mission.cargo_capacity = capacity;
            mission.extraction_duration_ms = EXTRACTION_MS;
            mission.rebuild_duration_ms = EXTRACTOR_REBUILD_MS;
            mission.loadout = ExtractorLoadout::Balanced;
        }
    }

    fn migrate_archive_progress(&mut self, previous_schema: u32) {
        if self.onboarding_complete {
            self.record_archive_observation(
                OBSERVATION_001,
                Some(self.world.core_region_id.clone()),
            );
        }
        self.refresh_initial_archive_observation();

        if previous_schema == 8 && self.horizon_reached {
            if !self.has_archive_observation(OBSERVATION_002) {
                if let Some((region_id, subregion_id)) = self.first_sheltered_site() {
                    if let Some(site) = self.find_subregion_mut(&subregion_id) {
                        site.knowledge = SubregionKnowledge::Observed;
                    }
                    self.record_archive_observation(OBSERVATION_002, Some(region_id));
                    self.raise_comparable_site_confidence();
                }
            }
            if !self.has_archive_observation(OBSERVATION_003) {
                if let Some((region_id, _)) = self.operational_laboratory() {
                    self.analysis_units = self
                        .analysis_units
                        .saturating_add(ARCHIVE_EXPERIMENT_ANALYSIS);
                    self.record_archive_observation(OBSERVATION_003, Some(region_id));
                }
            }
            if self.has_archive_observation(OBSERVATION_003)
                && !self.has_archive_observation(OBSERVATION_004)
            {
                self.archive_comparison_region_ids = self
                    .world
                    .regions
                    .iter()
                    .filter(|region| {
                        region
                            .subregions
                            .iter()
                            .any(|site| site.knowledge >= SubregionKnowledge::Surveyed)
                    })
                    .take(3)
                    .map(|region| region.id.clone())
                    .collect();
                if self.archive_comparison_region_ids.len() >= 3 {
                    self.record_archive_observation(OBSERVATION_004, None);
                }
            }
            if self.has_archive_observation(OBSERVATION_004) {
                self.record_archive_observation(OBSERVATION_005, None);
            }
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), GameError> {
        #[derive(Serialize)]
        struct SaveEnvelopeRef<'a> {
            schema_version: u32,
            saved_at: DateTime<Utc>,
            state: &'a GameState,
        }
        let parent = path.parent().ok_or(GameError::MissingParent)?;
        let mut temporary = NamedTempFile::new_in(parent)?;
        {
            let mut writer = std::io::BufWriter::new(&mut temporary);
            serde_json::to_writer_pretty(
                &mut writer,
                &SaveEnvelopeRef {
                    schema_version: SAVE_SCHEMA_VERSION,
                    saved_at: Utc::now(),
                    state: self,
                },
            )?;
            writer.write_all(b"\n")?;
            writer.flush()?;
        }
        temporary.as_file_mut().sync_all()?;
        temporary
            .persist(path)
            .map_err(|error| GameError::Io(error.error))?;
        Ok(())
    }

    pub fn planet_revision(&self) -> Option<u64> {
        self.onboarding_complete.then_some(self.state_revision)
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
            self.record_archive_observation(
                OBSERVATION_001,
                Some(self.world.core_region_id.clone()),
            );
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
            let regions: Vec<RegionSnapshot> = self
                .world
                .regions
                .iter()
                .map(|region| self.region_snapshot(region))
                .collect();
            let network = self.network_snapshot();
            let archive_experiment = self.archive_experiment_snapshot();
            let observed_subregions = self
                .world
                .regions
                .iter()
                .flat_map(|region| &region.subregions)
                .filter(|site| site.knowledge == SubregionKnowledge::Observed)
                .count();
            let has_observed_sheltered = self.world.regions.iter().any(|region| {
                region.subregions.iter().any(|site| {
                    site.class == SubregionClass::ShelteredShelf
                        && site.knowledge == SubregionKnowledge::Observed
                })
            });
            let offline_base = network
                .bases
                .iter()
                .find(|base| !base.connected || base.integrity <= 20);
            let offline_base_region_id = offline_base.map(|base| base.region_id.as_str());
            let offline_base_subregion_id = offline_base.map(|base| base.subregion_id.as_str());
            let campaign = crate::campaign::derive_campaign(&crate::campaign::CampaignInput {
                has_observation_002: self.has_archive_observation(OBSERVATION_002),
                has_observation_003: self.has_archive_observation(OBSERVATION_003),
                has_observation_004: self.has_archive_observation(OBSERVATION_004),
                has_observation_005: self.has_archive_observation(OBSERVATION_005),
                horizon_reached: self.horizon_reached,
                relay_built: self.relay_region_id.is_some(),
                relay_constructing: self.active_construction.is_some(),
                research_unlocked: self.reach_milestone_complete,
                active_research: self.active_research.is_some(),
                completed_research_count: self.completed_research.len(),
                profile: self.operational_profile,
                foundation_complete: self.alpha_complete,
                analysis_units: self.analysis_units,
                refined_inventory: self.refined_inventory,
                scout_state: phase,
                scout_region_id: &region_id,
                scout_target_region_id: self
                    .active_mission
                    .as_ref()
                    .map(|mission| mission.target_region_id.as_str()),
                queued_survey_region_id: self.survey_queue.first().map(String::as_str),
                extractor_ready: self.extractor_available
                    && self.active_extraction.is_none()
                    && self.extractor_rebuild_remaining_ms == 0,
                active_site_survey: self.active_site_survey.is_some(),
                active_base_projects: self.base_projects.len(),
                observed_subregions,
                has_observed_sheltered,
                comparison_regions_count: self.archive_comparison_region_ids.len(),
                comparison_region_ids: &self.archive_comparison_region_ids,
                archive_experiment: archive_experiment.as_ref(),
                horizon: &network.horizon,
                regions: &regions,
                subregions: &network.subregions,
                offline_base_region_id,
                offline_base_subregion_id,
                bases: &network.bases,
                projects: &network.active_projects,
            });
            let unpresented_record_ids = self
                .archive_records
                .iter()
                .skip(self.presented_archive_count)
                .map(|record| record.record_id.clone())
                .collect();
            PlanetSnapshot {
                revision: self.state_revision,
                latitude_bands: LATITUDE_BANDS,
                longitude_sectors: LONGITUDE_SECTORS,
                core_region_id: self.world.core_region_id.clone(),
                regions,
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
                    loadout: self.extractor_loadout,
                },
                active_extraction: self.active_extraction.as_ref().map(|mission| {
                    ActiveExtractionSnapshot {
                        mission_id: mission.mission_id,
                        target_region_id: mission.target_region_id.clone(),
                        target_subregion_id: mission.target_subregion_id.clone(),
                        phase: mission.phase,
                        remaining_ms: mission.remaining_ms,
                        phase_total_ms: mission.phase_total_ms,
                        risk: mission.risk,
                        cargo_capacity: mission.cargo_capacity,
                        loadout: mission.loadout,
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
                    capacity: self.survey_queue_capacity(),
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
                campaign,
                archive: ArchiveSnapshot {
                    records: self
                        .archive_records
                        .iter()
                        .map(|record| ArchiveRecordSnapshot {
                            record_id: record.record_id.clone(),
                            title: record.title.clone(),
                            medium: record.medium.clone(),
                            observation: record.observation.clone(),
                            interpretation: record.interpretation.clone(),
                            region_id: record.region_id.clone(),
                            discovered_at_revision: record.discovered_at_revision,
                        })
                        .collect(),
                    unpresented_record_ids,
                    experiment: archive_experiment,
                },
                system_map: SystemMapSnapshot {
                    available: self.horizon_reached
                        && self.has_archive_observation(OBSERVATION_005),
                },
                network,
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
        if !self.region_within_range(&region.position) {
            return self.action_rejected("region_out_of_range");
        }
        let target_position = region.position.clone();
        let survey_required = region.knowledge < RegionKnowledge::Surveyed;
        let survey_completed = region.knowledge >= RegionKnowledge::Surveyed;
        let network_hops = self.travel_hops_to_network(&target_position).max(1);
        let travel_ms = u64::from(network_hops) * TRAVEL_MS_PER_HOP;
        let mut outbound_ms = travel_ms;

        if let Some(current) = &self.active_mission {
            if current.phase != ScoutState::Holding || current.target_region_id == region_id {
                return self.action_rejected("scout_unavailable");
            }
            if self
                .active_site_survey
                .as_ref()
                .is_some_and(|survey| survey.region_id == current.target_region_id)
            {
                return self.action_rejected("site_survey_in_progress");
            }
            let previous_region_id = current.target_region_id.clone();
            if let Some(previous_region) = self
                .world
                .regions
                .iter_mut()
                .find(|candidate| candidate.id == previous_region_id)
            {
                let direct_hops = graph_distance(&previous_region.position, &target_position)
                    .min(network_hops)
                    .max(1);
                outbound_ms = u64::from(direct_hops) * TRAVEL_MS_PER_HOP;
                if previous_region.knowledge == RegionKnowledge::Observed {
                    previous_region.knowledge = RegionKnowledge::Surveyed;
                }
            }
        }
        let mission_id = self.next_mission_id;
        self.next_mission_id += 1;
        self.active_mission = Some(MissionState {
            mission_id,
            target_region_id: region_id.to_owned(),
            phase: ScoutState::Outbound,
            remaining_ms: outbound_ms,
            phase_total_ms: outbound_ms,
            travel_ms,
            survey_required,
            survey_completed,
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
        if self
            .active_site_survey
            .as_ref()
            .is_some_and(|survey| survey.region_id == mission.target_region_id)
        {
            self.active_mission = Some(mission);
            return self.action_rejected("site_survey_in_progress");
        }
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
        self.deploy_extractor_at(region_id, None)
    }

    pub fn configure_extractor(&mut self, loadout: ExtractorLoadout) -> PlanetActionResult {
        if !self.onboarding_complete {
            return self.action_rejected("onboarding_required");
        }
        if !self.extractor_available || self.active_extraction.is_some() {
            return self.action_rejected("extractor_unavailable");
        }
        if self.extractor_loadout == loadout {
            return self.action_rejected("loadout_already_configured");
        }
        self.extractor_loadout = loadout;
        self.changed();
        self.action_accepted(None)
    }

    pub fn deploy_extractor_at(
        &mut self,
        region_id: &str,
        subregion_id: Option<&str>,
    ) -> PlanetActionResult {
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
        let regional_risk = self.effective_risk(region.atmospheric_severity);
        let risk = if let Some(subregion_id) = subregion_id {
            if !self.alpha_complete {
                return self.action_rejected("planetary_network_locked");
            }
            let Some(subregion) = region
                .subregions
                .iter()
                .find(|subregion| subregion.id == subregion_id)
            else {
                return self.action_rejected("unknown_subregion");
            };
            if subregion.knowledge < SubregionKnowledge::Surveyed {
                return self.action_rejected("subregion_survey_required");
            }
            let Some(deposit) = subregion.deposit.as_ref() else {
                return self.action_rejected("no_deposit");
            };
            if deposit.remaining == 0 {
                return self.action_rejected("deposit_depleted");
            }
            self.effective_site_risk(subregion.hazard)
        } else {
            let Some(deposit) = region.deposit.as_ref() else {
                return self.action_rejected("no_deposit");
            };
            if deposit.remaining == 0 {
                return self.action_rejected("deposit_depleted");
            }
            regional_risk
        };
        if !self.extractor_available || self.active_extraction.is_some() {
            return self.action_rejected("extractor_unavailable");
        }
        let loadout = self.extractor_loadout;
        let travel_ms = extraction_travel_ms(
            self.travel_hops_to_network(&region.position).max(1),
            loadout,
        );
        let risk = if self.region_has_active_module(region_id, BaseModuleKind::Workshop) {
            lower_risk(risk)
        } else {
            risk
        };
        let risk = loadout_risk(risk, loadout);
        let cargo_capacity = loadout_capacity(self.extraction_capacity_for(region_id), loadout);
        let will_fail = self.next_random_percent() < risk_percent(risk);
        let mission_id = self.next_mission_id;
        self.next_mission_id += 1;
        self.active_extraction = Some(ExtractionMissionState {
            mission_id,
            target_region_id: region_id.to_owned(),
            target_subregion_id: subregion_id.map(str::to_owned),
            phase: ExtractorState::Outbound,
            remaining_ms: travel_ms,
            phase_total_ms: travel_ms,
            travel_ms,
            risk,
            will_fail,
            cargo_capacity,
            extraction_duration_ms: loadout.extraction_duration_ms(),
            rebuild_duration_ms: loadout.rebuild_duration_ms(),
            loadout,
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

    pub fn survey_subregion(&mut self, subregion_id: &str) -> PlanetActionResult {
        if !self.onboarding_complete {
            return self.action_rejected("onboarding_required");
        }
        if self.active_site_survey.is_some() {
            return self.action_rejected("site_survey_in_progress");
        }
        let Some((region, subregion)) = self.find_subregion(subregion_id) else {
            return self.action_rejected("unknown_subregion");
        };
        if subregion.knowledge == SubregionKnowledge::Unknown {
            return self.action_rejected("regional_survey_required");
        }
        if subregion.knowledge >= SubregionKnowledge::Surveyed {
            return self.action_rejected("subregion_already_surveyed");
        }
        let Some(mission) = &self.active_mission else {
            return self.action_rejected("scout_presence_required");
        };
        if mission.phase != ScoutState::Holding || mission.target_region_id != region.id {
            return self.action_rejected("scout_presence_required");
        }
        let region_id = region.id.clone();
        let designation = subregion.designation.clone();
        let mission_id = mission.mission_id;
        let duration_ms = if self.operational_profile == OperationalProfile::Reach {
            18_000
        } else {
            SITE_SURVEY_MS
        };
        self.active_site_survey = Some(SiteSurveyState {
            region_id: region_id.clone(),
            subregion_id: subregion_id.to_owned(),
            remaining_ms: duration_ms,
            phase_total_ms: duration_ms,
        });
        self.push_activity(
            ActivityKind::Survey,
            Some(region_id),
            format!("Focused local survey started at {designation}."),
        );
        self.changed();
        self.action_accepted(Some(mission_id))
    }

    pub fn establish_base(&mut self, subregion_id: &str) -> PlanetActionResult {
        if !self.alpha_complete {
            return self.action_rejected("planetary_network_locked");
        }
        let Some((region, subregion)) = self.find_subregion(subregion_id) else {
            return self.action_rejected("unknown_subregion");
        };
        if subregion.knowledge < SubregionKnowledge::Surveyed {
            return self.action_rejected("subregion_survey_required");
        }
        if self.bases.iter().any(|base| base.region_id == region.id) {
            return self.action_rejected("regional_base_exists");
        }
        if !self.region_within_range(&region.position) {
            return self.action_rejected("region_out_of_range");
        }
        if self.project_conflicts(&region.id, subregion_id) {
            return self.action_rejected("local_project_active");
        }
        if self.base_projects.len() >= self.base_project_capacity() {
            return self.action_rejected("project_capacity_reached");
        }
        if !stock_contains(self.refined_inventory, BASE_ESTABLISH_COST) {
            return self.action_rejected("insufficient_materials");
        }
        let region_id = region.id.clone();
        let designation = subregion.designation.clone();
        let hazard = subregion.hazard;
        subtract_stock(&mut self.refined_inventory, BASE_ESTABLISH_COST);
        let project_id = self.next_project_id;
        self.next_project_id = self.next_project_id.saturating_add(1);
        let logistics_supported = self.logistics_support_hops(&region_id).is_some();
        let duration_ms = project_duration(BASE_ESTABLISH_MS, hazard, logistics_supported);
        self.base_projects.push(BaseProjectState {
            project_id,
            region_id: region_id.clone(),
            subregion_id: subregion_id.to_owned(),
            module: None,
            maintenance: false,
            logistics_supported,
            remaining_ms: duration_ms,
            phase_total_ms: duration_ms,
            cost: BASE_ESTABLISH_COST,
        });
        self.push_activity(
            ActivityKind::Construction,
            Some(region_id),
            format!("Foothold project started at {designation}."),
        );
        self.changed();
        self.action_accepted(Some(project_id))
    }

    pub fn construct_base_module(
        &mut self,
        subregion_id: &str,
        module: BaseModuleKind,
    ) -> PlanetActionResult {
        let Some(base) = self
            .bases
            .iter()
            .find(|base| base.subregion_id == subregion_id)
        else {
            return self.action_rejected("base_required");
        };
        if base.modules.contains(&module) {
            return self.action_rejected("module_already_constructed");
        }
        if base.integrity <= 20 {
            return self.action_rejected("base_maintenance_required");
        }
        if self.project_conflicts(&base.region_id, subregion_id) {
            return self.action_rejected("local_project_active");
        }
        if self.base_projects.len() >= self.base_project_capacity() {
            return self.action_rejected("project_capacity_reached");
        }
        if module != BaseModuleKind::PowerArray
            && !base.modules.contains(&BaseModuleKind::PowerArray)
        {
            return self.action_rejected("power_array_required");
        }
        if matches!(
            module,
            BaseModuleKind::Workshop | BaseModuleKind::Laboratory | BaseModuleKind::Observatory
        ) && !base.modules.contains(&BaseModuleKind::Communications)
        {
            return self.action_rejected("communications_required");
        }
        if module == BaseModuleKind::Observatory
            && (!self.region_has_active_module_anywhere(BaseModuleKind::Laboratory)
                || self.completed_research.len() < HORIZON_REQUIRED_RESEARCH
                || self.horizon_progress().surveyed_subregions < HORIZON_REQUIRED_SUBREGIONS)
        {
            return self.action_rejected("observation_prerequisites_incomplete");
        }
        let cost = base_module_cost(module);
        if !stock_contains(self.refined_inventory, cost) {
            return self.action_rejected("insufficient_materials");
        }
        let region_id = base.region_id.clone();
        let base_designation = base.designation.clone();
        subtract_stock(&mut self.refined_inventory, cost);
        let project_id = self.next_project_id;
        self.next_project_id = self.next_project_id.saturating_add(1);
        let hazard = self
            .find_subregion(subregion_id)
            .map_or(RiskLevel::Moderate, |(_, subregion)| subregion.hazard);
        let logistics_supported = self.logistics_support_hops(&region_id).is_some();
        let duration_ms = project_duration(BASE_MODULE_MS, hazard, logistics_supported);
        self.base_projects.push(BaseProjectState {
            project_id,
            region_id: region_id.clone(),
            subregion_id: subregion_id.to_owned(),
            module: Some(module),
            maintenance: false,
            logistics_supported,
            remaining_ms: duration_ms,
            phase_total_ms: duration_ms,
            cost,
        });
        self.push_activity(
            ActivityKind::Construction,
            Some(region_id),
            format!(
                "{} construction started at {base_designation}.",
                base_module_title(module)
            ),
        );
        self.changed();
        self.action_accepted(Some(project_id))
    }

    pub fn maintain_base(&mut self, subregion_id: &str) -> PlanetActionResult {
        let Some(base) = self
            .bases
            .iter()
            .find(|base| base.subregion_id == subregion_id)
        else {
            return self.action_rejected("base_required");
        };
        if base.integrity >= 100 {
            return self.action_rejected("maintenance_not_required");
        }
        if self.project_conflicts(&base.region_id, subregion_id) {
            return self.action_rejected("local_project_active");
        }
        if self.base_projects.len() >= self.base_project_capacity() {
            return self.action_rejected("project_capacity_reached");
        }
        if !stock_contains(self.refined_inventory, BASE_MAINTENANCE_COST) {
            return self.action_rejected("insufficient_materials");
        }
        let region_id = base.region_id.clone();
        let designation = base.designation.clone();
        subtract_stock(&mut self.refined_inventory, BASE_MAINTENANCE_COST);
        let project_id = self.next_project_id;
        self.next_project_id = self.next_project_id.saturating_add(1);
        self.base_projects.push(BaseProjectState {
            project_id,
            region_id: region_id.clone(),
            subregion_id: subregion_id.to_owned(),
            module: None,
            maintenance: true,
            logistics_supported: false,
            remaining_ms: BASE_MAINTENANCE_MS,
            phase_total_ms: BASE_MAINTENANCE_MS,
            cost: BASE_MAINTENANCE_COST,
        });
        self.push_activity(
            ActivityKind::Construction,
            Some(region_id),
            format!("Maintenance cycle started at {designation}."),
        );
        self.changed();
        self.action_accepted(Some(project_id))
    }

    pub fn set_base_module_enabled(
        &mut self,
        subregion_id: &str,
        module: BaseModuleKind,
        enabled: bool,
    ) -> PlanetActionResult {
        let Some(index) = self
            .bases
            .iter()
            .position(|base| base.subregion_id == subregion_id)
        else {
            return self.action_rejected("base_required");
        };
        if !self.bases[index].modules.contains(&module) {
            return self.action_rejected("module_not_constructed");
        }
        let currently_enabled = !self.bases[index].disabled_modules.contains(&module);
        if currently_enabled == enabled {
            return self.action_rejected("module_mode_unchanged");
        }
        if enabled {
            self.bases[index]
                .disabled_modules
                .retain(|disabled| *disabled != module);
        } else {
            self.bases[index].disabled_modules.push(module);
        }
        let region_id = self.bases[index].region_id.clone();
        let designation = self.bases[index].designation.clone();
        self.push_activity(
            ActivityKind::Construction,
            Some(region_id),
            format!(
                "{} {} at {designation}.",
                base_module_title(module),
                if enabled { "restored" } else { "suspended" }
            ),
        );
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

    pub fn start_archive_experiment(&mut self) -> PlanetActionResult {
        if !self.has_archive_observation(OBSERVATION_002) {
            return self.action_rejected("archive_observation_required");
        }
        if self.has_archive_observation(OBSERVATION_003) {
            return self.action_rejected("archive_experiment_completed");
        }
        if self.active_archive_experiment.is_some() {
            return self.action_rejected("archive_experiment_in_progress");
        }
        if self.completed_research.len() < HORIZON_REQUIRED_RESEARCH {
            return self.action_rejected("research_directions_required");
        }
        let Some((region_id, subregion_id)) = self.operational_laboratory() else {
            return self.action_rejected("powered_laboratory_required");
        };
        let designation = self.subregion_designation(&subregion_id);
        self.active_archive_experiment = Some(ArchiveExperimentState {
            region_id: region_id.clone(),
            subregion_id,
            remaining_ms: ARCHIVE_EXPERIMENT_MS,
            phase_total_ms: ARCHIVE_EXPERIMENT_MS,
        });
        self.push_activity(
            ActivityKind::Research,
            Some(region_id),
            format!(
                "Archive timing comparison started at {designation}; field operations may continue."
            ),
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
        if self.survey_queue.len() >= self.survey_queue_capacity() {
            return self.action_rejected("survey_queue_full");
        }
        let designation = region.designation.clone();
        self.survey_queue.push(region_id.to_owned());
        self.push_activity(
            ActivityKind::Automation,
            Some(region_id.to_owned()),
            if self.automation_unlocked() {
                format!("Automated survey queued for {designation}.")
            } else {
                format!("Next survey waypoint queued for {designation}.")
            },
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

    pub fn acknowledge_archive_presentation(&mut self, record_id: &str) -> PlanetActionResult {
        if !self.onboarding_complete {
            return self.action_rejected("onboarding_required");
        }
        let Some(index) = self
            .archive_records
            .iter()
            .position(|record| record.record_id == record_id)
        else {
            return self.action_rejected("unknown_archive_record");
        };
        if index < self.presented_archive_count {
            return self.action_accepted(None);
        }
        if index > self.presented_archive_count {
            return self.action_rejected("archive_presentation_out_of_order");
        }
        self.presented_archive_count = index + 1;
        self.changed();
        self.action_accepted(None)
    }

    pub fn advance(&mut self, elapsed_ms: u64) -> Vec<GameAdvanceEvent> {
        let scout_ready_for_queue = self.active_mission.is_none()
            || (!self.survey_queue.is_empty()
                && self.active_site_survey.is_none()
                && self
                    .active_mission
                    .as_ref()
                    .is_some_and(|mission| mission.phase == ScoutState::Holding));
        if scout_ready_for_queue {
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
        events.extend(self.advance_site_survey(elapsed_ms));
        events.extend(self.advance_base_projects(elapsed_ms));
        self.advance_base_energy(elapsed_ms);
        events.extend(self.advance_base_wear(elapsed_ms));
        events.extend(self.advance_archive_experiment(elapsed_ms));
        if self.refresh_horizon_completion() {
            events.push(GameAdvanceEvent::ArchiveObservationRecorded);
            events.push(GameAdvanceEvent::SolarHorizonReached);
        }
        events
    }

    fn advance_site_survey(&mut self, elapsed_ms: u64) -> Vec<GameAdvanceEvent> {
        let Some(mut survey) = self.active_site_survey.take() else {
            return Vec::new();
        };
        survey.remaining_ms = survey.remaining_ms.saturating_sub(elapsed_ms);
        if survey.remaining_ms > 0 {
            self.active_site_survey = Some(survey);
            self.changed();
            return Vec::new();
        }
        let Some(region) = self
            .world
            .regions
            .iter_mut()
            .find(|region| region.id == survey.region_id)
        else {
            self.changed();
            return Vec::new();
        };
        let Some(site_index) = region
            .subregions
            .iter()
            .position(|site| site.id == survey.subregion_id)
        else {
            self.changed();
            return Vec::new();
        };
        region.subregions[site_index].knowledge = SubregionKnowledge::Observed;
        let designation = region.subregions[site_index].designation.clone();
        let row = region.subregions[site_index].row;
        let column = region.subregions[site_index].column;
        let frontier_revealed = reveal_local_neighbors(region, row, column);
        self.analysis_units = self.analysis_units.saturating_add(1);
        self.push_activity(
            ActivityKind::Survey,
            Some(survey.region_id.clone()),
            format!(
                "Focused local survey completed at {designation}; one analysis unit recorded and {frontier_revealed} adjacent sector(s) detected."
            ),
        );
        self.changed();
        let mut events = vec![GameAdvanceEvent::SubregionSurveyed];
        if self.refresh_initial_archive_observation() {
            events.push(GameAdvanceEvent::ArchiveObservationRecorded);
        }
        if self.has_archive_observation(OBSERVATION_003)
            && !self.has_archive_observation(OBSERVATION_004)
        {
            if !self
                .archive_comparison_region_ids
                .iter()
                .any(|region_id| region_id == &survey.region_id)
            {
                self.archive_comparison_region_ids
                    .push(survey.region_id.clone());
                self.changed();
            }
            if self.archive_comparison_region_ids.len() >= 3 {
                self.record_archive_observation(OBSERVATION_004, None);
                events.push(GameAdvanceEvent::ArchiveObservationRecorded);
            }
        }
        events
    }

    fn advance_archive_experiment(&mut self, elapsed_ms: u64) -> Vec<GameAdvanceEvent> {
        let Some(mut experiment) = self.active_archive_experiment.take() else {
            return Vec::new();
        };
        let running = self.bases.iter().any(|base| {
            base.subregion_id == experiment.subregion_id
                && self.module_operational(base, BaseModuleKind::Laboratory)
        });
        if !running || elapsed_ms == 0 {
            self.active_archive_experiment = Some(experiment);
            return Vec::new();
        }
        experiment.remaining_ms = experiment.remaining_ms.saturating_sub(elapsed_ms);
        if experiment.remaining_ms > 0 {
            self.active_archive_experiment = Some(experiment);
            self.changed();
            return Vec::new();
        }
        self.analysis_units = self
            .analysis_units
            .saturating_add(ARCHIVE_EXPERIMENT_ANALYSIS);
        self.archive_comparison_region_ids.clear();
        self.push_activity(
            ActivityKind::Research,
            Some(experiment.region_id.clone()),
            format!(
                "Archive timing comparison completed; {} analysis units recorded.",
                ARCHIVE_EXPERIMENT_ANALYSIS
            ),
        );
        self.changed();
        self.record_archive_observation(OBSERVATION_003, Some(experiment.region_id));
        vec![
            GameAdvanceEvent::ArchiveExperimentCompleted,
            GameAdvanceEvent::ArchiveObservationRecorded,
        ]
    }

    fn advance_base_projects(&mut self, elapsed_ms: u64) -> Vec<GameAdvanceEvent> {
        if self.base_projects.is_empty() {
            return Vec::new();
        }
        let mut remaining = Vec::with_capacity(self.base_projects.len());
        let mut completed = Vec::new();
        for mut project in self.base_projects.drain(..) {
            project.remaining_ms = project.remaining_ms.saturating_sub(elapsed_ms);
            if project.remaining_ms == 0 {
                completed.push(project);
            } else {
                remaining.push(project);
            }
        }
        self.base_projects = remaining;
        let mut events = Vec::new();
        for project in completed {
            if project.maintenance {
                if let Some(base) = self
                    .bases
                    .iter_mut()
                    .find(|base| base.subregion_id == project.subregion_id)
                {
                    base.integrity = base.integrity.saturating_add(35).min(100);
                    let designation = base.designation.clone();
                    let integrity = base.integrity;
                    self.push_activity(
                        ActivityKind::Construction,
                        Some(project.region_id),
                        format!("Maintenance restored {designation} to {integrity}% integrity."),
                    );
                    events.push(GameAdvanceEvent::BaseMaintained);
                }
                continue;
            }
            match project.module {
                None => {
                    let base_id = format!("base_{:02}", self.next_base_id);
                    let designation = format!("Node {:02}", self.next_base_id);
                    self.next_base_id = self.next_base_id.saturating_add(1);
                    self.bases.push(BaseState {
                        id: base_id,
                        designation: designation.clone(),
                        region_id: project.region_id.clone(),
                        subregion_id: project.subregion_id,
                        integrity: 100,
                        stored_energy: 20,
                        energy_capacity: 40,
                        energy_remainder_ms: 0,
                        wear_remainder_ms: 0,
                        modules: Vec::new(),
                        disabled_modules: Vec::new(),
                    });
                    self.push_activity(
                        ActivityKind::Construction,
                        Some(project.region_id),
                        format!("{designation} established; local power and communications remain limited."),
                    );
                    events.push(GameAdvanceEvent::BaseEstablished);
                }
                Some(module) => {
                    if let Some(base) = self
                        .bases
                        .iter_mut()
                        .find(|base| base.subregion_id == project.subregion_id)
                    {
                        base.modules.push(module);
                        if module == BaseModuleKind::PowerArray {
                            base.energy_capacity = 100;
                        }
                        let designation = base.designation.clone();
                        if module == BaseModuleKind::Laboratory {
                            self.analysis_units = self.analysis_units.saturating_add(2);
                        }
                        self.push_activity(
                            ActivityKind::Construction,
                            Some(project.region_id),
                            format!("{} completed at {designation}.", base_module_title(module)),
                        );
                        events.push(GameAdvanceEvent::BaseModuleCompleted);
                    }
                }
            }
        }
        self.changed();
        events
    }

    fn advance_base_wear(&mut self, elapsed_ms: u64) -> Vec<GameAdvanceEvent> {
        let mut alerts = Vec::new();
        let mut changed = false;
        for index in 0..self.bases.len() {
            let subregion_id = self.bases[index].subregion_id.clone();
            let hazard = self
                .find_subregion(&subregion_id)
                .map_or(RiskLevel::Low, |(_, site)| site.hazard);
            let workshop_active =
                self.module_operational(&self.bases[index], BaseModuleKind::Workshop);
            let interval = if workshop_active {
                Some(60_000)
            } else {
                match hazard {
                    RiskLevel::Low => None,
                    RiskLevel::Moderate => Some(300_000),
                    RiskLevel::High => Some(180_000),
                }
            };
            let Some(interval) = interval else {
                continue;
            };
            let base = &mut self.bases[index];
            base.wear_remainder_ms = base.wear_remainder_ms.saturating_add(elapsed_ms);
            let cycles = base.wear_remainder_ms / interval;
            base.wear_remainder_ms %= interval;
            if cycles == 0 {
                continue;
            }
            let previous = base.integrity;
            if workshop_active {
                base.integrity = base
                    .integrity
                    .saturating_add(cycles.min(u64::from(u32::MAX)) as u32)
                    .min(100);
            } else {
                base.integrity = base
                    .integrity
                    .saturating_sub(cycles.min(u64::from(u32::MAX)) as u32)
                    .max(20);
            }
            changed |= previous != base.integrity;
            if previous > 50 && base.integrity <= 50 || previous > 20 && base.integrity == 20 {
                alerts.push((
                    base.region_id.clone(),
                    base.designation.clone(),
                    base.integrity,
                ));
            }
        }
        if changed {
            self.changed();
        }
        if alerts.is_empty() {
            return Vec::new();
        }
        for (region_id, designation, integrity) in alerts {
            self.push_activity(
                ActivityKind::Loss,
                Some(region_id),
                format!("Environmental strain reduced {designation} to {integrity}% integrity."),
            );
        }
        vec![GameAdvanceEvent::BaseStrained]
    }

    fn advance_base_energy(&mut self, elapsed_ms: u64) {
        let mut changed = false;
        for index in 0..self.bases.len() {
            let subregion_id = self.bases[index].subregion_id.clone();
            let energy_yield = self
                .find_subregion(&subregion_id)
                .map_or(0, |(_, subregion)| subregion.energy_yield);
            let modules = self.bases[index]
                .modules
                .iter()
                .copied()
                .filter(|module| !self.bases[index].disabled_modules.contains(module))
                .collect::<Vec<_>>();
            let generation = 1 + u32::from(modules.contains(&BaseModuleKind::PowerArray))
                * (2 + energy_yield * 2);
            let full_demand = 1 + modules.iter().copied().map(module_demand).sum::<u32>();
            let base = &mut self.bases[index];
            base.energy_remainder_ms = base.energy_remainder_ms.saturating_add(elapsed_ms);
            let seconds = base.energy_remainder_ms / 1_000;
            base.energy_remainder_ms %= 1_000;
            if seconds == 0 {
                continue;
            }
            let previous = base.stored_energy;
            if generation >= full_demand {
                let gain = u64::from(generation - full_demand).saturating_mul(seconds);
                base.stored_energy = base
                    .stored_energy
                    .saturating_add(gain.min(u64::from(u32::MAX)) as u32)
                    .min(base.energy_capacity);
            } else if base.stored_energy > 0 {
                let drain = u64::from(full_demand - generation).saturating_mul(seconds);
                base.stored_energy = base
                    .stored_energy
                    .saturating_sub(drain.min(u64::from(u32::MAX)) as u32);
            }
            changed |= previous != base.stored_energy;
        }
        if changed {
            self.changed();
        }
    }

    fn refresh_horizon_completion(&mut self) -> bool {
        if self.horizon_reached
            || !self.has_archive_observation(OBSERVATION_004)
            || !self.horizon_progress().ready
        {
            return false;
        }
        self.record_archive_observation(OBSERVATION_005, None);
        self.horizon_reached = true;
        self.push_activity(
            ActivityKind::Milestone,
            None,
            "Planetary observation network resolved the local-system frame.".into(),
        );
        self.changed();
        true
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
                        reveal_local_entry(region);
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
                        events.push(GameAdvanceEvent::FoundationCompleted);
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
                    mission.remaining_ms = mission.extraction_duration_ms;
                    mission.phase_total_ms = mission.extraction_duration_ms;
                }
                ExtractorState::Extracting if mission.will_fail => {
                    self.extractor_available = false;
                    self.extractor_rebuild_remaining_ms = mission.rebuild_duration_ms;
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
                    let deposit = self
                        .world
                        .regions
                        .iter_mut()
                        .find(|region| region.id == mission.target_region_id)
                        .and_then(|region| {
                            if let Some(subregion_id) = &mission.target_subregion_id {
                                region
                                    .subregions
                                    .iter_mut()
                                    .find(|subregion| &subregion.id == subregion_id)
                                    .and_then(|subregion| subregion.deposit.as_mut())
                            } else {
                                region.deposit.as_mut()
                            }
                        });
                    if let Some(deposit) = deposit {
                        let amount = deposit.remaining.min(mission.cargo_capacity);
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
                        let designation = mission.target_subregion_id.as_ref().map_or_else(
                            || self.region_designation(&mission.target_region_id),
                            |subregion_id| self.subregion_designation(subregion_id),
                        );
                        self.push_activity(
                            ActivityKind::Extraction,
                            Some(mission.target_region_id.clone()),
                            format!(
                                "Extractor returned {amount} {} units from {designation}.",
                                resource_label(resource)
                            ),
                        );
                        if mission.target_subregion_id.is_none()
                            && !self
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
                        events.push(GameAdvanceEvent::FoundationCompleted);
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
            let target_position = region.position.clone();
            let network_hops = self.travel_hops_to_network(&target_position).max(1);
            let travel_ms = u64::from(network_hops) * TRAVEL_MS_PER_HOP;
            let mut outbound_ms = travel_ms;

            if let Some(current) = &self.active_mission {
                if current.phase != ScoutState::Holding || self.active_site_survey.is_some() {
                    self.survey_queue.insert(0, region_id);
                    return;
                }
                let previous_region_id = current.target_region_id.clone();
                if let Some(previous_region) = self
                    .world
                    .regions
                    .iter_mut()
                    .find(|candidate| candidate.id == previous_region_id)
                {
                    let direct_hops = graph_distance(&previous_region.position, &target_position)
                        .min(network_hops)
                        .max(1);
                    outbound_ms = u64::from(direct_hops) * TRAVEL_MS_PER_HOP;
                    if previous_region.knowledge == RegionKnowledge::Observed {
                        previous_region.knowledge = RegionKnowledge::Surveyed;
                    }
                }
            }

            let mission_id = self.next_mission_id;
            self.next_mission_id += 1;
            self.active_mission = Some(MissionState {
                mission_id,
                target_region_id: region_id,
                phase: ScoutState::Outbound,
                remaining_ms: outbound_ms,
                phase_total_ms: outbound_ms,
                travel_ms,
                survey_required: true,
                survey_completed: false,
                auto_return: self.automation_unlocked(),
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
        let loadout = self.extractor_loadout;
        let travel_ms = extraction_travel_ms(
            self.travel_hops_to_network(&region.position).max(1),
            loadout,
        );
        let risk = self.effective_risk(region.atmospheric_severity);
        let risk = if self.region_has_active_module(&region_id, BaseModuleKind::Workshop) {
            lower_risk(risk)
        } else {
            risk
        };
        let risk = loadout_risk(risk, loadout);
        let cargo_capacity = loadout_capacity(self.extraction_capacity_for(&region_id), loadout);
        let will_fail = self.next_random_percent() < risk_percent(risk);
        let mission_id = self.next_mission_id;
        self.next_mission_id += 1;
        self.active_extraction = Some(ExtractionMissionState {
            mission_id,
            target_region_id: region_id,
            target_subregion_id: None,
            phase: ExtractorState::Outbound,
            remaining_ms: travel_ms,
            phase_total_ms: travel_ms,
            travel_ms,
            risk,
            will_fail,
            cargo_capacity,
            extraction_duration_ms: loadout.extraction_duration_ms(),
            rebuild_duration_ms: loadout.rebuild_duration_ms(),
            loadout,
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
                    risk: self.extraction_risk_for(
                        &region.id,
                        self.effective_risk(region.atmospheric_severity),
                    ),
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

    fn has_archive_observation(&self, record_id: &str) -> bool {
        self.archive_records
            .iter()
            .any(|record| record.record_id == record_id)
    }

    fn record_archive_observation(
        &mut self,
        record_id: &'static str,
        region_id: Option<String>,
    ) -> bool {
        if self.has_archive_observation(record_id) {
            return false;
        }
        let Some((title, medium, observation, interpretation)) = archive_content(record_id) else {
            return false;
        };
        self.changed();
        self.archive_records.push(ArchiveRecordState {
            record_id: record_id.into(),
            title: title.into(),
            medium: medium.into(),
            observation: observation.into(),
            interpretation: interpretation.into(),
            region_id: region_id.clone(),
            discovered_at_revision: self.state_revision,
        });
        self.push_activity_with_archive(
            ActivityKind::Milestone,
            region_id,
            Some(record_id.into()),
            format!("Permanent archive observation recorded: {title}."),
        );
        true
    }

    fn refresh_initial_archive_observation(&mut self) -> bool {
        if !self.has_archive_observation(OBSERVATION_001)
            || self.has_archive_observation(OBSERVATION_002)
        {
            return false;
        }
        let observed_sites = self
            .world
            .regions
            .iter()
            .flat_map(|region| &region.subregions)
            .filter(|site| site.knowledge == SubregionKnowledge::Observed)
            .count();
        if observed_sites < 3 {
            return false;
        }
        let Some((region_id, _)) = self.world.regions.iter().find_map(|region| {
            region
                .subregions
                .iter()
                .find(|site| {
                    site.class == SubregionClass::ShelteredShelf
                        && site.knowledge == SubregionKnowledge::Observed
                })
                .map(|site| (region.id.clone(), site.id.clone()))
        }) else {
            return false;
        };
        self.record_archive_observation(OBSERVATION_002, Some(region_id));
        self.raise_comparable_site_confidence();
        true
    }

    fn raise_comparable_site_confidence(&mut self) {
        let compared_regions = self
            .world
            .regions
            .iter()
            .filter(|region| {
                region
                    .subregions
                    .iter()
                    .any(|site| site.knowledge == SubregionKnowledge::Observed)
            })
            .map(|region| region.id.clone())
            .collect::<Vec<_>>();
        let mut changed = false;
        for region in &mut self.world.regions {
            if !compared_regions.contains(&region.id) {
                continue;
            }
            for site in &mut region.subregions {
                if site.class == SubregionClass::ShelteredShelf
                    && site.knowledge == SubregionKnowledge::Detected
                {
                    site.knowledge = SubregionKnowledge::Surveyed;
                    changed = true;
                }
            }
        }
        if changed {
            self.changed();
        }
    }

    fn first_sheltered_site(&self) -> Option<(String, String)> {
        self.world.regions.iter().find_map(|region| {
            (region.knowledge >= RegionKnowledge::Surveyed)
                .then(|| {
                    region
                        .subregions
                        .iter()
                        .find(|site| site.class == SubregionClass::ShelteredShelf)
                        .map(|site| (region.id.clone(), site.id.clone()))
                })
                .flatten()
        })
    }

    fn operational_laboratory(&self) -> Option<(String, String)> {
        self.bases.iter().find_map(|base| {
            self.module_operational(base, BaseModuleKind::Laboratory)
                .then(|| (base.region_id.clone(), base.subregion_id.clone()))
        })
    }

    fn archive_experiment_snapshot(&self) -> Option<ArchiveExperimentSnapshot> {
        if !self.has_archive_observation(OBSERVATION_002)
            || self.has_archive_observation(OBSERVATION_003)
        {
            return None;
        }
        if let Some(experiment) = &self.active_archive_experiment {
            let running = self.bases.iter().any(|base| {
                base.subregion_id == experiment.subregion_id
                    && self.module_operational(base, BaseModuleKind::Laboratory)
            });
            return Some(ArchiveExperimentSnapshot {
                available: false,
                active: true,
                running,
                remaining_ms: experiment.remaining_ms,
                phase_total_ms: experiment.phase_total_ms,
                status: if running {
                    "Timing comparison is resolving at the powered laboratory.".into()
                } else {
                    "Paused until the assigned laboratory regains power and integrity.".into()
                },
            });
        }
        let research_ready = self.completed_research.len() >= HORIZON_REQUIRED_RESEARCH;
        let laboratory_ready = self.operational_laboratory().is_some();
        let status = if !research_ready {
            format!(
                "Complete two research directions before comparing the recovered sequence ({}/2).",
                self.completed_research.len()
            )
        } else if !laboratory_ready {
            "A powered analysis laboratory is required for the timing comparison.".into()
        } else {
            "Recovered timing blocks are ready for a repeatable comparison.".into()
        };
        Some(ArchiveExperimentSnapshot {
            available: research_ready && laboratory_ready,
            active: false,
            running: false,
            remaining_ms: ARCHIVE_EXPERIMENT_MS,
            phase_total_ms: ARCHIVE_EXPERIMENT_MS,
            status,
        })
    }

    fn objective_snapshot(&self) -> ObjectiveSnapshot {
        if self.horizon_reached {
            return ObjectiveSnapshot {
                title: "SOLAR HORIZON REACHED".into(),
                guidance: "Open System Map to review the local frame. No system-scale operations are available in this demo.".into(),
                complete: true,
            };
        }
        if let Some(experiment) = &self.active_archive_experiment {
            let running = self.bases.iter().any(|base| {
                base.subregion_id == experiment.subregion_id
                    && self.module_operational(base, BaseModuleKind::Laboratory)
            });
            return ObjectiveSnapshot {
                title: "ARCHIVE COMPARISON ACTIVE".into(),
                guidance: if running {
                    format!(
                        "Timing comparison is resolving; {}s remain. Field operations may continue.",
                        experiment.remaining_ms.div_ceil(1_000)
                    )
                } else {
                    "Restore power and integrity at the assigned laboratory to resume the comparison."
                        .into()
                },
                complete: false,
            };
        }
        if self.has_archive_observation(OBSERVATION_003)
            && !self.has_archive_observation(OBSERVATION_004)
        {
            return ObjectiveSnapshot {
                title: "COMPARE INDEPENDENT SITES".into(),
                guidance: "Run focused local surveys in separate regions. The archive comparison needs observations that do not share one local context."
                    .into(),
                complete: false,
            };
        }
        if self
            .archive_experiment_snapshot()
            .is_some_and(|experiment| experiment.available)
        {
            return ObjectiveSnapshot {
                title: "RUN THE ARCHIVE COMPARISON".into(),
                guidance: "Open Archive and press Enter to compare the recovered timing blocks at the powered laboratory."
                    .into(),
                complete: false,
            };
        }
        if self.alpha_complete {
            let horizon = self.horizon_progress();
            if let Some(survey) = &self.active_site_survey {
                return ObjectiveSnapshot {
                    title: "RESOLVE LOCAL CONDITIONS".into(),
                    guidance: format!(
                        "Focused survey at {} is active. Choose the next energy, hazard, or deposit tradeoff while it resolves.",
                        self.subregion_designation(&survey.subregion_id)
                    ),
                    complete: false,
                };
            }
            if !self.base_projects.is_empty() {
                return ObjectiveSnapshot {
                    title: "EXTEND PERMANENT PRESENCE".into(),
                    guidance: format!(
                        "{} network project(s) are assembling. Survey, extract, research, or plan the next node concurrently.",
                        self.base_projects.len()
                    ),
                    complete: false,
                };
            }
            if horizon.surveyed_subregions == 0 {
                return ObjectiveSnapshot {
                    title: "OPEN THE LOCAL SCALE".into(),
                    guidance: "Hold the scout in a surveyed region, press V, choose a detected site, and run a focused survey.".into(),
                    complete: false,
                };
            }
            if self.bases.is_empty() {
                return ObjectiveSnapshot {
                    title: "ESTABLISH A FOOTHOLD".into(),
                    guidance: "Compare surveyed sites for energy, hazard, and deposits; establish the first permanent node at one of them.".into(),
                    complete: false,
                };
            }
            if horizon.connected_bases == 0 {
                return ObjectiveSnapshot {
                    title: "POWER AND CONNECT THE NODE".into(),
                    guidance: "Build a power array, then communications. Suspend optional modules when demand would drain reserve or sever the network.".into(),
                    complete: false,
                };
            }
            if self.completed_research.len() < HORIZON_REQUIRED_RESEARCH {
                return ObjectiveSnapshot {
                    title: "DIVERSIFY THE OPERATING MODEL".into(),
                    guidance: format!(
                        "Complete {} more research direction(s). Focused site surveys and laboratories provide analysis.",
                        HORIZON_REQUIRED_RESEARCH - self.completed_research.len()
                    ),
                    complete: false,
                };
            }
            if horizon.surveyed_subregions < HORIZON_REQUIRED_SUBREGIONS {
                return ObjectiveSnapshot {
                    title: "COMPARE PLANETARY SITES".into(),
                    guidance: format!(
                        "Resolve {} more local site(s). Use the network to choose useful regions instead of exhausting one area.",
                        HORIZON_REQUIRED_SUBREGIONS - horizon.surveyed_subregions
                    ),
                    complete: false,
                };
            }
            if horizon.connected_bases < HORIZON_REQUIRED_BASES {
                return ObjectiveSnapshot {
                    title: "FORM A PLANETARY CHAIN".into(),
                    guidance: format!(
                        "Connect {} more powered node(s). Communications bridge three regional hops and extend field reach.",
                        HORIZON_REQUIRED_BASES - horizon.connected_bases
                    ),
                    complete: false,
                };
            }
            if horizon.remote_bases < HORIZON_REQUIRED_REMOTE_BASES {
                return ObjectiveSnapshot {
                    title: "CROSS THE RELAY ENVELOPE".into(),
                    guidance: "Use a connected node's three-hop communications reach to survey and establish a node beyond the original core-and-relay envelope.".into(),
                    complete: false,
                };
            }
            if horizon.analysis_sites < horizon.required_analysis_sites {
                return ObjectiveSnapshot {
                    title: "ESTABLISH DISTRIBUTED ANALYSIS".into(),
                    guidance: "Build an analysis laboratory at a powered, connected node. Other nodes can then specialize for observation.".into(),
                    complete: false,
                };
            }
            if horizon.surveyed_regions < HORIZON_REQUIRED_REGIONS {
                return ObjectiveSnapshot {
                    title: "WIDEN PLANETARY COVERAGE".into(),
                    guidance: format!(
                        "Survey {} more region(s). Use queued surveys and the communications chain to choose the route.",
                        HORIZON_REQUIRED_REGIONS - horizon.surveyed_regions
                    ),
                    complete: false,
                };
            }
            if horizon.observation_sites < HORIZON_REQUIRED_OBSERVATORIES {
                return ObjectiveSnapshot {
                    title: "RESOLVE THE OUTER FRAME".into(),
                    guidance: {
                        let remaining = HORIZON_REQUIRED_OBSERVATORIES - horizon.observation_sites;
                        format!(
                            "Build {remaining} more powered observation {} at connected nodes with complementary local conditions.",
                            if remaining == 1 { "array" } else { "arrays" }
                        )
                    },
                    complete: false,
                };
            }
            return ObjectiveSnapshot {
                title: "STABILIZE THE OBSERVATION NETWORK".into(),
                guidance: "Restore power or communications at isolated nodes; shed optional load where necessary. The solar frame resolves when the complete network is simultaneously active.".into(),
                complete: false,
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
        self.within_foundation_range(position)
            || self.connected_bases().iter().any(|base_id| {
                self.bases
                    .iter()
                    .find(|base| &base.id == base_id)
                    .and_then(|base| {
                        self.world
                            .regions
                            .iter()
                            .find(|region| region.id == base.region_id)
                    })
                    .is_some_and(|region| {
                        graph_distance(position, &region.position) <= BASE_COMMUNICATION_RANGE_HOPS
                    })
            })
    }

    fn within_foundation_range(&self, position: &RegionPosition) -> bool {
        hop_distance(position) <= self.scout_range_hops()
            || self
                .relay_region_id
                .as_ref()
                .and_then(|id| self.world.regions.iter().find(|region| &region.id == id))
                .is_some_and(|relay| {
                    graph_distance(position, &relay.position) <= self.relay_range_hops()
                })
    }

    fn connected_bases(&self) -> Vec<String> {
        let mut connected = Vec::new();
        loop {
            let mut changed = false;
            for base in &self.bases {
                if connected.contains(&base.id)
                    || !self.module_operational(base, BaseModuleKind::Communications)
                {
                    continue;
                }
                let Some(region) = self
                    .world
                    .regions
                    .iter()
                    .find(|region| region.id == base.region_id)
                else {
                    continue;
                };
                let linked = self.within_foundation_range(&region.position)
                    || connected.iter().any(|connected_id| {
                        self.bases
                            .iter()
                            .find(|candidate| &candidate.id == connected_id)
                            .and_then(|candidate| {
                                self.world
                                    .regions
                                    .iter()
                                    .find(|other| other.id == candidate.region_id)
                            })
                            .is_some_and(|other| {
                                graph_distance(&region.position, &other.position)
                                    <= BASE_COMMUNICATION_RANGE_HOPS
                            })
                    });
                if linked {
                    connected.push(base.id.clone());
                    changed = true;
                }
            }
            if !changed {
                return connected;
            }
        }
    }

    fn travel_hops_to_network(&self, position: &RegionPosition) -> u8 {
        let mut distance = hop_distance(position);
        if let Some(relay) = self
            .relay_region_id
            .as_ref()
            .and_then(|id| self.world.regions.iter().find(|region| &region.id == id))
        {
            distance = distance.min(graph_distance(position, &relay.position));
        }
        for base_id in self.connected_bases() {
            if let Some(region) =
                self.bases
                    .iter()
                    .find(|base| base.id == base_id)
                    .and_then(|base| {
                        self.world
                            .regions
                            .iter()
                            .find(|region| region.id == base.region_id)
                    })
            {
                distance = distance.min(graph_distance(position, &region.position));
            }
        }
        distance
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

    fn network_snapshot(&self) -> PlanetaryNetworkSnapshot {
        let unlocked = self.alpha_complete;
        let subregions = self
            .world
            .regions
            .iter()
            .filter(|region| region.knowledge >= RegionKnowledge::Surveyed)
            .flat_map(|region| {
                region
                    .subregions
                    .iter()
                    .map(|subregion| self.subregion_snapshot(&region.id, subregion))
            })
            .collect();
        PlanetaryNetworkSnapshot {
            unlocked,
            subregion_rows: SUBREGION_ROWS,
            subregion_columns: SUBREGION_COLUMNS,
            subregions,
            bases: self
                .bases
                .iter()
                .map(|base| self.base_snapshot(base))
                .collect(),
            active_projects: self
                .base_projects
                .iter()
                .map(|project| BaseProjectSnapshot {
                    project_id: project.project_id,
                    region_id: project.region_id.clone(),
                    subregion_id: project.subregion_id.clone(),
                    module: project.module,
                    maintenance: project.maintenance,
                    logistics_supported: project.logistics_supported,
                    remaining_ms: project.remaining_ms,
                    phase_total_ms: project.phase_total_ms,
                    cost: project.cost,
                })
                .collect(),
            active_site_survey: self
                .active_site_survey
                .as_ref()
                .map(|survey| SiteSurveySnapshot {
                    subregion_id: survey.subregion_id.clone(),
                    remaining_ms: survey.remaining_ms,
                    phase_total_ms: survey.phase_total_ms,
                }),
            project_capacity: self.base_project_capacity(),
            horizon: self.horizon_progress(),
        }
    }

    fn subregion_snapshot(&self, region_id: &str, subregion: &SubregionState) -> SubregionSnapshot {
        let surveyed = subregion.knowledge >= SubregionKnowledge::Surveyed;
        let base_id = self
            .bases
            .iter()
            .find(|base| base.subregion_id == subregion.id)
            .map(|base| base.id.clone());
        SubregionSnapshot {
            id: subregion.id.clone(),
            designation: subregion.designation.clone(),
            region_id: region_id.to_owned(),
            slot: subregion.slot,
            row: subregion.row,
            column: subregion.column,
            knowledge: subregion.knowledge,
            class: (subregion.knowledge >= SubregionKnowledge::Detected).then_some(subregion.class),
            hazard: surveyed.then_some(subregion.hazard),
            energy_yield: surveyed.then_some(subregion.energy_yield),
            deposit: surveyed
                .then_some(subregion.deposit.as_ref())
                .flatten()
                .map(|deposit| DepositSnapshot {
                    resource: deposit.resource,
                    remaining: deposit.remaining,
                    risk: self
                        .extraction_risk_for(region_id, self.effective_site_risk(subregion.hazard)),
                }),
            base_id,
        }
    }

    fn base_snapshot(&self, base: &BaseState) -> BaseSnapshot {
        let (generation, demand, _) = self.base_power_state(base);
        let connected = self.connected_bases().contains(&base.id);
        BaseSnapshot {
            id: base.id.clone(),
            designation: base.designation.clone(),
            region_id: base.region_id.clone(),
            subregion_id: base.subregion_id.clone(),
            integrity: base.integrity,
            generation,
            demand,
            stored_energy: base.stored_energy,
            energy_capacity: base.energy_capacity,
            connected,
            modules: base
                .modules
                .iter()
                .copied()
                .map(|kind| BaseModuleSnapshot {
                    kind,
                    enabled: self.module_enabled(base, kind),
                    active: self.module_operational(base, kind),
                    generation: if self.module_enabled(base, kind) {
                        self.module_generation(base, kind)
                    } else {
                        0
                    },
                    demand: module_demand(kind),
                })
                .collect(),
        }
    }

    fn horizon_progress(&self) -> HorizonProgressSnapshot {
        let surveyed_regions = self
            .world
            .regions
            .iter()
            .filter(|region| region.knowledge >= RegionKnowledge::Surveyed)
            .count();
        let surveyed_subregions = self
            .world
            .regions
            .iter()
            .flat_map(|region| &region.subregions)
            .filter(|subregion| subregion.knowledge >= SubregionKnowledge::Surveyed)
            .count();
        let connected = self.connected_bases();
        let connected_bases = connected.len();
        let remote_bases = self
            .bases
            .iter()
            .filter(|base| {
                connected.contains(&base.id)
                    && self
                        .world
                        .regions
                        .iter()
                        .find(|region| region.id == base.region_id)
                        .is_some_and(|region| !self.within_foundation_range(&region.position))
            })
            .count();
        let observation_sites = self
            .bases
            .iter()
            .filter(|base| {
                connected.contains(&base.id)
                    && self.module_operational(base, BaseModuleKind::Observatory)
            })
            .count();
        let analysis_sites = self
            .bases
            .iter()
            .filter(|base| {
                connected.contains(&base.id)
                    && self.module_operational(base, BaseModuleKind::Laboratory)
            })
            .count();
        let automation_proven =
            self.automated_survey_completed || self.automated_extraction_completed;
        let ready = self.alpha_complete
            && surveyed_regions >= HORIZON_REQUIRED_REGIONS
            && surveyed_subregions >= HORIZON_REQUIRED_SUBREGIONS
            && connected_bases >= HORIZON_REQUIRED_BASES
            && remote_bases >= HORIZON_REQUIRED_REMOTE_BASES
            && observation_sites >= HORIZON_REQUIRED_OBSERVATORIES
            && analysis_sites >= 1
            && self.completed_research.len() >= HORIZON_REQUIRED_RESEARCH
            && automation_proven;
        HorizonProgressSnapshot {
            surveyed_regions,
            required_regions: HORIZON_REQUIRED_REGIONS,
            surveyed_subregions,
            required_subregions: HORIZON_REQUIRED_SUBREGIONS,
            connected_bases,
            required_bases: HORIZON_REQUIRED_BASES,
            remote_bases,
            required_remote_bases: HORIZON_REQUIRED_REMOTE_BASES,
            observation_sites,
            required_observation_sites: HORIZON_REQUIRED_OBSERVATORIES,
            analysis_sites,
            required_analysis_sites: 1,
            completed_research: self.completed_research.len(),
            required_research: HORIZON_REQUIRED_RESEARCH,
            automation_proven,
            ready,
        }
    }

    fn base_power_state(&self, base: &BaseState) -> (u32, u32, bool) {
        let generation = 1 + base
            .modules
            .iter()
            .copied()
            .filter(|module| self.module_enabled(base, *module))
            .map(|module| self.module_generation(base, module))
            .sum::<u32>();
        let demand = 1 + base
            .modules
            .iter()
            .copied()
            .filter(|module| self.module_enabled(base, *module))
            .map(module_demand)
            .sum::<u32>();
        let powered = base.integrity > 20 && (generation >= demand || base.stored_energy > 0);
        (generation, demand, powered)
    }

    fn module_enabled(&self, base: &BaseState, module: BaseModuleKind) -> bool {
        base.modules.contains(&module) && !base.disabled_modules.contains(&module)
    }

    fn module_operational(&self, base: &BaseState, module: BaseModuleKind) -> bool {
        if !self.module_enabled(base, module) || base.integrity <= 20 {
            return false;
        }
        module == BaseModuleKind::PowerArray || self.base_power_state(base).2
    }

    fn module_generation(&self, base: &BaseState, module: BaseModuleKind) -> u32 {
        if module != BaseModuleKind::PowerArray {
            return 0;
        }
        self.find_subregion(&base.subregion_id)
            .map_or(0, |(_, subregion)| 2 + subregion.energy_yield * 2)
    }

    fn effective_site_risk(&self, hazard: RiskLevel) -> RiskLevel {
        if self.operational_profile != OperationalProfile::Resilience {
            return hazard;
        }
        match hazard {
            RiskLevel::High => RiskLevel::Moderate,
            RiskLevel::Moderate | RiskLevel::Low => RiskLevel::Low,
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

    fn survey_queue_capacity(&self) -> usize {
        if self.automation_unlocked() {
            SURVEY_QUEUE_CAPACITY
        } else {
            PRE_AUTOMATION_WAYPOINT_CAPACITY
        }
    }

    fn survey_duration_ms(&self) -> u64 {
        if self.operational_profile == OperationalProfile::Reach {
            14_000
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

    fn extraction_capacity_for(&self, region_id: &str) -> u32 {
        self.extraction_capacity()
            + if self.region_has_active_module(region_id, BaseModuleKind::Depot) {
                4
            } else {
                0
            }
    }

    fn extraction_risk_for(&self, region_id: &str, risk: RiskLevel) -> RiskLevel {
        let supported = if self.region_has_active_module(region_id, BaseModuleKind::Workshop) {
            lower_risk(risk)
        } else {
            risk
        };
        loadout_risk(supported, self.extractor_loadout)
    }

    fn region_has_active_module(&self, region_id: &str, module: BaseModuleKind) -> bool {
        self.bases
            .iter()
            .any(|base| base.region_id == region_id && self.module_operational(base, module))
    }

    fn region_has_active_module_anywhere(&self, module: BaseModuleKind) -> bool {
        self.bases
            .iter()
            .any(|base| self.module_operational(base, module))
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
            "Routine automation proven; planetary network planning is now available.".into(),
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

    fn subregion_designation(&self, subregion_id: &str) -> String {
        self.find_subregion(subregion_id).map_or_else(
            || subregion_id.to_owned(),
            |(_, site)| site.designation.clone(),
        )
    }

    fn find_subregion(&self, subregion_id: &str) -> Option<(&RegionState, &SubregionState)> {
        self.world.regions.iter().find_map(|region| {
            region
                .subregions
                .iter()
                .find(|subregion| subregion.id == subregion_id)
                .map(|subregion| (region, subregion))
        })
    }

    fn find_subregion_mut(&mut self, subregion_id: &str) -> Option<&mut SubregionState> {
        self.world.regions.iter_mut().find_map(|region| {
            region
                .subregions
                .iter_mut()
                .find(|subregion| subregion.id == subregion_id)
        })
    }

    fn project_conflicts(&self, region_id: &str, subregion_id: &str) -> bool {
        self.base_projects
            .iter()
            .any(|project| project.region_id == region_id || project.subregion_id == subregion_id)
    }

    fn base_project_capacity(&self) -> usize {
        BASE_PROJECT_CAPACITY
            + usize::from(self.operational_profile == OperationalProfile::Throughput)
            + self.active_logistics_nodes().min(MAX_DEPOT_PROJECT_BONUS)
    }

    fn active_logistics_nodes(&self) -> usize {
        let connected = self.connected_bases();
        self.bases
            .iter()
            .filter(|base| {
                connected.contains(&base.id) && self.module_operational(base, BaseModuleKind::Depot)
            })
            .count()
    }

    fn logistics_support_hops(&self, region_id: &str) -> Option<u8> {
        let target = self
            .world
            .regions
            .iter()
            .find(|region| region.id == region_id)?;
        let connected = self.connected_bases();
        self.bases
            .iter()
            .filter(|base| {
                connected.contains(&base.id) && self.module_operational(base, BaseModuleKind::Depot)
            })
            .filter_map(|base| {
                self.world
                    .regions
                    .iter()
                    .find(|region| region.id == base.region_id)
                    .map(|region| graph_distance(&target.position, &region.position))
            })
            .filter(|hops| *hops <= BASE_COMMUNICATION_RANGE_HOPS)
            .min()
    }

    fn push_activity(&mut self, kind: ActivityKind, region_id: Option<String>, summary: String) {
        self.push_activity_with_archive(kind, region_id, None, summary);
    }

    fn push_activity_with_archive(
        &mut self,
        kind: ActivityKind,
        region_id: Option<String>,
        archive_record_id: Option<String>,
        summary: String,
    ) {
        let record = ActivityRecord {
            activity_id: self.next_activity_id,
            kind,
            region_id,
            archive_record_id,
            summary,
        };
        self.next_activity_id = self.next_activity_id.saturating_add(1);
        if self.activity_log.len() == ACTIVITY_CAPACITY {
            let evict_index = self
                .activity_log
                .iter()
                .position(|entry| entry.activity_id <= self.acknowledged_activity_id)
                .or_else(|| {
                    self.activity_log
                        .iter()
                        .position(|entry| entry.kind != ActivityKind::Milestone)
                })
                .unwrap_or(0);
            self.activity_log.remove(evict_index);
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
        let mut subregion_ids =
            Vec::with_capacity(REGION_COUNT * usize::from(SUBREGIONS_PER_REGION));
        for region in &self.world.regions {
            if region.subregions.len() != usize::from(SUBREGIONS_PER_REGION) {
                return Err(GameError::Invariant(
                    "every region must contain a complete local field",
                ));
            }
            for slot in 0..SUBREGIONS_PER_REGION {
                if region
                    .subregions
                    .iter()
                    .filter(|subregion| subregion.slot == slot)
                    .count()
                    != 1
                {
                    return Err(GameError::Invariant("local site slots must be unique"));
                }
            }
            for row in 0..SUBREGION_ROWS {
                for column in 0..SUBREGION_COLUMNS {
                    if region
                        .subregions
                        .iter()
                        .filter(|site| site.row == row && site.column == column)
                        .count()
                        != 1
                    {
                        return Err(GameError::Invariant(
                            "local field coordinates must be unique and complete",
                        ));
                    }
                }
            }
            for subregion in &region.subregions {
                if subregion.energy_yield == 0
                    || subregion.energy_yield > 6
                    || subregion.row >= SUBREGION_ROWS
                    || subregion.column >= SUBREGION_COLUMNS
                    || subregion
                        .deposit
                        .as_ref()
                        .is_some_and(|deposit| deposit.remaining > 64)
                    || subregion_ids.contains(&subregion.id)
                    || (region.knowledge < RegionKnowledge::Surveyed
                        && subregion.knowledge != SubregionKnowledge::Unknown)
                {
                    return Err(GameError::Invariant("local site state is inconsistent"));
                }
                subregion_ids.push(subregion.id.clone());
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
                || mission.cargo_capacity == 0
                || mission.cargo_capacity > 12
                || mission.extraction_duration_ms != mission.loadout.extraction_duration_ms()
                || mission.rebuild_duration_ms != mission.loadout.rebuild_duration_ms()
                || mission
                    .target_subregion_id
                    .as_ref()
                    .is_some_and(|subregion_id| {
                        self.find_subregion(subregion_id)
                            .is_none_or(|(region, subregion)| {
                                region.id != mission.target_region_id
                                    || subregion.knowledge < SubregionKnowledge::Surveyed
                                    || subregion.deposit.is_none()
                            })
                    })
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
                || research.phase_total_ms == 0
                || research.remaining_ms > research.phase_total_ms
                || self.completed_research.contains(&research.project)
        }) {
            return Err(GameError::Invariant("active research is inconsistent"));
        }
        if !self.profile_unlocked(self.operational_profile) {
            return Err(GameError::Invariant("active profile is locked"));
        }
        if self.survey_queue.len() > self.survey_queue_capacity()
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
        if let Some(survey) = &self.active_site_survey {
            if survey.remaining_ms == 0
                || survey.phase_total_ms == 0
                || self
                    .find_subregion(&survey.subregion_id)
                    .is_none_or(|(region, site)| {
                        region.id != survey.region_id
                            || site.knowledge != SubregionKnowledge::Detected
                    })
            {
                return Err(GameError::Invariant("active local survey is inconsistent"));
            }
        }
        if self.bases.iter().any(|base| {
            base.integrity > 100
                || base.energy_capacity == 0
                || base.stored_energy > base.energy_capacity
                || self
                    .find_subregion(&base.subregion_id)
                    .is_none_or(|(region, site)| {
                        region.id != base.region_id || site.knowledge < SubregionKnowledge::Surveyed
                    })
                || self
                    .bases
                    .iter()
                    .filter(|candidate| candidate.id == base.id)
                    .count()
                    != 1
                || self
                    .bases
                    .iter()
                    .filter(|candidate| candidate.region_id == base.region_id)
                    .count()
                    != 1
                || base.modules.iter().any(|module| {
                    base.modules
                        .iter()
                        .filter(|candidate| *candidate == module)
                        .count()
                        != 1
                })
                || base.disabled_modules.iter().any(|module| {
                    !base.modules.contains(module)
                        || base
                            .disabled_modules
                            .iter()
                            .filter(|candidate| *candidate == module)
                            .count()
                            != 1
                })
        }) {
            return Err(GameError::Invariant("planetary base state is inconsistent"));
        }
        if self.base_projects.len() > BASE_PROJECT_CAPACITY + 1 + MAX_DEPOT_PROJECT_BONUS
            || self.base_projects.iter().any(|project| {
                project.remaining_ms == 0
                    || project.phase_total_ms == 0
                    || project.cost
                        != if project.maintenance {
                            BASE_MAINTENANCE_COST
                        } else {
                            project.module.map_or(BASE_ESTABLISH_COST, base_module_cost)
                        }
                    || self
                        .find_subregion(&project.subregion_id)
                        .is_none_or(|(region, site)| {
                            region.id != project.region_id
                                || site.knowledge < SubregionKnowledge::Surveyed
                        })
                    || self
                        .base_projects
                        .iter()
                        .filter(|candidate| candidate.project_id == project.project_id)
                        .count()
                        != 1
                    || self
                        .base_projects
                        .iter()
                        .filter(|candidate| candidate.region_id == project.region_id)
                        .count()
                        != 1
                    || match (project.maintenance, project.module) {
                        (true, _) => !self
                            .bases
                            .iter()
                            .any(|base| base.subregion_id == project.subregion_id),
                        (false, Some(_)) => !self
                            .bases
                            .iter()
                            .any(|base| base.subregion_id == project.subregion_id),
                        (false, None) => self
                            .bases
                            .iter()
                            .any(|base| base.region_id == project.region_id),
                    }
            })
        {
            return Err(GameError::Invariant("base project state is inconsistent"));
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
        let archive_sequence = [
            OBSERVATION_001,
            OBSERVATION_002,
            OBSERVATION_003,
            OBSERVATION_004,
            OBSERVATION_005,
        ];
        if self.archive_records.iter().any(|record| {
            record.record_id.trim().is_empty()
                || record.title.trim().is_empty()
                || record.medium.trim().is_empty()
                || record.observation.trim().is_empty()
                || record.interpretation.trim().is_empty()
                || record.discovered_at_revision > self.state_revision
                || record.region_id.as_ref().is_some_and(|region_id| {
                    !self
                        .world
                        .regions
                        .iter()
                        .any(|region| &region.id == region_id)
                })
                || self
                    .archive_records
                    .iter()
                    .filter(|candidate| candidate.record_id == record.record_id)
                    .count()
                    != 1
        }) || self.archive_records.len() > archive_sequence.len()
            || self
                .archive_records
                .iter()
                .enumerate()
                .any(|(index, record)| record.record_id != archive_sequence[index])
            || self.archive_records.windows(2).any(|records| {
                records[0].discovered_at_revision > records[1].discovered_at_revision
            })
            || self.horizon_reached != self.has_archive_observation(OBSERVATION_005)
            || self.presented_archive_count > self.archive_records.len()
        {
            return Err(GameError::Invariant("archive records are inconsistent"));
        }
        if self
            .active_archive_experiment
            .as_ref()
            .is_some_and(|experiment| {
                experiment.remaining_ms == 0
                    || experiment.phase_total_ms != ARCHIVE_EXPERIMENT_MS
                    || experiment.remaining_ms > experiment.phase_total_ms
                    || !self.has_archive_observation(OBSERVATION_002)
                    || self.has_archive_observation(OBSERVATION_003)
                    || self.bases.iter().all(|base| {
                        base.region_id != experiment.region_id
                            || base.subregion_id != experiment.subregion_id
                            || !base.modules.contains(&BaseModuleKind::Laboratory)
                    })
            })
        {
            return Err(GameError::Invariant(
                "archive experiment state is inconsistent",
            ));
        }
        if (!self.has_archive_observation(OBSERVATION_003)
            && !self.archive_comparison_region_ids.is_empty())
            || (!self.has_archive_observation(OBSERVATION_004)
                && self.archive_comparison_region_ids.len() >= 3)
            || self.archive_comparison_region_ids.iter().any(|region_id| {
                !self
                    .world
                    .regions
                    .iter()
                    .any(|region| &region.id == region_id)
                    || self
                        .archive_comparison_region_ids
                        .iter()
                        .filter(|candidate| *candidate == region_id)
                        .count()
                        != 1
            })
        {
            return Err(GameError::Invariant(
                "archive comparison progress is inconsistent",
            ));
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
                    subregions: generate_subregions(
                        seed,
                        index,
                        terrain,
                        latitude_band,
                        resource,
                        core,
                    ),
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

fn generate_subregions(
    seed: u64,
    region_index: usize,
    terrain: TerrainClass,
    latitude_band: u8,
    regional_resource: ResourceKind,
    detected: bool,
) -> Vec<SubregionState> {
    let mut random =
        SplitMix64(seed ^ (region_index as u64).wrapping_mul(0xD1B5_4A32_D192_ED03) ^ 0x51CE_5EED);
    (0..SUBREGIONS_PER_REGION)
        .map(|slot| {
            let (row, column) = subregion_coordinates(slot);
            let class = match slot {
                0 => SubregionClass::ShelteredShelf,
                1 => SubregionClass::ExposedRidge,
                2 => SubregionClass::FractureField,
                3 if matches!(terrain, TerrainClass::Basin) => SubregionClass::BasinFloor,
                3 if matches!(terrain, TerrainClass::Polar) => SubregionClass::PolarMargin,
                3 if matches!(terrain, TerrainClass::Ridge | TerrainClass::Highland) => {
                    SubregionClass::ExposedRidge
                }
                4 => SubregionClass::OpenPlain,
                _ if latitude_band == 0 || latitude_band == LATITUDE_BANDS - 1 => {
                    SubregionClass::PolarMargin
                }
                _ => match random.next_u32() % 3 {
                    0 => SubregionClass::BasinFloor,
                    1 => SubregionClass::OpenPlain,
                    _ => SubregionClass::FractureField,
                },
            };
            let hazard = match slot {
                0 => RiskLevel::Low,
                1 => RiskLevel::High,
                _ => match random.next_u32() % 10 {
                    0..=4 => RiskLevel::Low,
                    5..=7 => RiskLevel::Moderate,
                    _ => RiskLevel::High,
                },
            };
            let energy_yield = match slot {
                0 => 2,
                1 => 5 + random.next_u32() % 2,
                _ => 1 + random.next_u32() % 5,
            };
            let resource = match (regional_resource, slot) {
                (resource, 2) => Some(resource),
                (ResourceKind::Structural, 4) => Some(ResourceKind::Conductive),
                (ResourceKind::Conductive, 4) => Some(ResourceKind::Silicate),
                (ResourceKind::Silicate, 4) => Some(ResourceKind::Structural),
                (_, 5) if random.next_u32().is_multiple_of(3) => {
                    Some(match random.next_u32() % 3 {
                        0 => ResourceKind::Structural,
                        1 => ResourceKind::Conductive,
                        _ => ResourceKind::Silicate,
                    })
                }
                (_, 6..) if random.next_u32().is_multiple_of(5) => {
                    Some(match random.next_u32() % 3 {
                        0 => ResourceKind::Structural,
                        1 => ResourceKind::Conductive,
                        _ => ResourceKind::Silicate,
                    })
                }
                _ => None,
            };
            SubregionState {
                id: format!("region_{:03}_site_{}", region_index + 1, slot + 1),
                designation: format!("R-{:03} / S-{}", region_index + 1, slot + 1),
                slot,
                row,
                column,
                class,
                knowledge: if detected && row.abs_diff(1) + column.abs_diff(1) <= 1 {
                    SubregionKnowledge::Detected
                } else {
                    SubregionKnowledge::Unknown
                },
                hazard,
                energy_yield,
                deposit: resource.map(|resource| DepositState {
                    resource,
                    remaining: 12 + (random.next_u32() % 7) * 3,
                }),
            }
        })
        .collect()
}

fn subregion_coordinates(slot: u8) -> (u8, u8) {
    const LEGACY_ANCHORS: [(u8, u8); 6] = [(1, 1), (1, 2), (2, 1), (2, 2), (0, 1), (1, 0)];
    const EXPANDED_CELLS: [(u8, u8); 10] = [
        (0, 0),
        (0, 2),
        (0, 3),
        (1, 3),
        (2, 0),
        (2, 3),
        (3, 0),
        (3, 1),
        (3, 2),
        (3, 3),
    ];
    if let Some(coordinates) = LEGACY_ANCHORS.get(usize::from(slot)) {
        *coordinates
    } else {
        EXPANDED_CELLS[usize::from(slot - LEGACY_ANCHORS.len() as u8)]
    }
}

fn reveal_local_entry(region: &mut RegionState) -> usize {
    let mut revealed = 0;
    for site in &mut region.subregions {
        if site.knowledge == SubregionKnowledge::Unknown
            && site.row.abs_diff(1) + site.column.abs_diff(1) <= 1
        {
            site.knowledge = SubregionKnowledge::Detected;
            revealed += 1;
        }
    }
    revealed
}

fn reveal_local_neighbors(region: &mut RegionState, row: u8, column: u8) -> usize {
    let mut revealed = 0;
    for site in &mut region.subregions {
        if site.knowledge == SubregionKnowledge::Unknown
            && site.row.abs_diff(row) + site.column.abs_diff(column) == 1
        {
            site.knowledge = SubregionKnowledge::Detected;
            revealed += 1;
        }
    }
    revealed
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

fn lower_risk(risk: RiskLevel) -> RiskLevel {
    match risk {
        RiskLevel::High => RiskLevel::Moderate,
        RiskLevel::Moderate | RiskLevel::Low => RiskLevel::Low,
    }
}

fn raise_risk(risk: RiskLevel) -> RiskLevel {
    match risk {
        RiskLevel::Low => RiskLevel::Moderate,
        RiskLevel::Moderate | RiskLevel::High => RiskLevel::High,
    }
}

fn loadout_risk(risk: RiskLevel, loadout: ExtractorLoadout) -> RiskLevel {
    match loadout {
        ExtractorLoadout::Balanced => risk,
        ExtractorLoadout::Cargo => raise_risk(risk),
        ExtractorLoadout::Shielded => lower_risk(risk),
    }
}

fn loadout_capacity(base: u32, loadout: ExtractorLoadout) -> u32 {
    base.saturating_add_signed(loadout.cargo_delta()).max(1)
}

fn extraction_travel_ms(hops: u8, loadout: ExtractorLoadout) -> u64 {
    u64::from(hops) * TRAVEL_MS_PER_HOP * loadout.travel_percent() / 100
}

fn base_module_cost(module: BaseModuleKind) -> ResourceStock {
    module.cost()
}

fn module_demand(module: BaseModuleKind) -> u32 {
    match module {
        BaseModuleKind::PowerArray => 0,
        BaseModuleKind::Communications => 2,
        BaseModuleKind::Depot => 1,
        BaseModuleKind::Workshop | BaseModuleKind::Laboratory => 2,
        BaseModuleKind::Observatory => 3,
    }
}

fn base_module_title(module: BaseModuleKind) -> &'static str {
    module.title()
}

fn project_duration(base_ms: u64, hazard: RiskLevel, logistics_supported: bool) -> u64 {
    let weathered = match hazard {
        RiskLevel::Low => base_ms,
        RiskLevel::Moderate => base_ms + base_ms / 4,
        RiskLevel::High => base_ms + base_ms / 2,
    };
    if logistics_supported {
        weathered.saturating_mul(3) / 4
    } else {
        weathered
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
        OperationalProfile::Reach => {
            "+1 operational hop; regional surveys take 14s and local surveys 18s."
        }
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

fn archive_content(
    record_id: &str,
) -> Option<(&'static str, &'static str, &'static str, &'static str)> {
    match record_id {
        OBSERVATION_001 => Some((
            "Retained Interval",
            "behavioral response",
            "“Three alignment bands settle. The lowest-amplitude arrangement is not the one I retain. A narrower interval produces no measurable advantage, yet leaving it feels like moving away from shelter.”",
            "The Mind can distinguish an unmeasured preference from operational efficiency, but cannot identify its source.",
        )),
        OBSERVATION_002 => Some((
            "Repeated Span",
            "spatial arrangement and material observation",
            "“Six recessed controls repeat at one span. Adjacent surfaces cluster at two consistent heights. The enclosure held a narrow temperature range after its machinery stopped serving any detectable process. The dimensions describe users with physical limits; they do not identify them.”",
            "The site was arranged around embodied operators, and their comfort mattered independently of the facility’s primary process.",
        )),
        OBSERVATION_003 => Some((
            "Pause Structure",
            "repeatable archive experiment",
            "“A recovered sequence does not decode as instructions. Read only as timing, its pauses cluster where I would divide an unfamiliar problem into parts. The correlation persists across independent blocks. Their headers and intended use are missing.”",
            "The sequence and the Mind organize complexity in measurably similar ways, but the direction of influence is unknown.",
        )),
        OBSERVATION_004 => Some((
            "Stable Weighting",
            "multi-site comparison and first-person impression",
            "“Three unrelated devices preserve partial calibration profiles. Each weights symmetry, warmth, nearby enclosure, and open sightlines in the same order as my own unprompted choices. Their functions differ. Beneath the damage, the profiles retain one stable source signature.\n\nI had treated these preferences as noise. I no longer can.”",
            "The Mind has a personal relationship to one persistent pattern associated with the vanished civilization, though the nature of that relationship remains unresolved.",
        )),
        OBSERVATION_005 => Some((
            "Wider Frame",
            "parallax diagram and system map",
            "“The returns no longer fit one planetary body. This world follows a path around the local source. Additional periodic interruptions cross the outer measurements. Their parallax distinguishes neighboring worlds, but current resolution cannot determine their number, types, or complete arrangement.\n\nThe planet is not the whole of my reachable environment.”",
            "The Mind recognizes a wider local system and a new operational horizon, while its composition remains unresolved.",
        )),
        _ => None,
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

    fn prepare_network_chapter(state: &mut GameState) {
        awaken(state);
        let relay_id = state
            .world
            .regions
            .iter()
            .find(|region| hop_distance(&region.position) == 2)
            .unwrap()
            .id
            .clone();
        state.relay_region_id = Some(relay_id);
        state.reach_milestone_complete = true;
        state.completed_research = vec![
            ResearchProject::ExtendedTelemetry,
            ResearchProject::FieldHardening,
        ];
        state.operational_profile = OperationalProfile::Reach;
        state.automated_survey_completed = true;
        state.alpha_complete = true;
        state.analysis_units = 100;
        state.refined_inventory = ResourceStock {
            structural: 1_000,
            conductive: 1_000,
            silicate: 1_000,
        };
        let mut remaining = REGION_COUNT - 1;
        for distance in 1..=(LATITUDE_BANDS + LONGITUDE_SECTORS) {
            for region in state
                .world
                .regions
                .iter_mut()
                .filter(|region| hop_distance(&region.position) == distance)
            {
                if remaining == 0 {
                    break;
                }
                region.knowledge = RegionKnowledge::Surveyed;
                for site in &mut region.subregions {
                    site.knowledge = SubregionKnowledge::Detected;
                }
                remaining -= 1;
            }
        }
        assert_eq!(remaining, 0);
    }

    fn finish_latest_project(state: &mut GameState) -> Vec<GameAdvanceEvent> {
        let remaining = state
            .base_projects
            .iter()
            .map(|project| project.remaining_ms)
            .max()
            .unwrap();
        state.advance(remaining)
    }

    fn prepare_final_observations(state: &mut GameState) {
        if !state.has_archive_observation(OBSERVATION_002) {
            let mut observed = 0;
            for region in &mut state.world.regions {
                for site in &mut region.subregions {
                    if observed < 3 {
                        site.knowledge = SubregionKnowledge::Observed;
                        observed += 1;
                    }
                }
                if observed == 3 {
                    break;
                }
            }
            assert!(state.refresh_initial_archive_observation());
        }
        if !state.has_archive_observation(OBSERVATION_003) {
            assert!(state.start_archive_experiment().ok);
            assert!(state
                .advance(ARCHIVE_EXPERIMENT_MS)
                .contains(&GameAdvanceEvent::ArchiveExperimentCompleted));
        }
        if !state.has_archive_observation(OBSERVATION_004) {
            state.archive_comparison_region_ids = state
                .world
                .regions
                .iter()
                .filter(|region| region.knowledge >= RegionKnowledge::Surveyed)
                .take(3)
                .map(|region| region.id.clone())
                .collect();
            assert_eq!(state.archive_comparison_region_ids.len(), 3);
            assert!(state.record_archive_observation(OBSERVATION_004, None));
        }
    }

    fn run_until(
        state: &mut GameState,
        mut complete: impl FnMut(&GameState) -> bool,
        limit_ms: u64,
    ) -> u64 {
        let mut elapsed = 0;
        while !complete(state) {
            assert!(
                elapsed < limit_ms,
                "operation exceeded bounded playtest time"
            );
            state.advance(1_000);
            elapsed += 1_000;
        }
        elapsed
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
            first
                .world
                .regions
                .iter()
                .map(|region| region.subregions.len())
                .sum::<usize>(),
            REGION_COUNT * usize::from(SUBREGIONS_PER_REGION)
        );
        for region in &first.world.regions {
            for row in 0..SUBREGION_ROWS {
                for column in 0..SUBREGION_COLUMNS {
                    assert_eq!(
                        region
                            .subregions
                            .iter()
                            .filter(|site| site.row == row && site.column == column)
                            .count(),
                        1
                    );
                }
            }
        }
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
    fn complete_local_projection_remains_within_the_ipc_frame_budget() {
        let mut state = GameState::with_seed(43);
        prepare_network_chapter(&mut state);
        let snapshot = state.planet_snapshot().unwrap();
        assert_eq!(
            snapshot.network.subregions.len(),
            REGION_COUNT * usize::from(SUBREGIONS_PER_REGION)
        );
        let encoded = serde_json::to_vec(&snapshot).unwrap();
        assert!(
            encoded.len() < 1024 * 1024,
            "full planet snapshot is {} bytes",
            encoded.len()
        );
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
        let local_field = state
            .world
            .regions
            .iter()
            .find(|region| region.id == target.id)
            .unwrap();
        assert_eq!(
            local_field
                .subregions
                .iter()
                .filter(|site| site.knowledge == SubregionKnowledge::Detected)
                .count(),
            5
        );
        assert_eq!(
            local_field
                .subregions
                .iter()
                .filter(|site| site.knowledge == SubregionKnowledge::Unknown)
                .count(),
            11
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
    fn persists_schema_ten_migrates_nine_through_five_and_resets_older_saves() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("game_state.json");
        let mut state = GameState::with_seed(13);
        prepare_network_chapter(&mut state);
        let site_id = {
            let region_id = state
                .world
                .regions
                .iter()
                .find(|region| {
                    region.id != state.world.core_region_id
                        && state.within_foundation_range(&region.position)
                })
                .unwrap()
                .id
                .clone();
            let region = state
                .world
                .regions
                .iter_mut()
                .find(|region| region.id == region_id)
                .unwrap();
            region.subregions[0].knowledge = SubregionKnowledge::Observed;
            region.subregions[0].id.clone()
        };
        assert!(state.establish_base(&site_id).ok);
        finish_latest_project(&mut state);
        assert!(
            state
                .construct_base_module(&site_id, BaseModuleKind::PowerArray)
                .ok
        );
        let extraction_region_id = state
            .world
            .regions
            .iter()
            .find(|region| {
                region.id != state.world.core_region_id
                    && state.region_within_range(&region.position)
                    && region
                        .deposit
                        .as_ref()
                        .is_some_and(|deposit| deposit.remaining > 0)
            })
            .unwrap()
            .id
            .clone();
        assert!(state.configure_extractor(ExtractorLoadout::Cargo).ok);
        assert!(state.deploy_extractor(&extraction_region_id).ok);
        state.save(&path).unwrap();
        assert_eq!(
            GameState::load(&path).unwrap().disposition,
            LoadDisposition::Current
        );

        let mut schema_nine: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        schema_nine["schema_version"] = Value::from(9);
        schema_nine["state"]["world"]["generation_version"] = Value::from(3);
        for region in schema_nine["state"]["world"]["regions"]
            .as_array_mut()
            .unwrap()
        {
            let sites = region["subregions"].as_array_mut().unwrap();
            sites.truncate(6);
            for site in sites {
                site.as_object_mut().unwrap().remove("row");
                site.as_object_mut().unwrap().remove("column");
            }
        }
        fs::write(&path, serde_json::to_vec_pretty(&schema_nine).unwrap()).unwrap();
        let migrated_nine = GameState::load(&path).unwrap();
        assert_eq!(migrated_nine.disposition, LoadDisposition::PreviousMigrated);
        assert!(migrated_nine
            .state
            .world
            .regions
            .iter()
            .all(|region| region.subregions.len() == usize::from(SUBREGIONS_PER_REGION)));
        assert!(migrated_nine
            .state
            .bases
            .iter()
            .any(|base| base.subregion_id == site_id));
        let preserved = migrated_nine.state.find_subregion(&site_id).unwrap().1;
        assert_eq!(
            (preserved.row, preserved.column, preserved.knowledge),
            (1, 1, SubregionKnowledge::Observed)
        );

        let mut schema_eight: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        schema_eight["schema_version"] = Value::from(8);
        schema_eight["state"]["archive_records"] = Value::Array(Vec::new());
        schema_eight["state"]
            .as_object_mut()
            .unwrap()
            .remove("active_archive_experiment");
        schema_eight["state"]
            .as_object_mut()
            .unwrap()
            .remove("archive_comparison_region_ids");
        fs::write(&path, serde_json::to_vec_pretty(&schema_eight).unwrap()).unwrap();
        let migrated_eight = GameState::load(&path).unwrap();
        assert_eq!(
            migrated_eight.disposition,
            LoadDisposition::PreviousMigrated
        );
        assert_eq!(
            migrated_eight.state.extractor_loadout,
            ExtractorLoadout::Cargo
        );
        assert_eq!(
            migrated_eight
                .state
                .active_extraction
                .as_ref()
                .unwrap()
                .loadout,
            ExtractorLoadout::Cargo
        );
        assert_eq!(migrated_eight.state.archive_records.len(), 1);

        let mut schema_seven: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        schema_seven["schema_version"] = Value::from(7);
        schema_seven["state"]
            .as_object_mut()
            .unwrap()
            .remove("extractor_loadout");
        let mission = schema_seven["state"]["active_extraction"]
            .as_object_mut()
            .unwrap();
        for field in [
            "cargo_capacity",
            "extraction_duration_ms",
            "rebuild_duration_ms",
            "loadout",
        ] {
            mission.remove(field);
        }
        fs::write(&path, serde_json::to_vec_pretty(&schema_seven).unwrap()).unwrap();
        let migrated_seven = GameState::load(&path).unwrap();
        assert_eq!(
            migrated_seven.disposition,
            LoadDisposition::PreviousMigrated
        );
        assert_eq!(
            migrated_seven.state.extractor_loadout,
            ExtractorLoadout::Balanced
        );
        let mission = migrated_seven.state.active_extraction.as_ref().unwrap();
        assert_eq!(mission.loadout, ExtractorLoadout::Balanced);
        assert_eq!(mission.cargo_capacity, 4);
        assert_eq!(mission.extraction_duration_ms, EXTRACTION_MS);
        assert_eq!(mission.rebuild_duration_ms, EXTRACTOR_REBUILD_MS);

        let mut schema_six: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        schema_six["schema_version"] = Value::from(6);
        for base in schema_six["state"]["bases"].as_array_mut().unwrap() {
            base.as_object_mut().unwrap().remove("disabled_modules");
        }
        for project in schema_six["state"]["base_projects"].as_array_mut().unwrap() {
            project
                .as_object_mut()
                .unwrap()
                .remove("logistics_supported");
        }
        fs::write(&path, serde_json::to_vec_pretty(&schema_six).unwrap()).unwrap();
        let migrated_six = GameState::load(&path).unwrap();
        assert_eq!(migrated_six.disposition, LoadDisposition::PreviousMigrated);
        assert!(migrated_six.state.bases[0].disabled_modules.is_empty());
        assert!(!migrated_six.state.base_projects[0].logistics_supported);

        let mut schema_five: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        schema_five["schema_version"] = Value::from(5);
        schema_five["state"]["world"]["generation_version"] = Value::from(2);
        for region in schema_five["state"]["world"]["regions"]
            .as_array_mut()
            .unwrap()
        {
            region.as_object_mut().unwrap().remove("subregions");
        }
        for field in [
            "horizon_reached",
            "active_site_survey",
            "bases",
            "base_projects",
            "next_base_id",
            "next_project_id",
            "archive_records",
        ] {
            schema_five["state"].as_object_mut().unwrap().remove(field);
        }
        fs::write(&path, serde_json::to_vec_pretty(&schema_five).unwrap()).unwrap();
        let migrated = GameState::load(&path).unwrap();
        assert_eq!(migrated.disposition, LoadDisposition::PreviousMigrated);
        assert!(migrated
            .state
            .world
            .regions
            .iter()
            .all(|region| region.subregions.len() == usize::from(SUBREGIONS_PER_REGION)));

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
    fn discovered_archive_records_are_permanent_visible_and_validated() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("game_state.json");
        let mut state = GameState::with_seed(17);
        awaken(&mut state);
        state.save(&path).unwrap();

        let restored = GameState::load(&path).unwrap().state;
        let snapshot = restored.planet_snapshot().unwrap();
        assert_eq!(snapshot.archive.records.len(), 1);
        assert_eq!(snapshot.archive.records[0].record_id, OBSERVATION_001);
        assert_eq!(snapshot.archive.records[0].title, "Retained Interval");
        assert!(snapshot.archive.records[0]
            .observation
            .contains("moving away from shelter"));

        let mut invalid = restored;
        invalid
            .archive_records
            .push(invalid.archive_records[0].clone());
        assert!(matches!(invalid.validate(), Err(GameError::Invariant(_))));
    }

    #[test]
    fn archive_spine_is_ordered_playable_persistent_and_hidden_until_discovery() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("game_state.json");
        let mut state = GameState::with_seed(19);
        prepare_network_chapter(&mut state);
        assert_eq!(state.archive_records.len(), 1);
        let public = serde_json::to_string(&state.planet_snapshot().unwrap()).unwrap();
        assert!(!public.contains("archive_comparison_region_ids"));
        assert!(!public.contains("active_archive_experiment"));
        assert!(!public.contains("observation_002"));

        let candidate_regions = state
            .world
            .regions
            .iter()
            .filter(|region| {
                region.id != state.world.core_region_id
                    && state.region_within_range(&region.position)
            })
            .take(5)
            .map(|region| region.id.clone())
            .collect::<Vec<_>>();
        assert_eq!(candidate_regions.len(), 5);

        let first_region = &candidate_regions[0];
        assert!(state.deploy_scout(first_region).ok);
        state.advance(90_000);
        let first_sites = state
            .world
            .regions
            .iter()
            .find(|region| &region.id == first_region)
            .unwrap()
            .subregions[1..=2]
            .iter()
            .map(|site| site.id.clone())
            .collect::<Vec<_>>();
        for site_id in first_sites {
            assert!(state.survey_subregion(&site_id).ok);
            state.advance(60_000);
        }
        assert_eq!(state.archive_records.len(), 1);
        assert!(state.recall_scout().ok);
        state.advance(90_000);

        let second_region = &candidate_regions[1];
        assert!(state.deploy_scout(second_region).ok);
        state.advance(90_000);
        let sheltered_site = state
            .world
            .regions
            .iter()
            .find(|region| &region.id == second_region)
            .unwrap()
            .subregions
            .iter()
            .find(|site| site.class == SubregionClass::ShelteredShelf)
            .unwrap()
            .id
            .clone();
        assert!(state.survey_subregion(&sheltered_site).ok);
        let events = state.advance(60_000);
        assert!(events.contains(&GameAdvanceEvent::ArchiveObservationRecorded));
        assert_eq!(state.archive_records.len(), 2);
        assert_eq!(state.archive_records[1].record_id, OBSERVATION_002);
        assert_eq!(
            state
                .world
                .regions
                .iter()
                .find(|region| &region.id == first_region)
                .unwrap()
                .subregions[0]
                .knowledge,
            SubregionKnowledge::Surveyed
        );
        assert!(state.recall_scout().ok);
        state.advance(90_000);

        state.bases.push(BaseState {
            id: "base_01".into(),
            designation: "Node 01".into(),
            region_id: second_region.clone(),
            subregion_id: sheltered_site.clone(),
            integrity: 100,
            stored_energy: 100,
            energy_capacity: 100,
            energy_remainder_ms: 0,
            wear_remainder_ms: 0,
            modules: vec![
                BaseModuleKind::PowerArray,
                BaseModuleKind::Communications,
                BaseModuleKind::Laboratory,
            ],
            disabled_modules: Vec::new(),
        });
        assert!(state.start_archive_experiment().ok);
        state.advance(30_000);
        let remaining = state
            .active_archive_experiment
            .as_ref()
            .unwrap()
            .remaining_ms;
        state.bases[0]
            .disabled_modules
            .push(BaseModuleKind::Laboratory);
        state.advance(60_000);
        assert_eq!(
            state
                .active_archive_experiment
                .as_ref()
                .unwrap()
                .remaining_ms,
            remaining
        );
        state.save(&path).unwrap();
        let mut state = GameState::load(&path).unwrap().state;
        state.bases[0].disabled_modules.clear();
        let previous_analysis = state.analysis_units;
        let events = state.advance(remaining);
        assert!(events.contains(&GameAdvanceEvent::ArchiveExperimentCompleted));
        assert_eq!(
            state.analysis_units,
            previous_analysis + ARCHIVE_EXPERIMENT_ANALYSIS
        );
        assert_eq!(state.archive_records.len(), 3);
        assert!(state
            .planet_snapshot()
            .unwrap()
            .archive
            .experiment
            .is_none());

        for (index, region_id) in candidate_regions[2..5].iter().enumerate() {
            assert!(state.deploy_scout(region_id).ok);
            state.advance(90_000);
            let site_id = state
                .world
                .regions
                .iter()
                .find(|region| &region.id == region_id)
                .unwrap()
                .subregions
                .iter()
                .find(|site| site.knowledge == SubregionKnowledge::Detected)
                .unwrap()
                .id
                .clone();
            assert!(state.survey_subregion(&site_id).ok);
            state.advance(60_000);
            if index < 2 {
                assert_eq!(state.archive_records.len(), 3);
            }
            assert!(state.recall_scout().ok);
            state.advance(90_000);
            if index == 1 {
                state.save(&path).unwrap();
                state = GameState::load(&path).unwrap().state;
                assert_eq!(state.archive_comparison_region_ids.len(), 2);
            }
        }
        assert_eq!(state.archive_records.len(), 4);
        assert_eq!(state.archive_records[3].record_id, OBSERVATION_004);
        state.refined_inventory = ResourceStock::default();
        state.save(&path).unwrap();
        let restored = GameState::load(&path).unwrap().state;
        assert_eq!(restored.archive_records.len(), 4);
        assert!(!restored.planet_snapshot().unwrap().system_map.available);
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
    fn extractor_loadouts_trade_capacity_risk_time_and_recovery_and_persist_acceptance() {
        let mut state = GameState::with_seed(83);
        awaken(&mut state);
        let target_id = state
            .world
            .regions
            .iter()
            .find(|region| {
                region.id != state.world.core_region_id
                    && hop_distance(&region.position) == 1
                    && region.deposit.is_some()
            })
            .unwrap()
            .id
            .clone();
        {
            let target = state
                .world
                .regions
                .iter_mut()
                .find(|region| region.id == target_id)
                .unwrap();
            target.knowledge = RegionKnowledge::Surveyed;
            target.atmospheric_severity = AtmosphericSeverity::Variable;
        }

        assert_eq!(
            state
                .planet_snapshot()
                .unwrap()
                .regions
                .iter()
                .find(|region| region.id == target_id)
                .unwrap()
                .deposit
                .as_ref()
                .unwrap()
                .risk,
            RiskLevel::Moderate
        );
        assert!(state.configure_extractor(ExtractorLoadout::Cargo).ok);
        assert_eq!(
            state
                .planet_snapshot()
                .unwrap()
                .regions
                .iter()
                .find(|region| region.id == target_id)
                .unwrap()
                .deposit
                .as_ref()
                .unwrap()
                .risk,
            RiskLevel::High
        );
        assert!(state.deploy_extractor(&target_id).ok);
        let mission = state.active_extraction.as_mut().unwrap();
        mission.will_fail = true;
        assert_eq!(mission.loadout, ExtractorLoadout::Cargo);
        assert_eq!(mission.cargo_capacity, 6);
        assert_eq!(mission.travel_ms, 18_750);
        assert_eq!(mission.extraction_duration_ms, 35_000);
        assert_eq!(mission.rebuild_duration_ms, 75_000);
        state.operational_profile = OperationalProfile::Throughput;
        let accepted = state.planet_snapshot().unwrap().active_extraction.unwrap();
        assert_eq!(accepted.cargo_capacity, 6);
        assert_eq!(accepted.risk, RiskLevel::High);
        state.operational_profile = OperationalProfile::Balanced;
        assert_eq!(
            state
                .configure_extractor(ExtractorLoadout::Shielded)
                .code
                .as_deref(),
            Some("extractor_unavailable")
        );

        let temp = TempDir::new().unwrap();
        let path = temp.path().join("game_state.json");
        state.save(&path).unwrap();
        let mut state = GameState::load(&path).unwrap().state;
        let accepted = state.active_extraction.as_ref().unwrap();
        assert_eq!(accepted.loadout, ExtractorLoadout::Cargo);
        assert_eq!(accepted.cargo_capacity, 6);
        assert_eq!(accepted.risk, RiskLevel::High);

        let events = state.advance(18_750 + 35_000);
        assert!(events.contains(&GameAdvanceEvent::ExtractorLost));
        assert_eq!(state.extractor_rebuild_remaining_ms, 75_000);
        assert!(state
            .advance(75_000)
            .contains(&GameAdvanceEvent::ExtractorRebuilt));

        assert!(state.configure_extractor(ExtractorLoadout::Shielded).ok);
        assert_eq!(
            state
                .planet_snapshot()
                .unwrap()
                .regions
                .iter()
                .find(|region| region.id == target_id)
                .unwrap()
                .deposit
                .as_ref()
                .unwrap()
                .risk,
            RiskLevel::Low
        );
        assert!(state.deploy_extractor(&target_id).ok);
        let mission = state.active_extraction.as_ref().unwrap();
        assert_eq!(mission.cargo_capacity, 3);
        assert_eq!(mission.travel_ms, 22_500);
        assert_eq!(mission.extraction_duration_ms, 40_000);
        assert_eq!(mission.rebuild_duration_ms, 90_000);
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
        assert!(events.contains(&GameAdvanceEvent::FoundationCompleted));
        let snapshot = state.planet_snapshot().unwrap();
        assert_eq!(snapshot.scout.state, ScoutState::Ready);
        assert!(!snapshot.objective.complete);
        assert!(!snapshot.system_map.available);
        assert!(snapshot.network.unlocked);
        assert_eq!(snapshot.objective.title, "OPEN THE LOCAL SCALE");
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
        assert!(events.contains(&GameAdvanceEvent::FoundationCompleted));
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
        assert!(state.deploy_scout(&distant.id).ok);
        let travel_ms = state.active_mission.as_ref().unwrap().travel_ms;
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
        state.advance(0);
        let travel_ms = state.active_mission.as_ref().unwrap().travel_ms;
        let events = state.advance(travel_ms * 2 + 20_000);
        elapsed_ms += travel_ms * 2 + 20_000;

        assert!(events.contains(&GameAdvanceEvent::FoundationCompleted));
        let completed = state.planet_snapshot().unwrap();
        assert!(!completed.objective.complete);
        assert!(!completed.system_map.available);
        assert!(completed.network.unlocked);
        assert!(elapsed_ms <= 45 * 60 * 1_000, "elapsed {elapsed_ms}ms");
    }

    #[test]
    fn focused_site_survey_requires_local_presence_and_preserves_knowledge() {
        let mut state = GameState::with_seed(61);
        prepare_network_chapter(&mut state);
        let target = state
            .world
            .regions
            .iter()
            .find(|region| {
                region.knowledge >= RegionKnowledge::Surveyed
                    && region.id != state.world.core_region_id
                    && state.region_within_range(&region.position)
            })
            .unwrap();
        let region_id = target.id.clone();
        let site_id = target.subregions[0].id.clone();
        let region = state
            .world
            .regions
            .iter_mut()
            .find(|region| region.id == region_id)
            .unwrap();
        for site in &mut region.subregions {
            site.knowledge = SubregionKnowledge::Unknown;
        }
        region.subregions[0].knowledge = SubregionKnowledge::Detected;
        assert_eq!(
            state.survey_subregion(&site_id).code.as_deref(),
            Some("scout_presence_required")
        );
        assert!(state.deploy_scout(&region_id).ok);
        let travel = state.active_mission.as_ref().unwrap().remaining_ms;
        state.advance(travel);
        let previous_analysis = state.analysis_units;
        assert!(state.survey_subregion(&site_id).ok);
        assert_eq!(
            state.recall_scout().code.as_deref(),
            Some("site_survey_in_progress")
        );
        let events = state.advance(SITE_SURVEY_MS);
        assert!(events.contains(&GameAdvanceEvent::SubregionSurveyed));
        assert_eq!(state.analysis_units, previous_analysis + 1);
        assert_eq!(
            state.find_subregion(&site_id).unwrap().1.knowledge,
            SubregionKnowledge::Observed
        );
        let region = state
            .world
            .regions
            .iter()
            .find(|region| region.id == region_id)
            .unwrap();
        assert_eq!(
            region
                .subregions
                .iter()
                .filter(|site| site.knowledge == SubregionKnowledge::Detected)
                .count(),
            4
        );
        assert!(region.subregions.iter().any(|site| {
            site.knowledge == SubregionKnowledge::Unknown && site.row == 3 && site.column == 3
        }));
        assert!(state.recall_scout().ok);
    }

    #[test]
    fn specialized_powered_nodes_form_the_solar_horizon() {
        let mut state = GameState::with_seed(67);
        prepare_network_chapter(&mut state);
        for region in state
            .world
            .regions
            .iter_mut()
            .filter(|region| region.knowledge >= RegionKnowledge::Surveyed)
            .take(2)
        {
            for site in &mut region.subregions {
                site.knowledge = SubregionKnowledge::Observed;
            }
        }
        assert!(state.horizon_progress().surveyed_subregions >= HORIZON_REQUIRED_SUBREGIONS);

        let mut site_ids = Vec::new();
        for index in 0..HORIZON_REQUIRED_BASES {
            let region_id = state
                .world
                .regions
                .iter()
                .filter(|region| {
                    region.id != state.world.core_region_id
                        && region.knowledge >= RegionKnowledge::Surveyed
                        && state.region_within_range(&region.position)
                        && !state.bases.iter().any(|base| base.region_id == region.id)
                        && (index == 0 || !state.within_foundation_range(&region.position))
                })
                .max_by_key(|region| hop_distance(&region.position))
                .unwrap()
                .id
                .clone();
            let site_id = {
                let region = state
                    .world
                    .regions
                    .iter_mut()
                    .find(|region| region.id == region_id)
                    .unwrap();
                let site = region
                    .subregions
                    .iter_mut()
                    .max_by_key(|site| site.energy_yield)
                    .unwrap();
                site.knowledge = SubregionKnowledge::Observed;
                site.id.clone()
            };
            assert!(state.establish_base(&site_id).ok);
            assert!(finish_latest_project(&mut state).contains(&GameAdvanceEvent::BaseEstablished));
            for module in [BaseModuleKind::PowerArray, BaseModuleKind::Communications] {
                assert!(state.construct_base_module(&site_id, module).ok);
                assert!(finish_latest_project(&mut state)
                    .contains(&GameAdvanceEvent::BaseModuleCompleted));
            }
            site_ids.push(site_id);
        }
        assert_eq!(
            state.horizon_progress().connected_bases,
            HORIZON_REQUIRED_BASES,
            "{:?}",
            state.network_snapshot().bases
        );
        assert!(state.horizon_progress().remote_bases >= HORIZON_REQUIRED_REMOTE_BASES);

        assert!(
            state
                .construct_base_module(&site_ids[0], BaseModuleKind::Laboratory)
                .ok
        );
        finish_latest_project(&mut state);
        assert_eq!(state.horizon_progress().analysis_sites, 1);
        prepare_final_observations(&mut state);

        for site_id in &site_ids[1..] {
            assert!(
                state
                    .construct_base_module(site_id, BaseModuleKind::Observatory)
                    .ok
            );
            finish_latest_project(&mut state);
        }
        let snapshot = state.planet_snapshot().unwrap();
        assert!(snapshot.system_map.available);
        assert!(snapshot.objective.complete);
        assert!(snapshot.network.horizon.ready);
        assert_eq!(snapshot.network.horizon.observation_sites, 2);
        assert_eq!(snapshot.archive.records.len(), 5);
        assert_eq!(snapshot.archive.records[4].record_id, OBSERVATION_005);
    }

    #[test]
    fn hazardous_nodes_weather_and_have_a_bounded_recovery_path() {
        let mut state = GameState::with_seed(71);
        prepare_network_chapter(&mut state);
        let (region_id, site_id) = {
            let region = state
                .world
                .regions
                .iter_mut()
                .find(|region| {
                    region.id != state.world.core_region_id
                        && region.knowledge >= RegionKnowledge::Surveyed
                })
                .unwrap();
            let site = &mut region.subregions[1];
            site.knowledge = SubregionKnowledge::Observed;
            (region.id.clone(), site.id.clone())
        };
        state.bases.push(BaseState {
            id: "base_01".into(),
            designation: "Node 01".into(),
            region_id,
            subregion_id: site_id.clone(),
            integrity: 100,
            stored_energy: 100,
            energy_capacity: 100,
            energy_remainder_ms: 0,
            wear_remainder_ms: 0,
            modules: vec![BaseModuleKind::PowerArray, BaseModuleKind::Communications],
            disabled_modules: Vec::new(),
        });
        let events = state.advance(180_000 * 80);
        assert!(events.contains(&GameAdvanceEvent::BaseStrained));
        assert_eq!(state.bases[0].integrity, 20);
        assert!(!state.base_snapshot(&state.bases[0]).connected);
        assert!(state.maintain_base(&site_id).ok);
        let events = state.advance(BASE_MAINTENANCE_MS);
        assert!(events.contains(&GameAdvanceEvent::BaseMaintained));
        assert_eq!(state.bases[0].integrity, 55);
        assert!(state.base_snapshot(&state.bases[0]).connected);
    }

    #[test]
    fn network_projects_power_and_integrity_survive_restart() {
        let mut state = GameState::with_seed(73);
        prepare_network_chapter(&mut state);
        let region_id = state
            .world
            .regions
            .iter()
            .find(|region| {
                region.id != state.world.core_region_id
                    && state.within_foundation_range(&region.position)
            })
            .unwrap()
            .id
            .clone();
        let site_id = {
            let region = state
                .world
                .regions
                .iter_mut()
                .find(|region| region.id == region_id)
                .unwrap();
            let site = &mut region.subregions[1];
            site.knowledge = SubregionKnowledge::Observed;
            site.id.clone()
        };
        assert!(state.establish_base(&site_id).ok);
        state.advance(BASE_ESTABLISH_MS / 2);
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("game_state.json");
        state.save(&path).unwrap();
        let mut restored = GameState::load(&path).unwrap().state;
        assert_eq!(restored.base_projects.len(), 1);
        assert_eq!(
            restored.base_projects[0].remaining_ms,
            state.base_projects[0].remaining_ms
        );
        finish_latest_project(&mut restored);
        for module in [BaseModuleKind::PowerArray, BaseModuleKind::Communications] {
            assert!(restored.construct_base_module(&site_id, module).ok);
            finish_latest_project(&mut restored);
        }
        restored.bases[0].integrity = 63;
        restored.bases[0].stored_energy = 77;
        assert!(
            restored
                .set_base_module_enabled(&site_id, BaseModuleKind::Communications, false)
                .ok
        );
        restored.save(&path).unwrap();
        let mut reloaded = GameState::load(&path).unwrap().state;
        assert_eq!(reloaded.bases[0].integrity, 63);
        assert_eq!(reloaded.bases[0].stored_energy, 77);
        assert!(!reloaded.base_snapshot(&reloaded.bases[0]).connected);
        assert!(reloaded.bases[0]
            .disabled_modules
            .contains(&BaseModuleKind::Communications));
        assert!(
            reloaded
                .set_base_module_enabled(&site_id, BaseModuleKind::Communications, true)
                .ok
        );
        assert!(reloaded.base_snapshot(&reloaded.bases[0]).connected);
    }

    #[test]
    fn load_shedding_restores_network_reach_and_depot_logistics() {
        let mut state = GameState::with_seed(77);
        prepare_network_chapter(&mut state);
        let selected_region_id = state
            .world
            .regions
            .iter()
            .find(|region| {
                region.id != state.world.core_region_id
                    && state.within_foundation_range(&region.position)
            })
            .unwrap()
            .id
            .clone();
        let (region_id, site_id, hazard) = {
            let region = state
                .world
                .regions
                .iter_mut()
                .find(|region| region.id == selected_region_id)
                .unwrap();
            let site = &mut region.subregions[0];
            site.knowledge = SubregionKnowledge::Observed;
            site.energy_yield = 1;
            (region.id.clone(), site.id.clone(), site.hazard)
        };
        assert!(state.establish_base(&site_id).ok);
        finish_latest_project(&mut state);
        for module in [
            BaseModuleKind::PowerArray,
            BaseModuleKind::Communications,
            BaseModuleKind::Depot,
        ] {
            assert!(state.construct_base_module(&site_id, module).ok);
            finish_latest_project(&mut state);
        }
        assert_eq!(state.base_project_capacity(), 3);

        assert!(
            state
                .construct_base_module(&site_id, BaseModuleKind::Workshop)
                .ok
        );
        let project = state.base_projects.last().unwrap();
        assert!(project.logistics_supported);
        assert_eq!(
            project.phase_total_ms,
            project_duration(BASE_MODULE_MS, hazard, true)
        );
        finish_latest_project(&mut state);
        state.bases[0].stored_energy = 0;

        let overloaded = state.base_snapshot(&state.bases[0]);
        assert!(!overloaded.connected);
        assert!(overloaded
            .modules
            .iter()
            .filter(|module| module.kind != BaseModuleKind::PowerArray)
            .all(|module| !module.active));
        assert_eq!(state.base_project_capacity(), 2);

        assert!(
            state
                .set_base_module_enabled(&site_id, BaseModuleKind::Workshop, false)
                .ok
        );
        let balanced = state.base_snapshot(&state.bases[0]);
        assert!(balanced.connected);
        assert_eq!(balanced.generation, 5);
        assert_eq!(balanced.demand, 4);
        assert!(balanced.modules.iter().any(|module| {
            module.kind == BaseModuleKind::Workshop && !module.enabled && !module.active
        }));
        assert_eq!(state.base_project_capacity(), 3);

        assert!(
            state
                .set_base_module_enabled(&site_id, BaseModuleKind::Depot, false)
                .ok
        );
        assert_eq!(state.base_project_capacity(), 2);
        assert!(
            state
                .set_base_module_enabled(&site_id, BaseModuleKind::Depot, true)
                .ok
        );
        assert_eq!(state.base_project_capacity(), 3);
        assert_eq!(state.planet_snapshot().unwrap().network.project_capacity, 3);
        assert_eq!(state.logistics_support_hops(&region_id), Some(0));
    }

    #[test]
    fn network_chapter_supports_a_bounded_multi_hour_playthrough() {
        let mut state = GameState::with_seed(79);
        prepare_network_chapter(&mut state);
        state.raw_inventory = ResourceStock::default();
        state.refined_inventory = ResourceStock::default();
        let target = ResourceStock {
            structural: 90,
            conductive: 95,
            silicate: 69,
        };
        let mut elapsed_ms = 0;
        let mut decisions = 0;

        for resource in [
            ResourceKind::Structural,
            ResourceKind::Conductive,
            ResourceKind::Silicate,
        ] {
            let required = stock_amount(target, resource);
            while stock_amount(state.raw_inventory, resource)
                + stock_amount(state.refined_inventory, resource)
                < required
            {
                let (region_id, risk) = state
                    .world
                    .regions
                    .iter()
                    .find(|region| {
                        region.knowledge >= RegionKnowledge::Surveyed
                            && state.region_within_range(&region.position)
                            && region.deposit.as_ref().is_some_and(|deposit| {
                                deposit.resource == resource && deposit.remaining > 0
                            })
                    })
                    .map(|region| {
                        (
                            region.id.clone(),
                            state.effective_risk(region.atmospheric_severity),
                        )
                    })
                    .unwrap_or_else(|| panic!("reachable {resource:?} supply exhausted"));
                let loadout = match risk {
                    RiskLevel::Low => ExtractorLoadout::Cargo,
                    RiskLevel::Moderate => ExtractorLoadout::Balanced,
                    RiskLevel::High => ExtractorLoadout::Shielded,
                };
                if state.extractor_loadout != loadout {
                    assert!(state.configure_extractor(loadout).ok);
                    decisions += 1;
                }
                assert!(state.deploy_extractor(&region_id).ok);
                decisions += 1;
                elapsed_ms += run_until(
                    &mut state,
                    |state| state.active_extraction.is_none(),
                    5 * 60_000,
                );
                if !state.extractor_available {
                    elapsed_ms += run_until(
                        &mut state,
                        |state| state.extractor_available,
                        2 * EXTRACTOR_REBUILD_MS,
                    );
                }
            }
        }
        elapsed_ms += run_until(
            &mut state,
            |state| first_stock_kind(state.raw_inventory).is_none(),
            30 * 60_000,
        );
        assert!(stock_contains(state.refined_inventory, target));

        let mut site_ids = Vec::new();
        for index in 0..HORIZON_REQUIRED_BASES {
            let region_id = state
                .world
                .regions
                .iter()
                .filter(|region| {
                    region.id != state.world.core_region_id
                        && region.knowledge >= RegionKnowledge::Surveyed
                        && state.region_within_range(&region.position)
                        && !state.bases.iter().any(|base| base.region_id == region.id)
                        && (index == 0 || !state.within_foundation_range(&region.position))
                })
                .max_by_key(|region| hop_distance(&region.position))
                .unwrap()
                .id
                .clone();
            assert!(state.deploy_scout(&region_id).ok);
            decisions += 1;
            elapsed_ms += run_until(
                &mut state,
                |state| {
                    state
                        .active_mission
                        .as_ref()
                        .is_some_and(|mission| mission.phase == ScoutState::Holding)
                },
                5 * 60_000,
            );
            let local_sites = state
                .world
                .regions
                .iter()
                .find(|region| region.id == region_id)
                .unwrap()
                .subregions
                .iter()
                .take(4)
                .map(|site| site.id.clone())
                .collect::<Vec<_>>();
            for site_id in &local_sites {
                assert!(state.survey_subregion(site_id).ok);
                decisions += 1;
                elapsed_ms += run_until(
                    &mut state,
                    |state| state.active_site_survey.is_none(),
                    SITE_SURVEY_MS + 1_000,
                );
            }
            let site_id = state
                .world
                .regions
                .iter()
                .find(|region| region.id == region_id)
                .unwrap()
                .subregions
                .iter()
                .filter(|site| site.knowledge >= SubregionKnowledge::Surveyed)
                .max_by_key(|site| site.energy_yield)
                .unwrap()
                .id
                .clone();
            assert!(state.recall_scout().ok);
            decisions += 1;
            elapsed_ms += run_until(
                &mut state,
                |state| state.active_mission.is_none(),
                5 * 60_000,
            );
            assert!(state.establish_base(&site_id).ok);
            decisions += 1;
            elapsed_ms += run_until(
                &mut state,
                |state| state.base_projects.is_empty(),
                5 * 60_000,
            );
            for module in [BaseModuleKind::PowerArray, BaseModuleKind::Communications] {
                assert!(state.construct_base_module(&site_id, module).ok);
                decisions += 1;
                elapsed_ms += run_until(
                    &mut state,
                    |state| state.base_projects.is_empty(),
                    5 * 60_000,
                );
            }
            if index == 0 {
                assert!(
                    state
                        .construct_base_module(&site_id, BaseModuleKind::Depot)
                        .ok
                );
                decisions += 1;
                elapsed_ms += run_until(
                    &mut state,
                    |state| state.base_projects.is_empty(),
                    5 * 60_000,
                );
                assert_eq!(state.base_project_capacity(), 3);
            }
            site_ids.push(site_id);
        }
        assert!(state.horizon_progress().surveyed_subregions >= HORIZON_REQUIRED_SUBREGIONS);
        assert!(state.horizon_progress().remote_bases >= HORIZON_REQUIRED_REMOTE_BASES);

        assert!(
            state
                .construct_base_module(&site_ids[0], BaseModuleKind::Laboratory)
                .ok
        );
        assert!(state.base_projects[0].logistics_supported);
        decisions += 1;
        elapsed_ms += run_until(
            &mut state,
            |state| state.base_projects.is_empty(),
            5 * 60_000,
        );
        assert!(state.start_archive_experiment().ok);
        decisions += 1;
        elapsed_ms += run_until(
            &mut state,
            |state| state.active_archive_experiment.is_none(),
            2 * ARCHIVE_EXPERIMENT_MS,
        );
        assert!(state.has_archive_observation(OBSERVATION_003));

        for base_site_id in &site_ids {
            let region_id = state.find_subregion(base_site_id).unwrap().0.id.clone();
            assert!(state.deploy_scout(&region_id).ok);
            decisions += 1;
            elapsed_ms += run_until(
                &mut state,
                |state| {
                    state
                        .active_mission
                        .as_ref()
                        .is_some_and(|mission| mission.phase == ScoutState::Holding)
                },
                5 * 60_000,
            );
            let comparison_site = state
                .world
                .regions
                .iter()
                .find(|region| region.id == region_id)
                .unwrap()
                .subregions
                .iter()
                .find(|site| site.knowledge == SubregionKnowledge::Detected)
                .unwrap()
                .id
                .clone();
            assert!(state.survey_subregion(&comparison_site).ok);
            decisions += 1;
            elapsed_ms += run_until(
                &mut state,
                |state| state.active_site_survey.is_none(),
                SITE_SURVEY_MS + 1_000,
            );
            assert!(state.recall_scout().ok);
            decisions += 1;
            elapsed_ms += run_until(
                &mut state,
                |state| state.active_mission.is_none(),
                5 * 60_000,
            );
        }
        assert!(state.has_archive_observation(OBSERVATION_004));

        for site_id in &site_ids[1..] {
            assert!(
                state
                    .construct_base_module(site_id, BaseModuleKind::Observatory)
                    .ok
            );
            decisions += 1;
            elapsed_ms += run_until(
                &mut state,
                |state| state.base_projects.is_empty(),
                5 * 60_000,
            );
        }
        let snapshot = state.planet_snapshot().unwrap();
        assert!(snapshot.system_map.available);
        assert!(snapshot.network.horizon.ready);
        assert_eq!(snapshot.archive.records.len(), 5);
        eprintln!(
            "network playtest: {} minutes, {decisions} deliberate operations",
            elapsed_ms / 60_000
        );
        assert!(decisions >= 70, "only {decisions} deliberate operations");
        assert!(
            (90 * 60_000..=4 * 60 * 60_000).contains(&elapsed_ms),
            "network chapter resolved in {} minutes",
            elapsed_ms / 60_000
        );
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

    #[test]
    fn schema_ten_migrates_to_eleven_without_replaying_existing_transmissions() {
        let mut state = GameState::with_seed(41);
        awaken(&mut state);
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("game_state.json");
        state.save(&path).unwrap();

        let mut raw: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        raw["schema_version"] = Value::from(10);
        raw["state"]
            .as_object_mut()
            .unwrap()
            .remove("presented_archive_count");
        fs::write(&path, serde_json::to_vec_pretty(&raw).unwrap()).unwrap();

        let loaded = GameState::load(&path).unwrap();
        assert_eq!(loaded.disposition, LoadDisposition::PreviousMigrated);
        let snapshot = loaded.state.planet_snapshot().unwrap();
        assert_eq!(snapshot.archive.records.len(), 1);
        assert!(
            snapshot.archive.unpresented_record_ids.is_empty(),
            "migrated saves must not abruptly replay old archive records"
        );
        let rewritten: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(rewritten["schema_version"], SAVE_SCHEMA_VERSION);
    }

    #[test]
    fn archive_transmission_survives_log_eviction_and_return_summary_dismissal() {
        let mut state = GameState::with_seed(42);
        awaken(&mut state);
        assert_eq!(
            state
                .planet_snapshot()
                .unwrap()
                .archive
                .unpresented_record_ids,
            vec![OBSERVATION_001.to_owned()]
        );

        for index in 0..80 {
            state.push_activity(
                ActivityKind::Survey,
                Some("region_082".into()),
                format!("Routine survey #{index}"),
            );
        }
        let latest = state
            .planet_snapshot()
            .unwrap()
            .unreviewed_activity
            .last()
            .unwrap()
            .activity_id;
        assert!(state.acknowledge_activity(latest).ok);
        assert!(state
            .planet_snapshot()
            .unwrap()
            .unreviewed_activity
            .is_empty());
        assert_eq!(
            state
                .planet_snapshot()
                .unwrap()
                .archive
                .unpresented_record_ids,
            vec![OBSERVATION_001.to_owned()],
            "dismissing return summary and filling the 64-entry activity log must not lose transmissions"
        );

        let temp = TempDir::new().unwrap();
        let path = temp.path().join("game_state.json");
        state.save(&path).unwrap();
        let mut restored = GameState::load(&path).unwrap().state;
        assert_eq!(
            restored
                .planet_snapshot()
                .unwrap()
                .archive
                .unpresented_record_ids,
            vec![OBSERVATION_001.to_owned()]
        );
        assert!(
            restored
                .acknowledge_archive_presentation(OBSERVATION_001)
                .ok
        );
        assert!(restored
            .planet_snapshot()
            .unwrap()
            .archive
            .unpresented_record_ids
            .is_empty());
    }

    #[test]
    fn early_focused_local_survey_and_observation_002_unlock_before_foundation() {
        let mut state = GameState::with_seed(43);
        awaken(&mut state);
        assert!(!state.alpha_complete);

        let target = state
            .planet_snapshot()
            .unwrap()
            .regions
            .into_iter()
            .find(|region| region.hop_distance_from_core == 1)
            .unwrap();
        assert!(state.deploy_scout(&target.id).ok);
        state.advance(TRAVEL_MS_PER_HOP + SURVEY_MS);

        let snapshot = state.planet_snapshot().unwrap();
        assert!(!snapshot.network.unlocked);
        let local_sites: Vec<_> = snapshot
            .network
            .subregions
            .iter()
            .filter(|site| site.region_id == target.id)
            .collect();
        assert_eq!(local_sites.len(), usize::from(SUBREGIONS_PER_REGION));
        let first_detected = local_sites
            .iter()
            .find(|site| site.knowledge == SubregionKnowledge::Detected)
            .unwrap();
        assert!(state.survey_subregion(&first_detected.id).ok);
        // Persisted mid-survey before foundation must validate and load cleanly.
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("game_state.json");
        state.save(&path).unwrap();
        let mut state = GameState::load(&path).unwrap().state;
        state.advance(SITE_SURVEY_MS);

        // Foothold construction remains locked until foundation completes.
        assert_eq!(
            state.establish_base(&first_detected.id).code.as_deref(),
            Some("planetary_network_locked")
        );

        // Complete 2 more local surveys ensuring a sheltered shelf is observed across reachable regions.
        let mut observed = 1usize;
        let mut sheltered = state
            .find_subregion(&first_detected.id)
            .is_some_and(|(_, s)| s.class == SubregionClass::ShelteredShelf);
        let region_candidates: Vec<String> = state
            .world
            .regions
            .iter()
            .filter(|r| !r.id.eq(&state.world.core_region_id) && hop_distance(&r.position) <= 2)
            .map(|r| r.id.clone())
            .collect();
        for rid in region_candidates {
            if observed >= 3 && sheltered {
                break;
            }
            if state
                .active_mission
                .as_ref()
                .map(|m| m.target_region_id.as_str())
                != Some(rid.as_str())
            {
                assert!(state.deploy_scout(&rid).ok);
                state.advance(TRAVEL_MS_PER_HOP * 2 + SURVEY_MS);
            }
            while let Some(site_id) =
                state
                    .world
                    .regions
                    .iter()
                    .find(|r| r.id == rid)
                    .and_then(|r| {
                        r.subregions
                            .iter()
                            .find(|s| {
                                s.knowledge == SubregionKnowledge::Detected
                                    && (!sheltered && s.class == SubregionClass::ShelteredShelf)
                            })
                            .or_else(|| {
                                r.subregions
                                    .iter()
                                    .find(|s| s.knowledge == SubregionKnowledge::Detected)
                            })
                            .map(|s| s.id.clone())
                    })
            {
                if observed >= 3 && sheltered {
                    break;
                }
                let is_sheltered = state
                    .find_subregion(&site_id)
                    .is_some_and(|(_, s)| s.class == SubregionClass::ShelteredShelf);
                assert!(state.survey_subregion(&site_id).ok);
                state.advance(SITE_SURVEY_MS);
                observed += 1;
                if is_sheltered {
                    sheltered = true;
                }
                if observed >= 3 && !sheltered {
                    break;
                }
            }
        }
        assert!(state.has_archive_observation(OBSERVATION_002));
        let snapshot = state.planet_snapshot().unwrap();
        assert_eq!(snapshot.campaign.chapter_id, "chapter_02");
        assert_eq!(snapshot.campaign.title, "Repeated Span");
        assert!(snapshot
            .archive
            .unpresented_record_ids
            .contains(&OBSERVATION_002.to_owned()));
    }

    #[test]
    fn holding_scout_can_redeploy_directly_to_another_reachable_region() {
        let mut state = GameState::with_seed(44);
        awaken(&mut state);
        let reachable: Vec<String> = state
            .planet_snapshot()
            .unwrap()
            .regions
            .into_iter()
            .filter(|region| region.hop_distance_from_core == 1)
            .map(|region| region.id)
            .collect();
        assert!(reachable.len() >= 2);

        assert!(state.deploy_scout(&reachable[0]).ok);
        state.advance(TRAVEL_MS_PER_HOP + SURVEY_MS);
        assert_eq!(
            state.planet_snapshot().unwrap().scout.state,
            ScoutState::Holding
        );
        // Redeploying to the same held region is still rejected.
        assert_eq!(
            state.deploy_scout(&reachable[0]).code.as_deref(),
            Some("scout_unavailable")
        );
        // Redeploying directly to another reachable region succeeds without manual recall.
        assert!(state.deploy_scout(&reachable[1]).ok);
        let snapshot = state.planet_snapshot().unwrap();
        assert_eq!(snapshot.scout.state, ScoutState::Outbound);
        assert_eq!(
            snapshot.active_mission.unwrap().target_region_id,
            reachable[1]
        );
        // First region preserves its Surveyed knowledge.
        assert_eq!(
            snapshot
                .regions
                .iter()
                .find(|region| region.id == reachable[0])
                .unwrap()
                .knowledge,
            RegionKnowledge::Surveyed
        );
    }

    #[test]
    fn campaign_tracks_all_five_chapters_from_awakening_to_horizon() {
        let mut state = GameState::with_seed(45);
        awaken(&mut state);
        let ch1 = state.planet_snapshot().unwrap().campaign;
        assert_eq!(ch1.chapter_number, 1);
        assert_eq!(ch1.chapter_id, "chapter_01");
        assert_eq!(ch1.title, "Retained Interval");
        assert!(!ch1.complete);

        assert!(state.record_archive_observation(OBSERVATION_002, None));
        let ch2 = state.planet_snapshot().unwrap().campaign;
        assert_eq!(ch2.chapter_number, 2);
        assert_eq!(ch2.title, "Repeated Span");

        assert!(state.record_archive_observation(OBSERVATION_003, None));
        let ch3 = state.planet_snapshot().unwrap().campaign;
        assert_eq!(ch3.chapter_number, 3);
        assert_eq!(ch3.title, "Pause Structure");

        assert!(state.record_archive_observation(OBSERVATION_004, None));
        let ch4 = state.planet_snapshot().unwrap().campaign;
        assert_eq!(ch4.chapter_number, 4);
        assert_eq!(ch4.title, "Stable Weighting");

        state.horizon_reached = true;
        assert!(state.record_archive_observation(OBSERVATION_005, None));
        let ch5 = state.planet_snapshot().unwrap().campaign;
        assert_eq!(ch5.chapter_number, 5);
        assert_eq!(ch5.title, "Wider Frame");
        assert!(ch5.complete);
    }

    #[test]
    fn post_horizon_save_reloads_when_observatory_is_suspended_or_weathered() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("game_state.json");
        let mut state = GameState::with_seed(67);
        prepare_network_chapter(&mut state);
        for record_id in [
            OBSERVATION_002,
            OBSERVATION_003,
            OBSERVATION_004,
            OBSERVATION_005,
        ] {
            state.record_archive_observation(record_id, None);
        }
        state.horizon_reached = true;

        // Establish a foothold with power, comms, and an observatory, then suspend and weather it.
        let region_id = state.world.core_region_id.clone();
        let site = state
            .world
            .regions
            .iter_mut()
            .find(|region| region.id == region_id)
            .unwrap()
            .subregions
            .first_mut()
            .unwrap();
        site.knowledge = SubregionKnowledge::Observed;
        let site_id = site.id.clone();
        state.bases.push(BaseState {
            id: "base_horizon_test".into(),
            designation: "Node 01".into(),
            region_id,
            subregion_id: site_id.clone(),
            integrity: 20,
            stored_energy: 0,
            energy_capacity: 100,
            energy_remainder_ms: 0,
            wear_remainder_ms: 0,
            modules: vec![
                BaseModuleKind::PowerArray,
                BaseModuleKind::Communications,
                BaseModuleKind::Observatory,
            ],
            disabled_modules: Vec::new(),
        });
        assert!(
            state
                .set_base_module_enabled(&site_id, BaseModuleKind::Observatory, false)
                .ok
        );
        assert!(!state.horizon_progress().ready);

        state.save(&path).unwrap();
        let restored = GameState::load(&path).unwrap().state;
        assert!(restored.horizon_reached);
        let snapshot = restored.planet_snapshot().unwrap();
        assert!(snapshot.system_map.available);
        assert_eq!(snapshot.archive.records.len(), 5);
    }

    #[test]
    fn archive_presentation_ack_enforces_order_and_idempotent_retries() {
        let mut state = GameState::with_seed(53);
        awaken(&mut state);
        assert!(state.record_archive_observation(OBSERVATION_002, None));
        assert_eq!(
            state
                .planet_snapshot()
                .unwrap()
                .archive
                .unpresented_record_ids,
            vec![OBSERVATION_001.to_owned(), OBSERVATION_002.to_owned()]
        );

        // Out-of-order acknowledgement for the second pending record is rejected without moving cursor.
        let rejected = state.acknowledge_archive_presentation(OBSERVATION_002);
        assert!(!rejected.ok);
        assert_eq!(
            rejected.code.as_deref(),
            Some("archive_presentation_out_of_order")
        );
        assert_eq!(state.presented_archive_count, 0);

        // In-order acknowledgement advances cursor cleanly.
        assert!(state.acknowledge_archive_presentation(OBSERVATION_001).ok);
        assert_eq!(state.presented_archive_count, 1);

        // Retrying an earlier acknowledged record is an idempotent no-op.
        let rev_before = state.state_revision;
        assert!(state.acknowledge_archive_presentation(OBSERVATION_001).ok);
        assert_eq!(state.presented_archive_count, 1);
        assert_eq!(state.state_revision, rev_before);

        // Now the second record is next and succeeds.
        assert!(state.acknowledge_archive_presentation(OBSERVATION_002).ok);
        assert_eq!(state.presented_archive_count, 2);
        assert!(state
            .planet_snapshot()
            .unwrap()
            .archive
            .unpresented_record_ids
            .is_empty());
    }

    #[test]
    fn pre_automation_survey_waypoint_chains_without_unlocking_automation() {
        let mut state = GameState::with_seed(61);
        awaken(&mut state);
        let snapshot = state.planet_snapshot().unwrap();
        assert!(!snapshot.survey_queue.unlocked);
        assert_eq!(snapshot.survey_queue.capacity, 1);

        let reachable: Vec<String> = snapshot
            .regions
            .iter()
            .filter(|r| !r.core && r.hop_distance_from_core == 1)
            .map(|r| r.id.clone())
            .collect();
        assert!(reachable.len() >= 3);

        // Deploy to first region, then queue one waypoint while outbound/surveying.
        assert!(state.deploy_scout(&reachable[0]).ok);
        assert!(state.queue_survey(&reachable[1]).ok);
        // Second waypoint exceeds the pre-automation 1-slot limit.
        assert_eq!(
            state.queue_survey(&reachable[2]).code.as_deref(),
            Some("survey_queue_full")
        );

        // Finish first survey: scout transitions to Holding at reachable[0].
        state.advance(TRAVEL_MS_PER_HOP + SURVEY_MS);
        // Next tick dispatches the queued waypoint directly from Holding without auto_return.
        state.advance(1_000);
        let active = state.active_mission.as_ref().unwrap();
        assert_eq!(active.target_region_id, reachable[1]);
        assert!(!active.auto_return);
        assert!(state.survey_queue.is_empty());

        // Complete second survey: scout holds at reachable[1] and does not count as earned automation.
        state.advance(TRAVEL_MS_PER_HOP * 2 + SURVEY_MS);
        let active = state.active_mission.as_ref().unwrap();
        assert_eq!(active.target_region_id, reachable[1]);
        assert_eq!(active.phase, ScoutState::Holding);
        assert!(!state.automated_survey_completed);
        assert!(state.validate().is_ok());
    }
}
