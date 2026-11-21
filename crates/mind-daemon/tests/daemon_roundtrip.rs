use mind_core::{
    config::{AppPaths, Config},
    ipc::{IpcClient, IpcNotification},
    protocol::{BaseModuleKind, ExtractorLoadout, OperationalProfile, ResearchProject, ScoutState},
    runtime::RuntimeMetadata,
};
use std::{net::SocketAddr, time::Duration};
use tempfile::TempDir;

fn paths(temp: &TempDir) -> AppPaths {
    AppPaths {
        config_file: temp.path().join("config.toml"),
        data_dir: temp.path().join("data"),
        runtime_file: temp.path().join("data/daemon.json"),
        log_file: temp.path().join("data/mind-daemon.log"),
        game_state_file: temp.path().join("data/game_state.json"),
    }
}

async fn wait_for_metadata(runtime_file: &std::path::Path) -> RuntimeMetadata {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(metadata) = RuntimeMetadata::read(runtime_file).unwrap() {
                break metadata;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn serves_status_subscription_and_shutdown() {
    let temp = TempDir::new().unwrap();
    let paths = paths(&temp);
    paths.ensure_directories().unwrap();
    let config = Config {
        address: "127.0.0.1:0".parse().unwrap(),
        mcp_address: "127.0.0.1:0".parse().unwrap(),
        tick_rate_hz: 100,
        status_publish_interval_ms: 50,
    };
    let runtime_file = paths.runtime_file.clone();
    let daemon = tokio::spawn(mind_daemon::run(config, paths));

    let metadata = wait_for_metadata(&runtime_file).await;

    let mut client = IpcClient::connect(metadata.address).await.unwrap();
    assert!(tokio::net::TcpStream::connect(metadata.mcp_address)
        .await
        .is_ok());
    let initial = client.status().await.unwrap();
    assert_eq!(initial.tick_rate_hz, 100);
    assert_eq!(initial.protocol_version, mind_core::PROTOCOL_VERSION);
    assert!(!initial.onboarding.complete);
    assert!(client.planet().await.is_err());
    assert!(client.subscribe_planet().await.is_err());
    let rejected = client.complete_onboarding([1, 2, 1]).await.unwrap();
    assert_eq!(rejected.code.as_deref(), Some("not_coherent"));
    let completed = client.complete_onboarding([3, 3, 3]).await.unwrap();
    assert!(completed.ok);
    assert!(client.status().await.unwrap().onboarding.complete);
    let planet = client.planet().await.unwrap();
    assert_eq!(planet.latitude_bands, 10);
    assert_eq!(planet.longitude_sectors, 20);
    assert_eq!(planet.regions.len(), 200);
    assert_eq!(planet.archive.records.len(), 1);
    assert!(planet.archive.experiment.is_none());
    let public_planet = serde_json::to_string(&planet).unwrap();
    assert!(!public_planet.contains("archive_comparison_region_ids"));
    assert!(!public_planet.contains("active_archive_experiment"));
    let target = planet
        .regions
        .iter()
        .find(|region| region.hop_distance_from_core == 1)
        .unwrap()
        .id
        .clone();
    assert!(
        client
            .configure_extractor(ExtractorLoadout::Cargo)
            .await
            .unwrap()
            .ok
    );
    assert_eq!(
        client.planet().await.unwrap().extractor.loadout,
        ExtractorLoadout::Cargo
    );
    assert_eq!(
        client
            .configure_extractor(ExtractorLoadout::Cargo)
            .await
            .unwrap()
            .code
            .as_deref(),
        Some("loadout_already_configured")
    );
    let subscription_id = client.subscribe_status().await.unwrap();
    let planet_subscription_id = client.subscribe_planet().await.unwrap();
    assert_eq!(
        client
            .deploy_extractor(target.clone())
            .await
            .unwrap()
            .code
            .as_deref(),
        Some("survey_required")
    );
    assert_eq!(
        client
            .construct_relay(target.clone())
            .await
            .unwrap()
            .code
            .as_deref(),
        Some("survey_required")
    );
    assert_eq!(
        client
            .start_research(ResearchProject::ExtendedTelemetry)
            .await
            .unwrap()
            .code
            .as_deref(),
        Some("research_locked")
    );
    assert_eq!(
        client
            .start_archive_experiment()
            .await
            .unwrap()
            .code
            .as_deref(),
        Some("archive_observation_required")
    );
    assert_eq!(
        client
            .activate_profile(OperationalProfile::Reach)
            .await
            .unwrap()
            .code
            .as_deref(),
        Some("profile_locked")
    );
    assert_eq!(
        client
            .queue_survey(target.clone())
            .await
            .unwrap()
            .code
            .as_deref(),
        Some("automation_locked")
    );
    assert_eq!(
        client
            .remove_queued_survey(target.clone())
            .await
            .unwrap()
            .code
            .as_deref(),
        Some("survey_not_queued")
    );
    assert_eq!(
        client
            .set_extraction_route(target.clone())
            .await
            .unwrap()
            .code
            .as_deref(),
        Some("automation_locked")
    );
    assert_eq!(
        client
            .clear_extraction_route()
            .await
            .unwrap()
            .code
            .as_deref(),
        Some("no_extraction_route")
    );
    assert_eq!(
        client
            .set_base_module_enabled(target.clone(), BaseModuleKind::Communications, false)
            .await
            .unwrap()
            .code
            .as_deref(),
        Some("base_required")
    );
    assert!(client.acknowledge_activity(0).await.unwrap().ok);
    assert!(client.deploy_scout(target).await.unwrap().ok);
    let pulse = client.trigger_diagnostic_pulse().await.unwrap();
    assert_eq!(pulse.pulse_id, 1);
    assert_eq!(client.status().await.unwrap().last_diagnostic, Some(pulse));
    let update = client.next_status().await.unwrap();
    assert_eq!(update.subscription_id, subscription_id);
    assert!(update.status.tick >= initial.tick);
    let planet_update = tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if let IpcNotification::Planet(update) = client.next_notification().await.unwrap() {
                if update.subscription_id == planet_subscription_id {
                    break update.planet;
                }
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(planet_update.scout.state, ScoutState::Outbound);
    let cpu_status = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let update = client.next_status().await.unwrap();
            if update.status.cpu_usage_percent.is_some() {
                break update.status;
            }
        }
    })
    .await
    .unwrap();
    assert!(cpu_status.cpu_usage_percent.unwrap() >= 0.0);
    assert!(client.shutdown().await.unwrap());
    drop(client);

    tokio::time::timeout(Duration::from_secs(2), daemon)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(RuntimeMetadata::read(&runtime_file).unwrap().is_none());
}

#[tokio::test]
async fn creates_a_fresh_playthrough_over_ipc_without_stopping_the_daemon() {
    let temp = TempDir::new().unwrap();
    let paths = paths(&temp);
    paths.ensure_directories().unwrap();
    let config = Config {
        address: "127.0.0.1:0".parse().unwrap(),
        mcp_address: "127.0.0.1:0".parse().unwrap(),
        tick_rate_hz: 100,
        status_publish_interval_ms: 50,
    };
    let runtime_file = paths.runtime_file.clone();
    let game_state_file = paths.game_state_file.clone();
    let daemon = tokio::spawn(mind_daemon::run(config, paths));
    let metadata = wait_for_metadata(&runtime_file).await;
    let mut client = IpcClient::connect(metadata.address).await.unwrap();
    assert!(client.complete_onboarding([3, 3, 3]).await.unwrap().ok);
    let previous_revision = client.planet().await.unwrap().revision;

    let result = client.start_new_playthrough().await.unwrap();

    assert!(result.ok);
    assert!(!client.status().await.unwrap().onboarding.complete);
    assert!(client.planet().await.is_err());
    let loaded = mind_core::game::GameState::load(&game_state_file).unwrap();
    assert!(!loaded.state.onboarding().complete);
    assert!(loaded.state.state_revision() > previous_revision);
    assert!(client.shutdown().await.unwrap());
    drop(client);
    tokio::time::timeout(Duration::from_secs(2), daemon)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn graceful_restart_preserves_onboarding_completion_without_offline_progress() {
    let temp = TempDir::new().unwrap();
    let paths = paths(&temp);
    paths.ensure_directories().unwrap();
    let config = Config {
        address: "127.0.0.1:0".parse().unwrap(),
        mcp_address: "127.0.0.1:0".parse().unwrap(),
        tick_rate_hz: 100,
        status_publish_interval_ms: 50,
    };
    let runtime_file = paths.runtime_file.clone();

    let first_daemon = tokio::spawn(mind_daemon::run(config.clone(), paths.clone()));
    let first_metadata = wait_for_metadata(&runtime_file).await;
    let mut first_client = IpcClient::connect(first_metadata.address).await.unwrap();
    assert!(
        first_client
            .complete_onboarding([6, 6, 6])
            .await
            .unwrap()
            .ok
    );
    let initial_planet = first_client.planet().await.unwrap();
    assert_eq!(initial_planet.campaign.chapter_number, 1);
    assert_eq!(
        initial_planet.archive.unpresented_record_ids,
        vec!["observation_001".to_owned()]
    );
    assert!(
        first_client
            .acknowledge_archive_presentation("observation_001")
            .await
            .unwrap()
            .ok
    );
    assert!(first_client
        .planet()
        .await
        .unwrap()
        .archive
        .unpresented_record_ids
        .is_empty());
    let first_tick = first_client.status().await.unwrap().tick;
    let target = first_client
        .planet()
        .await
        .unwrap()
        .regions
        .into_iter()
        .find(|region| region.hop_distance_from_core == 1)
        .unwrap()
        .id;
    assert!(first_client.deploy_scout(target).await.unwrap().ok);
    tokio::time::sleep(Duration::from_millis(100)).await;
    let mission_before_stop = first_client.planet().await.unwrap().active_mission.unwrap();
    assert!(first_client.shutdown().await.unwrap());
    drop(first_client);
    tokio::time::timeout(Duration::from_secs(2), first_daemon)
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;
    let second_daemon = tokio::spawn(mind_daemon::run(config, paths));
    let second_metadata = wait_for_metadata(&runtime_file).await;
    let mut second_client = IpcClient::connect(second_metadata.address).await.unwrap();
    let restored = second_client.status().await.unwrap();
    assert!(restored.onboarding.complete);
    assert!(restored.tick <= first_tick + 5);
    let mission_after_restart = second_client
        .planet()
        .await
        .unwrap()
        .active_mission
        .unwrap();
    assert_eq!(
        mission_after_restart.mission_id,
        mission_before_stop.mission_id
    );
    assert!(mission_after_restart.remaining_ms <= mission_before_stop.remaining_ms);
    assert!(
        mission_before_stop.remaining_ms - mission_after_restart.remaining_ms <= 100,
        "offline time must not advance the mission"
    );
    assert!(second_client.shutdown().await.unwrap());
    drop(second_client);
    tokio::time::timeout(Duration::from_secs(2), second_daemon)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn refuses_non_loopback_configuration_before_run() {
    let config = Config {
        address: SocketAddr::from(([0, 0, 0, 0], 7776)),
        ..Config::default()
    };
    assert!(config.validate().is_err());
}

#[tokio::test]
async fn mcp_port_conflict_aborts_before_runtime_metadata() {
    let temp = TempDir::new().unwrap();
    let paths = paths(&temp);
    paths.ensure_directories().unwrap();
    let occupied = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let config = Config {
        address: "127.0.0.1:0".parse().unwrap(),
        mcp_address: occupied.local_addr().unwrap(),
        ..Config::default()
    };
    let runtime_file = paths.runtime_file.clone();
    let error = mind_daemon::run(config, paths).await.unwrap_err();
    assert!(error.to_string().contains("failed to bind MCP listener"));
    assert!(RuntimeMetadata::read(&runtime_file).unwrap().is_none());
}
