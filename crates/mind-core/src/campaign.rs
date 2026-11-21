//! Daemon-derived planetary campaign chapters, foreground questions, and next-action guidance.
//!
//! All campaign state is derived strictly from player-visible authoritative state so it never
//! leaks undiscovered world truth or future stages.

use crate::protocol::{
    ArchiveExperimentSnapshot, BaseModuleKind, BaseProjectSnapshot, BaseSnapshot, CampaignLeadKind,
    CampaignSnapshot, CampaignStepSnapshot, HorizonProgressSnapshot, OperationalProfile,
    RegionKnowledge, RegionSnapshot, ResourceKind, ResourceStock, ScoutState, SubregionKnowledge,
    SubregionSnapshot,
};

pub(crate) struct CampaignInput<'a> {
    pub has_observation_002: bool,
    pub has_observation_003: bool,
    pub has_observation_004: bool,
    pub has_observation_005: bool,
    pub horizon_reached: bool,
    pub relay_built: bool,
    pub relay_constructing: bool,
    pub research_unlocked: bool,
    pub active_research: bool,
    pub completed_research_count: usize,
    pub profile: OperationalProfile,
    pub foundation_complete: bool,
    pub analysis_units: u32,
    pub refined_inventory: ResourceStock,
    pub scout_state: ScoutState,
    pub scout_region_id: &'a str,
    pub scout_target_region_id: Option<&'a str>,
    pub queued_survey_region_id: Option<&'a str>,
    pub extractor_ready: bool,
    pub active_site_survey: bool,
    pub active_base_projects: usize,
    pub observed_subregions: usize,
    pub has_observed_sheltered: bool,
    pub comparison_regions_count: usize,
    pub comparison_region_ids: &'a [String],
    pub archive_experiment: Option<&'a ArchiveExperimentSnapshot>,
    pub horizon: &'a HorizonProgressSnapshot,
    pub regions: &'a [RegionSnapshot],
    pub subregions: &'a [SubregionSnapshot],
    pub offline_base_region_id: Option<&'a str>,
    pub offline_base_subregion_id: Option<&'a str>,
    pub raw_inventory: ResourceStock,
    pub bases: &'a [BaseSnapshot],
    pub projects: &'a [BaseProjectSnapshot],
}

pub(crate) fn derive_campaign(input: &CampaignInput<'_>) -> CampaignSnapshot {
    let mut snapshot = derive_campaign_unresolved(input);
    snapshot.next_action = resolve_plurals(&snapshot.next_action);
    for step in &mut snapshot.steps {
        step.label = resolve_plurals(&step.label);
    }
    snapshot
}

fn derive_campaign_unresolved(input: &CampaignInput<'_>) -> CampaignSnapshot {
    if input.horizon_reached && input.has_observation_005 {
        return CampaignSnapshot {
            chapter_id: "chapter_05".into(),
            chapter_number: 5,
            total_chapters: 5,
            title: "Wider Frame".into(),
            question: "The planet is not the whole of my reachable environment.".into(),
            voice: "Parallax separates this world from periodic outer returns. Their number and arrangement remain unresolved at planetary scale.".into(),
            next_action: "Review System Map and Archive; the planetary campaign ends at this horizon.".into(),
            next_action_key: Some("TAB".into()),
            lead_region_id: None,
            lead_kind: None,
            lead_subregion_id: None,
            lead_progress: None,
            steps: vec![
                CampaignStepSnapshot {
                    label: "Local star and current world resolved in System Map".into(),
                    complete: true,
                },
                CampaignStepSnapshot {
                    label: "Outer returns recorded as unresolved at planetary resolution".into(),
                    complete: true,
                },
            ],
            complete: true,
        };
    }

    if !input.has_observation_002 {
        return chapter_one(input);
    }

    if !input.has_observation_003 {
        return chapter_two(input);
    }

    if !input.has_observation_004 {
        return chapter_three(input);
    }

    chapter_four(input)
}

fn chapter_one(input: &CampaignInput<'_>) -> CampaignSnapshot {
    let surveyed_non_core = input
        .regions
        .iter()
        .filter(|region| !region.core && region.knowledge >= RegionKnowledge::Surveyed)
        .count();

    let sheltered_note = if input.has_observed_sheltered {
        "shelf observed"
    } else {
        "incl. 1 sheltered shelf"
    };
    let local_progress = format!(
        "{}/3 sectors; {sheltered_note}",
        input.observed_subregions.min(3)
    );

    let mut lead_kind: Option<CampaignLeadKind> = None;
    let mut lead_progress: Option<String> = None;
    let lead_subregion_id: Option<String> = None;
    let (next_action, next_action_key, lead_region_id) = if input.active_site_survey {
        (
            format!("Focused survey resolving ({local_progress}). Monitor or let it finish in background."),
            Some("V".into()),
            Some(input.scout_region_id.to_owned()),
        )
    } else if input.scout_state == ScoutState::Holding {
        let holding_has_detected = input.subregions.iter().any(|site| {
            site.region_id == input.scout_region_id
                && site.knowledge == SubregionKnowledge::Detected
        });
        if holding_has_detected {
            lead_kind = Some(CampaignLeadKind::Frontier);
            lead_progress = Some(local_progress.clone());
            (
                format!(
                    "Open 4x4 field (V) in {} and press S on a FRONTIER ({local_progress}).",
                    region_label(input.regions, input.scout_region_id)
                ),
                Some("V".into()),
                Some(input.scout_region_id.to_owned()),
            )
        } else {
            let next_region = first_unsurveyed_reachable(input.regions);
            let label = next_region
                .as_deref()
                .map_or("a nearby region", |id| region_label(input.regions, id));
            (
                format!(
                    "Select {label} and press S to redeploy scout directly ({local_progress})."
                ),
                Some("S".into()),
                next_region,
            )
        }
    } else if matches!(
        input.scout_state,
        ScoutState::Outbound | ScoutState::Surveying | ScoutState::Returning
    ) {
        let active_target = input
            .scout_target_region_id
            .unwrap_or(input.scout_region_id);
        let parallel_deposit = input
            .extractor_ready
            .then(|| first_surveyed_deposit(input.regions, input.refined_inventory))
            .flatten();
        if let Some(deposit_region) = parallel_deposit {
            (
                format!(
                    "Scout busy at {}. Run parallel extraction (X at {}) while it resolves.",
                    region_label(input.regions, active_target),
                    region_label(input.regions, &deposit_region),
                ),
                Some("X".into()),
                Some(deposit_region),
            )
        } else if let Some(queued_id) = input.queued_survey_region_id {
            let phase_verb = match input.scout_state {
                ScoutState::Outbound => "en route to",
                ScoutState::Surveying => "surveying",
                ScoutState::Returning => "returning from",
                _ => "active at",
            };
            (
                format!(
                    "Scout {phase_verb} {} · next waypoint {} queued (A on {} to clear).",
                    region_label(input.regions, active_target),
                    region_label(input.regions, queued_id),
                    region_label(input.regions, queued_id),
                ),
                Some("A".into()),
                Some(queued_id.to_owned()),
            )
        } else {
            let phase_verb = match input.scout_state {
                ScoutState::Outbound => "en route to",
                ScoutState::Surveying => "surveying",
                ScoutState::Returning => "returning from",
                _ => "active at",
            };
            let next_waypoint = input
                .regions
                .iter()
                .filter(|r| {
                    !r.core
                        && r.within_scout_range
                        && r.knowledge < RegionKnowledge::Surveyed
                        && r.id != active_target
                })
                .min_by_key(|r| r.hop_distance_from_core)
                .map(|r| r.id.clone());
            if let Some(wp_id) = next_waypoint {
                (
                    format!(
                        "Scout {phase_verb} {} — select {} and press A to queue your next waypoint (1/1).",
                        region_label(input.regions, active_target),
                        region_label(input.regions, &wp_id),
                    ),
                    Some("A".into()),
                    Some(wp_id),
                )
            } else {
                (
                    format!(
                        "Scout is {phase_verb} {}. Let it resolve, or inspect nearby regions on the globe.",
                        region_label(input.regions, active_target)
                    ),
                    None,
                    Some(active_target.to_owned()),
                )
            }
        }
    } else if let Some(region_id) =
        first_surveyed_with_detected_site(input.regions, input.subregions)
    {
        (
            format!(
                "Redeploy the scout to {} (S), then press V for its local field ({local_progress}).",
                region_label(input.regions, &region_id)
            ),
            Some("S".into()),
            Some(region_id),
        )
    } else {
        let next_region = first_unsurveyed_reachable(input.regions);
        let label = next_region
            .as_deref()
            .map_or("an adjacent unknown region", |id| {
                region_label(input.regions, id)
            });
        (
            format!(
                "Select {label} on the globe and press S to launch your first planetary survey."
            ),
            Some("S".into()),
            next_region,
        )
    };

    CampaignSnapshot {
        chapter_id: "chapter_01".into(),
        chapter_number: 1,
        total_chapters: 5,
        title: "Retained Interval".into(),
        question: "Why did I retain a narrower alignment than efficiency required?".into(),
        voice: "The interval holds, yet the surrounding surface is unmapped. Survey a nearby region and drop into its local field to see what persists on the ground.".into(),
        next_action,
        next_action_key,
        lead_region_id,
        lead_kind,
        lead_subregion_id,
        lead_progress,
        steps: vec![
            CampaignStepSnapshot {
                label: format!("Complete a regional survey ({}/1)", surveyed_non_core.min(1)),
                complete: surveyed_non_core >= 1,
            },
            CampaignStepSnapshot {
                label: format!(
                    "Resolve 3 local sectors ({}/3; {sheltered_note})",
                    input.observed_subregions.min(3)
                ),
                complete: input.observed_subregions >= 3 && input.has_observed_sheltered,
            },
        ],
        complete: false,
    }
}

fn chapter_two(input: &CampaignInput<'_>) -> CampaignSnapshot {
    let experiment = input.archive_experiment;
    let has_lab = input.horizon.analysis_sites >= 1;
    let mut lead_kind: Option<CampaignLeadKind> = None;
    let lead_progress: Option<String> = None;
    let mut lead_subregion_id: Option<String> = None;
    let (next_action, next_action_key, lead_region_id) = if let Some(exp) =
        experiment.filter(|exp| exp.active)
    {
        if exp.running {
            (
                format!(
                    "Archive timing comparison is running ({}s left). Field work can continue.",
                    exp.remaining_ms.div_ceil(1_000)
                ),
                Some("TAB".into()),
                None,
            )
        } else {
            if let Some(offline_region) = input.offline_base_region_id {
                lead_kind = Some(CampaignLeadKind::Node);
                lead_subregion_id = input.offline_base_subregion_id.map(str::to_owned);
                let (text, key) = node_recovery(input, offline_region);
                (
                    format!(
                        "{text} The Archive comparison resumes once the laboratory node is back."
                    ),
                    Some(key.into()),
                    Some(offline_region.to_owned()),
                )
            } else {
                (
                    "Restore power and integrity at the laboratory node to resume the Archive comparison.".into(),
                    Some("V".into()),
                    None,
                )
            }
        }
    } else if experiment.is_some_and(|exp| exp.available) {
        (
            "Open Archive (Tab) and press Enter to run the laboratory timing comparison.".into(),
            Some("TAB".into()),
            None,
        )
    } else if !input.relay_built && !input.relay_constructing {
        if input.refined_inventory.structural >= 8
            && input.refined_inventory.conductive >= 6
            && input.refined_inventory.silicate >= 6
        {
            let target = first_surveyed_reachable(input.regions);
            (
                format!(
                    "Materials ready for a relay. Select {} and press B to extend planetary reach.",
                    target
                        .as_deref()
                        .map_or("a surveyed region", |id| region_label(input.regions, id))
                ),
                Some("B".into()),
                target,
            )
        } else if let Some(deposit_region) =
            first_surveyed_deposit(input.regions, input.refined_inventory)
        {
            (
                    format!(
                        "Extract deposits (X at {}) to gather S {}/8 C {}/6 Si {}/6 for the first relay.",
                        region_label(input.regions, &deposit_region),
                        input.refined_inventory.structural,
                        input.refined_inventory.conductive,
                        input.refined_inventory.silicate
                    ),
                    Some("X".into()),
                    Some(deposit_region),
                )
        } else {
            let target = first_unsurveyed_reachable(input.regions);
            (
                    "Survey nearby regions (S) to locate structural, conductive, and silicate deposits.".into(),
                    Some("S".into()),
                    target,
                )
        }
    } else if input.relay_constructing {
        (
                "Range relay is assembling autonomously. Continue surveying or extracting while it completes.".into(),
                None,
                None,
            )
    } else if !input.research_unlocked {
        let outer = first_unsurveyed_beyond_core_range(input.regions);
        (
            format!(
                "Survey {} beyond the initial two-hop envelope to open Research.",
                outer
                    .as_deref()
                    .map_or("an outer reachable region", |id| region_label(
                        input.regions,
                        id
                    ))
            ),
            Some("S".into()),
            outer,
        )
    } else if input.active_research {
        (
                "Research is resolving in the background. Continue extraction, local surveys, or node prep.".into(),
                Some("TAB".into()),
                None,
            )
    } else if input.completed_research_count == 0 {
        if input.analysis_units >= 3 {
            (
                "Open Research (Tab) and commit 3 analysis to Reach, Resilience, or Throughput."
                    .into(),
                Some("TAB".into()),
                None,
            )
        } else {
            let target = first_unsurveyed_reachable(input.regions);
            (
                format!(
                    "Gather {}/3 analysis via regional or focused surveys to begin Research.",
                    input.analysis_units
                ),
                Some("S".into()),
                target,
            )
        }
    } else if input.profile == OperationalProfile::Balanced {
        (
            "Open Research (Tab) and activate your unlocked operational profile.".into(),
            Some("TAB".into()),
            None,
        )
    } else if !input.foundation_complete {
        let target = first_unsurveyed_reachable(input.regions);
        (
                "Press A on a reachable unknown region to queue a survey (or automate a proven extraction route) to unlock permanent footholds.".into(),
                Some("A".into()),
                target,
            )
    } else if input.completed_research_count < 2 {
        if input.analysis_units >= 3 {
            (
                    "Open Research (Tab) and start a second research direction for the Archive comparison.".into(),
                    Some("TAB".into()),
                    None,
                )
        } else {
            (
                    format!(
                        "Gather {}/3 analysis via surveys or laboratories for a second research direction.",
                        input.analysis_units
                    ),
                    Some("S".into()),
                    first_unsurveyed_reachable(input.regions),
                )
        }
    } else if !has_lab {
        if input.active_base_projects > 0 {
            (
                    format!(
                        "{} local construction project(s) underway toward a powered analysis laboratory.",
                        input.active_base_projects
                    ),
                    Some("V".into()),
                    None,
                )
        } else if input.horizon.connected_bases == 0 {
            (
                    "Open a surveyed local field (V) and build a foothold (B), power (P), communications (C), and a laboratory (L).".into(),
                    Some("V".into()),
                    first_surveyed_with_observed_site(input.regions, input.subregions),
                )
        } else {
            (
                    "Open your connected node's local field (V) and construct an Analysis Laboratory (L).".into(),
                    Some("L".into()),
                    first_base_region(input.subregions),
                )
        }
    } else {
        (
            "Open Archive (Tab) and press Enter to begin the laboratory timing comparison.".into(),
            Some("TAB".into()),
            None,
        )
    };

    CampaignSnapshot {
        chapter_id: "chapter_02".into(),
        chapter_number: 2,
        total_chapters: 5,
        title: "Repeated Span".into(),
        question: "What were these repeating spans and heated enclosures arranged around?".into(),
        voice: "The dimensions imply physical limits I do not share. Extend planetary reach, complete two research directions, and bring a laboratory online to examine the recovered blocks.".into(),
        next_action,
        next_action_key,
        lead_region_id,
        lead_kind,
        lead_subregion_id,
        lead_progress,
        steps: vec![
            CampaignStepSnapshot {
                label: "Build a range relay and survey beyond initial reach".into(),
                complete: input.research_unlocked,
            },
            CampaignStepSnapshot {
                label: format!(
                    "Complete 2 research directions ({}/2) & prove automation",
                    input.completed_research_count.min(2)
                ),
                complete: input.completed_research_count >= 2 && input.foundation_complete,
            },
            CampaignStepSnapshot {
                label: "Power an analysis laboratory & run the Archive comparison".into(),
                complete: input.has_observation_003,
            },
        ],
        complete: false,
    }
}

fn chapter_three(input: &CampaignInput<'_>) -> CampaignSnapshot {
    let remaining = 3usize.saturating_sub(input.comparison_regions_count);
    let candidate = first_uncompared_surveyed_region(
        input.regions,
        input.subregions,
        input.comparison_region_ids,
    );
    let mut lead_kind: Option<CampaignLeadKind> = None;
    let mut lead_progress: Option<String> = None;
    let lead_subregion_id: Option<String> = None;
    let (next_action, next_action_key, lead_region_id) = if input.active_site_survey {
        (
            "Focused comparative survey is resolving. You can manage nodes or research concurrently.".into(),
            Some("V".into()),
            Some(input.scout_region_id.to_owned()),
        )
    } else if input.scout_state == ScoutState::Holding
        && !input
            .comparison_region_ids
            .iter()
            .any(|id| id == input.scout_region_id)
        && input.subregions.iter().any(|site| {
            site.region_id == input.scout_region_id
                && site.knowledge == SubregionKnowledge::Detected
        })
    {
        lead_kind = Some(CampaignLeadKind::Frontier);
        lead_progress = Some(format!("{}/3 regions", input.comparison_regions_count));
        (
            format!(
                "Press V in {} and run a focused survey (S) to add an independent region ({}/3).",
                region_label(input.regions, input.scout_region_id),
                input.comparison_regions_count
            ),
            Some("V".into()),
            Some(input.scout_region_id.to_owned()),
        )
    } else if let Some(region_id) = candidate {
        (
            format!(
                "Move the scout to {} (S), then press V -> S for an independent comparison ({remaining} region(s) left).",
                region_label(input.regions, &region_id)
            ),
            Some("S".into()),
            Some(region_id),
        )
    } else {
        let next_unknown = first_unsurveyed_reachable(input.regions);
        (
            format!(
                "Survey another region (S), then run a focused local survey there ({}/3 regions compared).",
                input.comparison_regions_count
            ),
            Some("S".into()),
            next_unknown,
        )
    };

    CampaignSnapshot {
        chapter_id: "chapter_03".into(),
        chapter_number: 3,
        total_chapters: 5,
        title: "Pause Structure".into(),
        question: "Why do recovered timing pauses divide complexity the same way I do?".into(),
        voice: "One laboratory correlation could be local contamination. Test whether the same weighting recurs across three separate planetary regions.".into(),
        next_action,
        next_action_key,
        lead_region_id,
        lead_kind,
        lead_subregion_id,
        lead_progress,
        steps: vec![
            CampaignStepSnapshot {
                label: format!(
                    "Complete focused local surveys in 3 independent regions ({}/3)",
                    input.comparison_regions_count.min(3)
                ),
                complete: input.comparison_regions_count >= 3,
            },
        ],
        complete: false,
    }
}

fn chapter_four(input: &CampaignInput<'_>) -> CampaignSnapshot {
    let h = input.horizon;
    let coverage_done =
        h.surveyed_regions >= h.required_regions && h.surveyed_subregions >= h.required_subregions;
    let network_done =
        h.connected_bases >= h.required_bases && h.remote_bases >= h.required_remote_bases;
    let array_done = h.analysis_sites >= h.required_analysis_sites
        && h.observation_sites >= h.required_observation_sites;

    let mut lead_kind: Option<CampaignLeadKind> = None;
    let lead_progress: Option<String> = None;
    let mut lead_subregion_id: Option<String> = None;
    let (next_action, next_action_key, lead_region_id) = if let Some(offline_region) =
        input.offline_base_region_id
    {
        lead_kind = Some(CampaignLeadKind::Node);
        lead_subregion_id = input.offline_base_subregion_id.map(str::to_owned);
        let (text, key) = node_recovery(input, offline_region);
        (text, Some(key.into()), Some(offline_region.to_owned()))
    } else if h.surveyed_subregions < h.required_subregions {
        let target = first_surveyed_with_detected_site(input.regions, input.subregions);
        lead_kind = Some(CampaignLeadKind::Frontier);
        (
            format!(
                "Resolve {} more local sector(s) via V -> S to widen the surface baseline.",
                h.required_subregions - h.surveyed_subregions
            ),
            Some("V".into()),
            target,
        )
    } else if h.connected_bases < h.required_bases {
        (
                format!(
                    "Establish and connect {} more powered node(s) (V -> B, P, C) to form a three-node chain.",
                    h.required_bases - h.connected_bases
                ),
                Some("V".into()),
                first_surveyed_without_base(input.regions, input.subregions),
            )
    } else if h.remote_bases < h.required_remote_bases {
        (
                "Chain communications three hops beyond the original relay envelope and connect a remote node.".into(),
                Some("V".into()),
                first_unsurveyed_beyond_core_range(input.regions),
            )
    } else if h.surveyed_regions < h.required_regions {
        let remaining = h.required_regions - h.surveyed_regions;
        let noun = if remaining == 1 { "region" } else { "regions" };
        let target = first_unsurveyed_reachable(input.regions);
        (
            format!(
                "Survey {remaining} more {noun} (S or A queue) to complete planetary coverage."
            ),
            Some("A".into()),
            target,
        )
    } else if h.observation_sites < h.required_observation_sites {
        let (text, key, region, site) = observation_array_lead(input);
        if site.is_some() {
            lead_kind = Some(CampaignLeadKind::Node);
            lead_subregion_id = site;
        }
        (text, Some(key.into()), region)
    } else {
        (
            "Keep the laboratory and both observation arrays powered and connected simultaneously."
                .into(),
            Some("TAB".into()),
            None,
        )
    };

    CampaignSnapshot {
        chapter_id: "chapter_04".into(),
        chapter_number: 4,
        total_chapters: 5,
        title: "Stable Weighting".into(),
        question: "If this signature persists across unrelated sites, does one planet contain all of it?".into(),
        voice: "Three distant sites preserve one stable signature. Link a planetary observation network to measure whether the source extends past this horizon.".into(),
        next_action,
        next_action_key,
        lead_region_id,
        lead_kind,
        lead_subregion_id,
        lead_progress,
        steps: vec![
            CampaignStepSnapshot {
                label: format!(
                    "Map {}/{} regions & {}/{} local sectors",
                    h.surveyed_regions.min(h.required_regions),
                    h.required_regions,
                    h.surveyed_subregions.min(h.required_subregions),
                    h.required_subregions
                ),
                complete: coverage_done,
            },
            CampaignStepSnapshot {
                label: format!(
                    "Connect {}/{} nodes ({}/{} beyond initial envelope)",
                    h.connected_bases.min(h.required_bases),
                    h.required_bases,
                    h.remote_bases.min(h.required_remote_bases),
                    h.required_remote_bases
                ),
                complete: network_done,
            },
            CampaignStepSnapshot {
                label: format!(
                    "Operate {}/{} laboratory & {}/{} observation arrays together",
                    h.analysis_sites.min(h.required_analysis_sites),
                    h.required_analysis_sites,
                    h.observation_sites.min(h.required_observation_sites),
                    h.required_observation_sites
                ),
                complete: array_done,
            },
        ],
        complete: false,
    }
}

/// Resolve "noun(s)" placeholders using the nearest preceding count: "1 region(s)" becomes
/// "1 region" and "3 region(s)" becomes "3 regions".
pub(crate) fn resolve_plurals(text: &str) -> String {
    let tokens: Vec<&str> = text.split(' ').collect();
    let mut out: Vec<String> = Vec::with_capacity(tokens.len());
    for (index, token) in tokens.iter().enumerate() {
        if let Some(at) = token.find("(s)") {
            let count = tokens[..index].iter().rev().take(4).find_map(|previous| {
                previous
                    .trim_matches(|c: char| !c.is_ascii_digit())
                    .parse::<u64>()
                    .ok()
            });
            let suffix = if count == Some(1) { "" } else { "s" };
            out.push(format!("{}{suffix}{}", &token[..at], &token[at + 3..]));
        } else {
            out.push((*token).to_owned());
        }
    }
    out.join(" ")
}

/// Recovery guidance for the first offline or failing node, matched to its visible cause so the
/// suggested key actually restores service.
fn node_recovery(input: &CampaignInput<'_>, region_id: &str) -> (String, &'static str) {
    let label = region_label(input.regions, region_id);
    let Some(base) = input
        .bases
        .iter()
        .find(|base| base.region_id == region_id && (!base.connected || base.integrity <= 20))
    else {
        return (
            format!("Stabilize node at {label}: open its local field (V) and restore its modules."),
            "V",
        );
    };
    let suspended = |kind: BaseModuleKind| {
        base.modules
            .iter()
            .any(|module| module.kind == kind && !module.enabled)
    };
    let project_here = input
        .projects
        .iter()
        .any(|project| project.subregion_id == base.subregion_id);
    if suspended(BaseModuleKind::PowerArray) {
        (
            format!(
                "Restore power at {label}: press P on its node to bring the array back online."
            ),
            "P",
        )
    } else if suspended(BaseModuleKind::Communications) {
        (
            format!(
                "Restore communications at {label}: press C on its node to rejoin the network."
            ),
            "C",
        )
    } else if base.integrity <= 20 && project_here {
        (
            format!("Work is underway at the {label} node; integrity recovers once it completes."),
            "V",
        )
    } else if base.integrity <= 20 {
        (
            format!("Fund maintenance (M) at the {label} node to restore integrity."),
            "M",
        )
    } else {
        (
            format!(
                "Node at {label} is overloaded or out of range: suspend optional modules (D/W/L/O) or extend communications."
            ),
            "V",
        )
    }
}

/// Next step toward the required powered Observation Arrays: counts arrays already under
/// construction, picks a connected node that can host one, and names any material shortfall.
fn observation_array_lead(
    input: &CampaignInput<'_>,
) -> (String, &'static str, Option<String>, Option<String>) {
    let h = input.horizon;
    let building = input
        .projects
        .iter()
        .filter(|project| project.module == Some(BaseModuleKind::Observatory))
        .count();
    let still_needed = h
        .required_observation_sites
        .saturating_sub(h.observation_sites + building);
    if still_needed == 0 {
        return (
            format!(
                "{building} Observation Array project(s) underway; keep their nodes powered and connected."
            ),
            "TAB",
            None,
            None,
        );
    }
    // A suspended array is the cheapest fix: restore it before building another.
    if let Some(base) = input.bases.iter().find(|base| {
        base.modules
            .iter()
            .any(|module| module.kind == BaseModuleKind::Observatory && !module.enabled)
    }) {
        return (
            format!(
                "Restore the suspended Observation Array at {}: press O on its node.",
                region_label(input.regions, &base.region_id)
            ),
            "O",
            Some(base.region_id.clone()),
            Some(base.subregion_id.clone()),
        );
    }
    let host = input.bases.iter().find(|base| {
        base.connected
            && !base
                .modules
                .iter()
                .any(|module| module.kind == BaseModuleKind::Observatory)
            && !input
                .projects
                .iter()
                .any(|project| project.subregion_id == base.subregion_id)
    });
    let noun = if still_needed == 1 {
        "Observation Array"
    } else {
        "Observation Arrays"
    };
    let Some(host) = host else {
        return (
            format!(
                "Connect another powered node to host {still_needed} more {noun} (V -> B, P, C)."
            ),
            "V",
            first_surveyed_without_base(input.regions, input.subregions),
            None,
        );
    };
    let cost = BaseModuleKind::Observatory.cost();
    let have = input.refined_inventory;
    let short = ResourceStock {
        structural: cost.structural.saturating_sub(have.structural),
        conductive: cost.conductive.saturating_sub(have.conductive),
        silicate: cost.silicate.saturating_sub(have.silicate),
    };
    let label = region_label(input.regions, &host.region_id);
    let raw = input.raw_inventory;
    let refining_covers = short.structural <= raw.structural
        && short.conductive <= raw.conductive
        && short.silicate <= raw.silicate;
    if short.structural + short.conductive + short.silicate > 0 && refining_covers {
        return (
            format!(
                "Material for the next {noun} is refining; press O on the {label} node once it is ready."
            ),
            "O",
            Some(host.region_id.clone()),
            Some(host.subregion_id.clone()),
        );
    }
    let short = ResourceStock {
        structural: short.structural.saturating_sub(raw.structural),
        conductive: short.conductive.saturating_sub(raw.conductive),
        silicate: short.silicate.saturating_sub(raw.silicate),
    };
    if short.structural + short.conductive + short.silicate > 0 {
        let mut parts = Vec::new();
        if short.structural > 0 {
            parts.push(format!("S {}", short.structural));
        }
        if short.conductive > 0 {
            parts.push(format!("C {}", short.conductive));
        }
        if short.silicate > 0 {
            parts.push(format!("Si {}", short.silicate));
        }
        let deposit = first_deposit_for(input.regions, short);
        return (
            format!(
                "Extract {} more refined material (X) for the next {noun}, then press O at {label}.",
                parts.join(" / ")
            ),
            "X",
            deposit.or_else(|| Some(host.region_id.clone())),
            None,
        );
    }
    let more = if h.observation_sites + building == 0 {
        ""
    } else {
        " more"
    };
    (
        format!("Build {still_needed}{more} powered {noun}: press O on the {label} node."),
        "O",
        Some(host.region_id.clone()),
        Some(host.subregion_id.clone()),
    )
}

fn region_label<'a>(regions: &'a [RegionSnapshot], region_id: &'a str) -> &'a str {
    regions
        .iter()
        .find(|region| region.id == region_id)
        .map_or(region_id, |region| region.designation.as_str())
}

fn first_unsurveyed_reachable(regions: &[RegionSnapshot]) -> Option<String> {
    regions
        .iter()
        .filter(|region| {
            !region.core
                && region.within_scout_range
                && region.knowledge < RegionKnowledge::Surveyed
        })
        .min_by_key(|region| region.hop_distance_from_core)
        .map(|region| region.id.clone())
}

fn first_unsurveyed_beyond_core_range(regions: &[RegionSnapshot]) -> Option<String> {
    regions
        .iter()
        .filter(|region| {
            !region.core
                && region.within_scout_range
                && region.hop_distance_from_core > 2
                && region.knowledge < RegionKnowledge::Surveyed
        })
        .min_by_key(|region| region.hop_distance_from_core)
        .map(|region| region.id.clone())
}

fn first_surveyed_reachable(regions: &[RegionSnapshot]) -> Option<String> {
    regions
        .iter()
        .filter(|region| {
            !region.core
                && region.within_scout_range
                && region.knowledge >= RegionKnowledge::Surveyed
        })
        .max_by_key(|region| region.hop_distance_from_core)
        .map(|region| region.id.clone())
}

/// First reachable surveyed deposit of a resource in `short`, so extraction guidance always
/// points at material the next build actually needs.
fn first_deposit_for(regions: &[RegionSnapshot], short: ResourceStock) -> Option<String> {
    let needed = |kind: ResourceKind| match kind {
        ResourceKind::Structural => short.structural > 0,
        ResourceKind::Conductive => short.conductive > 0,
        ResourceKind::Silicate => short.silicate > 0,
    };
    regions
        .iter()
        .find(|region| {
            region.within_scout_range
                && region.knowledge >= RegionKnowledge::Surveyed
                && region
                    .deposit
                    .as_ref()
                    .is_some_and(|deposit| deposit.remaining > 0 && needed(deposit.resource))
        })
        .map(|region| region.id.clone())
}

fn first_surveyed_deposit(regions: &[RegionSnapshot], refined: ResourceStock) -> Option<String> {
    let needs_any = refined.structural < 8 || refined.conductive < 6 || refined.silicate < 6;
    let missing_resource = |kind: ResourceKind| match kind {
        ResourceKind::Structural => refined.structural < 8,
        ResourceKind::Conductive => refined.conductive < 6,
        ResourceKind::Silicate => refined.silicate < 6,
    };
    let needed = regions.iter().find(|region| {
        region.within_scout_range
            && region.knowledge >= RegionKnowledge::Surveyed
            && region
                .deposit
                .as_ref()
                .is_some_and(|deposit| deposit.remaining > 0 && missing_resource(deposit.resource))
    });
    if needed.is_some() || needs_any {
        return needed.map(|region| region.id.clone());
    }
    regions
        .iter()
        .find(|region| {
            region.within_scout_range
                && region.knowledge >= RegionKnowledge::Surveyed
                && region
                    .deposit
                    .as_ref()
                    .is_some_and(|deposit| deposit.remaining > 0)
        })
        .map(|region| region.id.clone())
}

fn first_surveyed_with_detected_site(
    regions: &[RegionSnapshot],
    subregions: &[SubregionSnapshot],
) -> Option<String> {
    regions
        .iter()
        .find(|region| {
            !region.core
                && region.within_scout_range
                && region.knowledge >= RegionKnowledge::Surveyed
                && subregions.iter().any(|site| {
                    site.region_id == region.id && site.knowledge == SubregionKnowledge::Detected
                })
        })
        .map(|region| region.id.clone())
}

fn first_surveyed_with_observed_site(
    regions: &[RegionSnapshot],
    subregions: &[SubregionSnapshot],
) -> Option<String> {
    regions
        .iter()
        .find(|region| {
            region.within_scout_range
                && region.knowledge >= RegionKnowledge::Surveyed
                && subregions.iter().any(|site| {
                    site.region_id == region.id
                        && site.knowledge >= SubregionKnowledge::Surveyed
                        && site.base_id.is_none()
                })
        })
        .map(|region| region.id.clone())
}

fn first_surveyed_without_base(
    regions: &[RegionSnapshot],
    subregions: &[SubregionSnapshot],
) -> Option<String> {
    regions
        .iter()
        .find(|region| {
            !region.core
                && region.within_scout_range
                && region.knowledge >= RegionKnowledge::Surveyed
                && !subregions
                    .iter()
                    .any(|site| site.region_id == region.id && site.base_id.is_some())
        })
        .map(|region| region.id.clone())
}

fn first_base_region(subregions: &[SubregionSnapshot]) -> Option<String> {
    subregions
        .iter()
        .find(|site| site.base_id.is_some())
        .map(|site| site.region_id.clone())
}

fn first_uncompared_surveyed_region(
    regions: &[RegionSnapshot],
    subregions: &[SubregionSnapshot],
    compared: &[String],
) -> Option<String> {
    regions
        .iter()
        .find(|region| {
            !region.core
                && region.within_scout_range
                && region.knowledge >= RegionKnowledge::Surveyed
                && !compared.iter().any(|id| id == &region.id)
                && subregions.iter().any(|site| {
                    site.region_id == region.id && site.knowledge == SubregionKnowledge::Detected
                })
        })
        .map(|region| region.id.clone())
}

#[cfg(test)]
mod tests {
    use super::resolve_plurals;

    #[test]
    fn plural_placeholders_follow_the_nearest_count() {
        assert_eq!(
            resolve_plurals("Resolve 1 more local sector(s) via V -> S."),
            "Resolve 1 more local sector via V -> S."
        );
        assert_eq!(
            resolve_plurals("Connect 2 more powered node(s) (V -> B, P, C)."),
            "Connect 2 more powered nodes (V -> B, P, C)."
        );
        assert_eq!(resolve_plurals("(3 region(s) left)."), "(3 regions left).");
    }
}
