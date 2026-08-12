use crate::protocol::{
    ActiveConstructionSnapshot, ActiveExtractionSnapshot, ActiveMissionSnapshot,
    AtmosphericSeverity, DepositSnapshot, ExtractionProbeSnapshot, ExtractorState, MissionReport,
    MissionReportOutcome, ObjectiveSnapshot, OnboardingCompletionResult, OnboardingSnapshot,
    PlanetActionResult, PlanetSnapshot, RegionKnowledge, RegionPosition, RegionSnapshot,
    ResourceKind, ResourceStock, RiskLevel, ScoutSnapshot, ScoutState, SurveyConfidence,
    TemperatureBand, TerrainClass,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fs, io, io::Write, path::Path};
use tempfile::NamedTempFile;
use thiserror::Error;
use uuid::Uuid;

const SAVE_SCHEMA_VERSION: u32 = 4;
const MIGRATABLE_SCHEMA_VERSION: u32 = 3;
const RESETTABLE_SCHEMA_VERSIONS: [u32; 2] = [1, 2];
const WORLD_GENERATION_VERSION: u32 = 1;
const LATITUDE_BANDS: u8 = 4;
const LONGITUDE_SECTORS: u8 = 6;
const REGION_COUNT: usize = (LATITUDE_BANDS as usize) * (LONGITUDE_SECTORS as usize);
const CORE_LATITUDE_BAND: u8 = 1;
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
    #[serde(default)]
    alpha_complete: bool,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ConstructionState {
    region_id: String,
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
            alpha_complete: false,
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
        let mut value: Value = serde_json::from_slice(&bytes)?;
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
        let migrated = version.schema_version == MIGRATABLE_SCHEMA_VERSION;
        if migrated {
            value["schema_version"] = Value::from(SAVE_SCHEMA_VERSION);
        }
        if version.schema_version != SAVE_SCHEMA_VERSION && !migrated {
            return Err(GameError::UnsupportedVersion(version.schema_version));
        }
        let envelope: SaveEnvelope = serde_json::from_value(value)?;
        let mut state = envelope.state;
        if migrated {
            state.initialize_alpha_fields();
            state.save(path)?;
        }
        state.validate()?;
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
                        cargo_capacity: EXTRACTION_CAPACITY,
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
        let risk = risk_for(region.atmospheric_severity);
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

    pub fn advance(&mut self, elapsed_ms: u64) -> Vec<GameAdvanceEvent> {
        let mut events = self.advance_scout(elapsed_ms);
        if events.contains(&GameAdvanceEvent::SurveyCompleted)
            && !self.alpha_complete
            && self.relay_region_id.is_some()
            && self.active_mission.as_ref().is_some_and(|mission| {
                self.world
                    .regions
                    .iter()
                    .find(|region| region.id == mission.target_region_id)
                    .is_some_and(|region| hop_distance(&region.position) > SCOUT_RANGE_HOPS)
            })
        {
            self.alpha_complete = true;
            self.changed();
            events.push(GameAdvanceEvent::AlphaMilestoneCompleted);
        }
        self.advance_refining(elapsed_ms);
        events.extend(self.advance_extraction(elapsed_ms));
        if let Some(mut construction) = self.active_construction.take() {
            construction.remaining_ms = construction.remaining_ms.saturating_sub(elapsed_ms);
            if construction.remaining_ms == 0 {
                self.relay_region_id = Some(construction.region_id);
                events.push(GameAdvanceEvent::RelayActivated);
            } else {
                self.active_construction = Some(construction);
            }
            self.changed();
        }
        if !self.extractor_available && self.extractor_rebuild_remaining_ms > 0 {
            self.extractor_rebuild_remaining_ms = self
                .extractor_rebuild_remaining_ms
                .saturating_sub(elapsed_ms);
            if self.extractor_rebuild_remaining_ms == 0 {
                self.extractor_available = true;
                events.push(GameAdvanceEvent::ExtractorRebuilt);
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
                            mission.remaining_ms = SURVEY_MS;
                            mission.phase_total_ms = SURVEY_MS;
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
                    mission.phase = ScoutState::Holding;
                    self.push_report(MissionReport {
                        mission_id: mission.mission_id,
                        region_id: mission.target_region_id.clone(),
                        outcome: MissionReportOutcome::SurveyCompleted,
                    });
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
                    events.push(GameAdvanceEvent::ScoutReturned);
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
                    self.changed();
                    events.push(GameAdvanceEvent::ExtractorLost);
                    return events;
                }
                ExtractorState::Extracting => {
                    if let Some(deposit) = self
                        .world
                        .regions
                        .iter_mut()
                        .find(|region| region.id == mission.target_region_id)
                        .and_then(|region| region.deposit.as_mut())
                    {
                        let amount = deposit.remaining.min(EXTRACTION_CAPACITY);
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
                    }
                    self.push_report(MissionReport {
                        mission_id: mission.mission_id,
                        region_id: mission.target_region_id.clone(),
                        outcome: MissionReportOutcome::ExtractionCompleted,
                    });
                    self.changed();
                    events.push(GameAdvanceEvent::ExtractionCompleted);
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
                self.refining_remaining_ms = REFINING_MS_PER_UNIT;
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
                    risk: risk_for(region.atmospheric_severity),
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
                title: "ALPHA REACH ESTABLISHED".into(),
                guidance: "The first network extension is proven. Continue surveying freely."
                    .into(),
                complete: true,
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

    fn initialize_alpha_fields(&mut self) {
        for region in &mut self.world.regions {
            if region.id == self.world.core_region_id || region.deposit.is_some() {
                continue;
            }
            let index = usize::from(region.position.latitude_band) * usize::from(LONGITUDE_SECTORS)
                + usize::from(region.position.longitude_sector);
            region.deposit = Some(DepositState {
                resource: match index % 3 {
                    0 => ResourceKind::Structural,
                    1 => ResourceKind::Conductive,
                    _ => ResourceKind::Silicate,
                },
                remaining: 12,
            });
        }
        if self.random_state == 0 {
            self.random_state = self.world.seed ^ 0xA17A_5EED_D15C_0FFE;
        }
        if self.active_extraction.is_none() && self.extractor_rebuild_remaining_ms == 0 {
            self.extractor_available = true;
        }
    }

    fn region_within_range(&self, position: &RegionPosition) -> bool {
        if hop_distance(position) <= SCOUT_RANGE_HOPS {
            return true;
        }
        self.relay_region_id
            .as_ref()
            .and_then(|id| self.world.regions.iter().find(|region| &region.id == id))
            .is_some_and(|relay| graph_distance(position, &relay.position) <= RELAY_RANGE_HOPS)
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
            return Err(GameError::Invariant("planet must contain 24 regions"));
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
                regions.push(RegionState {
                    id: format!("region_{:02}", index + 1),
                    designation: format!("R-{:02}", index + 1),
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
                        resource: match index % 3 {
                            0 => ResourceKind::Structural,
                            1 => ResourceKind::Conductive,
                            _ => ResourceKind::Silicate,
                        },
                        remaining: 12,
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
    match latitude_band {
        0 | 3 if roll < 70 => TemperatureBand::Frigid,
        0 | 3 => TemperatureBand::Cold,
        1 | 2 if roll < 15 => TemperatureBand::Cold,
        1 | 2 if roll > 80 => TemperatureBand::Hot,
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
        assert_eq!(snapshot.regions.len(), 24);
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
            state.deploy_scout("region_01").code.as_deref(),
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
    fn persists_schema_four_migrates_three_and_resets_older_saves() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("game_state.json");
        let mut state = GameState::with_seed(13);
        awaken(&mut state);
        state.save(&path).unwrap();
        assert_eq!(
            GameState::load(&path).unwrap().disposition,
            LoadDisposition::Current
        );

        let mut previous: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        previous["schema_version"] = Value::from(3);
        let state = previous["state"].as_object_mut().unwrap();
        for key in [
            "active_extraction",
            "extractor_available",
            "extractor_rebuild_remaining_ms",
            "raw_inventory",
            "refined_inventory",
            "refining_remaining_ms",
            "active_construction",
            "relay_region_id",
            "alpha_complete",
            "random_state",
        ] {
            state.remove(key);
        }
        for region in state["world"]["regions"].as_array_mut().unwrap() {
            region.as_object_mut().unwrap().remove("deposit");
        }
        fs::write(&path, serde_json::to_vec_pretty(&previous).unwrap()).unwrap();
        let migrated = GameState::load(&path).unwrap();
        assert_eq!(migrated.disposition, LoadDisposition::PreviousMigrated);
        assert!(migrated
            .state
            .planet_snapshot()
            .unwrap()
            .regions
            .iter()
            .filter(|region| !region.core)
            .all(|region| region.deposit.is_none()));
        let migrated_file: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(migrated_file["schema_version"], 4);

        fs::write(
            &path,
            r#"{"schema_version":2,"saved_at":"2026-01-01T00:00:00Z","state":{"onboarding_complete":true,"state_revision":1}}"#,
        )
        .unwrap();
        let reset = GameState::load(&path).unwrap();
        assert_eq!(reset.disposition, LoadDisposition::PreviousReset);
        assert!(!reset.state.onboarding().complete);
        let replacement: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(replacement["schema_version"], 4);
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
    fn relay_extends_reach_and_distant_survey_completes_alpha() {
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
        assert!(state.planet_snapshot().unwrap().objective.complete);
    }
}
