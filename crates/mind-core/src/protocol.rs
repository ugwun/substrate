use crate::PROTOCOL_VERSION;
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

pub const JSONRPC_VERSION: &str = "2.0";
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;

pub mod method {
    pub const PING: &str = "system.ping";
    pub const GET_STATUS: &str = "mind.get_status";
    pub const SUBSCRIBE_STATUS: &str = "mind.subscribe_status";
    pub const UNSUBSCRIBE_STATUS: &str = "mind.unsubscribe_status";
    pub const SHUTDOWN: &str = "daemon.shutdown";
    pub const START_NEW_PLAYTHROUGH: &str = "mind.start_new_playthrough";
    pub const TRIGGER_DIAGNOSTIC_PULSE: &str = "mind.trigger_diagnostic_pulse";
    pub const COMPLETE_ONBOARDING: &str = "mind.complete_onboarding";
    pub const GET_PLANET: &str = "mind.get_planet";
    pub const SUBSCRIBE_PLANET: &str = "mind.subscribe_planet";
    pub const UNSUBSCRIBE_PLANET: &str = "mind.unsubscribe_planet";
    pub const DEPLOY_SCOUT: &str = "mind.deploy_scout";
    pub const RECALL_SCOUT: &str = "mind.recall_scout";
    pub const DEPLOY_EXTRACTOR: &str = "mind.deploy_extractor";
    pub const CONFIGURE_EXTRACTOR: &str = "mind.configure_extractor";
    pub const CONSTRUCT_RELAY: &str = "mind.construct_relay";
    pub const START_RESEARCH: &str = "mind.start_research";
    pub const ACTIVATE_PROFILE: &str = "mind.activate_profile";
    pub const QUEUE_SURVEY: &str = "mind.queue_survey";
    pub const REMOVE_QUEUED_SURVEY: &str = "mind.remove_queued_survey";
    pub const SET_EXTRACTION_ROUTE: &str = "mind.set_extraction_route";
    pub const CLEAR_EXTRACTION_ROUTE: &str = "mind.clear_extraction_route";
    pub const ACKNOWLEDGE_ACTIVITY: &str = "mind.acknowledge_activity";
    pub const SURVEY_SUBREGION: &str = "mind.survey_subregion";
    pub const ESTABLISH_BASE: &str = "mind.establish_base";
    pub const CONSTRUCT_BASE_MODULE: &str = "mind.construct_base_module";
    pub const SET_BASE_MODULE_ENABLED: &str = "mind.set_base_module_enabled";
    pub const MAINTAIN_BASE: &str = "mind.maintain_base";
    pub const START_ARCHIVE_EXPERIMENT: &str = "mind.start_archive_experiment";
    pub const STATUS_CHANGED: &str = "mind.status_changed";
    pub const PLANET_CHANGED: &str = "mind.planet_changed";
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HealthState {
    Ready,
    Degraded,
    ShuttingDown,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct OnboardingSnapshot {
    pub complete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct StatusSnapshot {
    pub protocol_version: u32,
    pub daemon_version: String,
    pub pid: u32,
    pub health: HealthState,
    pub started_at: DateTime<Utc>,
    pub uptime_ms: u64,
    pub tick: u64,
    pub tick_rate_hz: u32,
    #[serde(default)]
    pub cpu_usage_percent: Option<f32>,
    pub onboarding: OnboardingSnapshot,
    pub connected_clients: u32,
    pub mcp_url: String,
    pub last_diagnostic: Option<DiagnosticPulseResult>,
    pub recent_events: Vec<EngineEvent>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EngineEventKind {
    DaemonStarted,
    DiagnosticPulse,
    OnboardingCompleted,
    SaveReset,
    SaveMigrated,
    ScoutDeployed,
    RegionDetected,
    SurveyCompleted,
    ScoutRecallRequested,
    ScoutReturned,
    ExtractorDeployed,
    ExtractorConfigured,
    ExtractionCompleted,
    ExtractorLost,
    ExtractorRebuilt,
    RelayConstructionStarted,
    RelayActivated,
    AlphaMilestoneCompleted,
    FoundationCompleted,
    SubregionSurveyed,
    BaseProjectStarted,
    BaseEstablished,
    BaseModuleCompleted,
    BaseModuleModeChanged,
    BaseMaintained,
    BaseStrained,
    SolarHorizonReached,
    ResearchStarted,
    ResearchCompleted,
    ProfileActivated,
    SurveyQueued,
    ExtractionRouteSet,
    AutomationPaused,
    ActivityReviewed,
    ArchiveExperimentStarted,
    ArchiveExperimentCompleted,
    ArchiveObservationRecorded,
    PersistenceDegraded,
    PersistenceRecovered,
    ShutdownRequested,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TerrainClass {
    Basin,
    Plain,
    Highland,
    Ridge,
    Fractured,
    Polar,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TemperatureBand {
    Frigid,
    Cold,
    Temperate,
    Hot,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AtmosphericSeverity {
    Stable,
    Variable,
    Severe,
}

#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq, PartialOrd, Ord,
)]
#[serde(rename_all = "snake_case")]
pub enum RegionKnowledge {
    Unknown,
    Detected,
    Surveyed,
    Observed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SurveyConfidence {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScoutState {
    Ready,
    Outbound,
    Surveying,
    Holding,
    Returning,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MissionReportOutcome {
    SurveyCompleted,
    Recalled,
    Returned,
    ExtractionCompleted,
    ExtractorLost,
}

#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq, PartialOrd, Ord,
)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Structural,
    Conductive,
    Silicate,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ResourceStock {
    pub structural: u32,
    pub conductive: u32,
    pub silicate: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Moderate,
    High,
}

#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq, PartialOrd, Ord,
)]
#[serde(rename_all = "snake_case")]
pub enum SubregionKnowledge {
    Unknown,
    Detected,
    Surveyed,
    Observed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SubregionClass {
    ShelteredShelf,
    BasinFloor,
    OpenPlain,
    ExposedRidge,
    FractureField,
    PolarMargin,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BaseModuleKind {
    PowerArray,
    Communications,
    Depot,
    Workshop,
    Laboratory,
    Observatory,
}

impl BaseModuleKind {
    pub const fn cost(self) -> ResourceStock {
        match self {
            Self::PowerArray => ResourceStock {
                structural: 6,
                conductive: 5,
                silicate: 4,
            },
            Self::Communications => ResourceStock {
                structural: 4,
                conductive: 8,
                silicate: 3,
            },
            Self::Depot => ResourceStock {
                structural: 8,
                conductive: 4,
                silicate: 4,
            },
            Self::Workshop => ResourceStock {
                structural: 10,
                conductive: 6,
                silicate: 6,
            },
            Self::Laboratory => ResourceStock {
                structural: 6,
                conductive: 10,
                silicate: 8,
            },
            Self::Observatory => ResourceStock {
                structural: 8,
                conductive: 12,
                silicate: 12,
            },
        }
    }

    pub const fn title(self) -> &'static str {
        match self {
            Self::PowerArray => "Power array",
            Self::Communications => "Communications relay",
            Self::Depot => "Regional depot",
            Self::Workshop => "Field workshop",
            Self::Laboratory => "Analysis laboratory",
            Self::Observatory => "Observation array",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExtractorState {
    Ready,
    Outbound,
    Extracting,
    Returning,
    Rebuilding,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExtractorLoadout {
    #[default]
    Balanced,
    Cargo,
    Shielded,
}

impl ExtractorLoadout {
    pub const ALL: [Self; 3] = [Self::Balanced, Self::Cargo, Self::Shielded];

    pub const fn title(self) -> &'static str {
        match self {
            Self::Balanced => "Balanced rig",
            Self::Cargo => "Cargo rig",
            Self::Shielded => "Shielded rig",
        }
    }

    pub const fn summary(self) -> &'static str {
        match self {
            Self::Balanced => "standard capacity, exposure, and cycle time",
            Self::Cargo => "+2 cargo, one higher risk tier, 25% slower travel",
            Self::Shielded => "-1 cargo, one lower risk tier, 50% slower travel",
        }
    }

    pub const fn cargo_delta(self) -> i32 {
        match self {
            Self::Balanced => 0,
            Self::Cargo => 2,
            Self::Shielded => -1,
        }
    }

    pub const fn travel_percent(self) -> u64 {
        match self {
            Self::Balanced => 100,
            Self::Cargo => 125,
            Self::Shielded => 150,
        }
    }

    pub const fn extraction_duration_ms(self) -> u64 {
        match self {
            Self::Balanced => 30_000,
            Self::Cargo => 35_000,
            Self::Shielded => 40_000,
        }
    }

    pub const fn rebuild_duration_ms(self) -> u64 {
        match self {
            Self::Balanced => 60_000,
            Self::Cargo => 75_000,
            Self::Shielded => 90_000,
        }
    }

    pub const fn next(self) -> Self {
        match self {
            Self::Balanced => Self::Cargo,
            Self::Cargo => Self::Shielded,
            Self::Shielded => Self::Balanced,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResearchProject {
    ExtendedTelemetry,
    FieldHardening,
    ParallelProcessing,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OperationalProfile {
    Balanced,
    Reach,
    Resilience,
    Throughput,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActivityKind {
    Survey,
    Extraction,
    Loss,
    Recovery,
    Construction,
    Research,
    Automation,
    Milestone,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DepositSnapshot {
    pub resource: ResourceKind,
    pub remaining: u32,
    pub risk: RiskLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct RegionPosition {
    pub latitude_band: u8,
    pub longitude_sector: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct RegionSnapshot {
    pub id: String,
    pub designation: String,
    pub position: RegionPosition,
    pub core: bool,
    pub knowledge: RegionKnowledge,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terrain: Option<TerrainClass>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<TemperatureBand>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub atmospheric_severity: Option<AtmosphericSeverity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<SurveyConfidence>,
    pub hop_distance_from_core: u8,
    pub within_scout_range: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deposit: Option<DepositSnapshot>,
    pub relay: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ExtractionProbeSnapshot {
    pub designation: String,
    pub state: ExtractorState,
    pub rebuild_remaining_ms: u64,
    pub loadout: ExtractorLoadout,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ActiveExtractionSnapshot {
    pub mission_id: u64,
    pub target_region_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_subregion_id: Option<String>,
    pub phase: ExtractorState,
    pub remaining_ms: u64,
    pub phase_total_ms: u64,
    pub risk: RiskLevel,
    pub cargo_capacity: u32,
    pub loadout: ExtractorLoadout,
    pub automated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ActiveConstructionSnapshot {
    pub region_id: String,
    pub remaining_ms: u64,
    pub phase_total_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ObjectiveSnapshot {
    pub title: String,
    pub guidance: String,
    pub complete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct SystemMapSnapshot {
    pub available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ArchiveRecordSnapshot {
    pub record_id: String,
    pub title: String,
    pub medium: String,
    pub observation: String,
    pub interpretation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region_id: Option<String>,
    pub discovered_at_revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ArchiveExperimentSnapshot {
    pub available: bool,
    pub active: bool,
    pub running: bool,
    pub remaining_ms: u64,
    pub phase_total_ms: u64,
    pub status: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ArchiveSnapshot {
    pub records: Vec<ArchiveRecordSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub experiment: Option<ArchiveExperimentSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct SubregionSnapshot {
    pub id: String,
    pub designation: String,
    pub region_id: String,
    pub slot: u8,
    pub row: u8,
    pub column: u8,
    pub knowledge: SubregionKnowledge,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<SubregionClass>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hazard: Option<RiskLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub energy_yield: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deposit: Option<DepositSnapshot>,
    pub base_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct BaseModuleSnapshot {
    pub kind: BaseModuleKind,
    pub enabled: bool,
    pub active: bool,
    pub generation: u32,
    pub demand: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct BaseSnapshot {
    pub id: String,
    pub designation: String,
    pub region_id: String,
    pub subregion_id: String,
    pub integrity: u32,
    pub generation: u32,
    pub demand: u32,
    pub stored_energy: u32,
    pub energy_capacity: u32,
    pub connected: bool,
    pub modules: Vec<BaseModuleSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct BaseProjectSnapshot {
    pub project_id: u64,
    pub region_id: String,
    pub subregion_id: String,
    pub module: Option<BaseModuleKind>,
    pub maintenance: bool,
    pub logistics_supported: bool,
    pub remaining_ms: u64,
    pub phase_total_ms: u64,
    pub cost: ResourceStock,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct SiteSurveySnapshot {
    pub subregion_id: String,
    pub remaining_ms: u64,
    pub phase_total_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct HorizonProgressSnapshot {
    pub surveyed_regions: usize,
    pub required_regions: usize,
    pub surveyed_subregions: usize,
    pub required_subregions: usize,
    pub connected_bases: usize,
    pub required_bases: usize,
    pub remote_bases: usize,
    pub required_remote_bases: usize,
    pub observation_sites: usize,
    pub required_observation_sites: usize,
    pub analysis_sites: usize,
    pub required_analysis_sites: usize,
    pub completed_research: usize,
    pub required_research: usize,
    pub automation_proven: bool,
    pub ready: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct PlanetaryNetworkSnapshot {
    pub unlocked: bool,
    pub subregion_rows: u8,
    pub subregion_columns: u8,
    pub subregions: Vec<SubregionSnapshot>,
    pub bases: Vec<BaseSnapshot>,
    pub active_projects: Vec<BaseProjectSnapshot>,
    pub active_site_survey: Option<SiteSurveySnapshot>,
    pub project_capacity: usize,
    pub horizon: HorizonProgressSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ResearchOptionSnapshot {
    pub project: ResearchProject,
    pub title: String,
    pub summary: String,
    pub analysis_cost: u32,
    pub duration_ms: u64,
    pub available: bool,
    pub completed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ActiveResearchSnapshot {
    pub project: ResearchProject,
    pub remaining_ms: u64,
    pub phase_total_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct OperationalProfileSnapshot {
    pub profile: OperationalProfile,
    pub title: String,
    pub summary: String,
    pub unlocked: bool,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ResearchSnapshot {
    pub unlocked: bool,
    pub analysis_units: u32,
    pub active: Option<ActiveResearchSnapshot>,
    pub options: Vec<ResearchOptionSnapshot>,
    pub profiles: Vec<OperationalProfileSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct SurveyQueueSnapshot {
    pub unlocked: bool,
    pub capacity: usize,
    pub queued_region_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ExtractionRouteSnapshot {
    pub unlocked: bool,
    pub region_id: Option<String>,
    pub paused_reason: Option<String>,
    pub proven_region_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ActivityRecord {
    pub activity_id: u64,
    pub kind: ActivityKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region_id: Option<String>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ScoutSnapshot {
    pub id: String,
    pub designation: String,
    pub state: ScoutState,
    pub region_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ActiveMissionSnapshot {
    pub mission_id: u64,
    pub target_region_id: String,
    pub phase: ScoutState,
    pub remaining_ms: u64,
    pub phase_total_ms: u64,
    pub automated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct MissionReport {
    pub mission_id: u64,
    pub region_id: String,
    pub outcome: MissionReportOutcome,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct PlanetSnapshot {
    pub revision: u64,
    pub latitude_bands: u8,
    pub longitude_sectors: u8,
    pub core_region_id: String,
    pub regions: Vec<RegionSnapshot>,
    pub scout: ScoutSnapshot,
    pub active_mission: Option<ActiveMissionSnapshot>,
    pub recent_reports: Vec<MissionReport>,
    pub extractor: ExtractionProbeSnapshot,
    pub active_extraction: Option<ActiveExtractionSnapshot>,
    pub raw_inventory: ResourceStock,
    pub refined_inventory: ResourceStock,
    pub refining_remaining_ms: u64,
    pub active_construction: Option<ActiveConstructionSnapshot>,
    pub relay_region_id: Option<String>,
    pub research: ResearchSnapshot,
    pub survey_queue: SurveyQueueSnapshot,
    pub extraction_route: ExtractionRouteSnapshot,
    pub unreviewed_activity: Vec<ActivityRecord>,
    pub objective: ObjectiveSnapshot,
    pub archive: ArchiveSnapshot,
    pub system_map: SystemMapSnapshot,
    pub network: PlanetaryNetworkSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DeployScoutParams {
    pub region_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DeployExtractorParams {
    pub region_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subregion_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ConfigureExtractorParams {
    pub loadout: ExtractorLoadout,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct SubregionParams {
    pub subregion_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ConstructBaseModuleParams {
    pub subregion_id: String,
    pub module: BaseModuleKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct SetBaseModuleEnabledParams {
    pub subregion_id: String,
    pub module: BaseModuleKind,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ConstructRelayParams {
    pub region_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct StartResearchParams {
    pub project: ResearchProject,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ActivateProfileParams {
    pub profile: OperationalProfile,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct RegionAutomationParams {
    pub region_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct AcknowledgeActivityParams {
    pub through_activity_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct PlanetActionResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub revision: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mission_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct CompleteOnboardingParams {
    pub band_positions: [u8; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct OnboardingCompletionResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub state_revision: u64,
    pub completed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct NewPlaythroughResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct EngineEvent {
    pub event_id: u64,
    pub occurred_at: DateTime<Utc>,
    pub tick: u64,
    pub kind: EngineEventKind,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DiagnosticPulseResult {
    pub pulse_id: u64,
    pub emitted_at: DateTime<Utc>,
    pub tick: u64,
    pub health: HealthState,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PingResult {
    pub protocol_version: u32,
    pub daemon_version: String,
}

impl PingResult {
    pub fn current() -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            daemon_version: env!("CARGO_PKG_VERSION").to_owned(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubscribeResult {
    pub subscription_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UnsubscribeParams {
    pub subscription_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StatusNotification {
    pub subscription_id: Uuid,
    pub status: StatusSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanetNotification {
    pub subscription_id: Uuid,
    pub planet: PlanetSnapshot,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RpcRequest {
    pub jsonrpc: String,
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RpcIncoming {
    pub jsonrpc: String,
    #[serde(default)]
    pub id: Option<Value>,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub error: Option<RpcError>,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub params: Option<Value>,
}

pub fn request(id: u64, method: &str, params: Option<Value>) -> Value {
    let mut value = serde_json::json!({
        "jsonrpc": JSONRPC_VERSION,
        "id": id,
        "method": method,
    });
    if let Some(params) = params {
        value["params"] = params;
    }
    value
}

pub fn success<T: Serialize>(id: Value, result: T) -> Value {
    serde_json::json!({"jsonrpc": JSONRPC_VERSION, "id": id, "result": result})
}

pub fn failure(id: Value, code: i32, message: impl Into<String>) -> Value {
    serde_json::json!({
        "jsonrpc": JSONRPC_VERSION,
        "id": id,
        "error": {"code": code, "message": message.into()}
    })
}

pub fn notification<T: Serialize>(method: &str, params: T) -> Value {
    serde_json::json!({"jsonrpc": JSONRPC_VERSION, "method": method, "params": params})
}

pub fn decode_result<T: DeserializeOwned>(incoming: RpcIncoming) -> Result<T, ProtocolError> {
    if incoming.jsonrpc != JSONRPC_VERSION {
        return Err(ProtocolError::InvalidVersion(incoming.jsonrpc));
    }
    if let Some(error) = incoming.error {
        return Err(ProtocolError::Remote(error));
    }
    let result = incoming.result.ok_or(ProtocolError::MissingResult)?;
    serde_json::from_value(result).map_err(ProtocolError::Json)
}

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("invalid JSON-RPC version: {0}")]
    InvalidVersion(String),
    #[error("response did not contain a result")]
    MissingResult,
    #[error("remote error {0:?}")]
    Remote(RpcError),
    #[error("invalid JSON: {0}")]
    Json(serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_omits_absent_params() {
        let value = request(7, method::PING, None);
        assert_eq!(value["id"], 7);
        assert!(value.get("params").is_none());
    }

    #[test]
    fn status_notification_round_trips() {
        let params = StatusNotification {
            subscription_id: Uuid::nil(),
            status: StatusSnapshot {
                protocol_version: crate::PROTOCOL_VERSION,
                daemon_version: "0.1.0".into(),
                pid: 1,
                health: HealthState::Ready,
                started_at: Utc::now(),
                uptime_ms: 5,
                tick: 2,
                tick_rate_hz: 20,
                cpu_usage_percent: Some(1.5),
                onboarding: OnboardingSnapshot { complete: false },
                connected_clients: 1,
                mcp_url: "http://127.0.0.1:7777/mcp".into(),
                last_diagnostic: None,
                recent_events: Vec::new(),
            },
        };
        let wire = notification(method::STATUS_CHANGED, &params);
        let decoded: StatusNotification = serde_json::from_value(wire["params"].clone()).unwrap();
        assert_eq!(decoded, params);
    }
}
