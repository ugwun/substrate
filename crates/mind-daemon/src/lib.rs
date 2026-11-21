use anyhow::{Context, Result};
use mind_core::{
    config::{AppPaths, Config},
    engine::MindEngine,
    protocol::{
        self, failure, method, notification, success, AcknowledgeActivityParams,
        ActivateProfileParams, CompleteOnboardingParams, ConfigureExtractorParams,
        ConstructBaseModuleParams, ConstructRelayParams, DeployExtractorParams, DeployScoutParams,
        PlanetNotification, RegionAutomationParams, RpcRequest, SetBaseModuleEnabledParams,
        StartResearchParams, StatusNotification, SubregionParams, SubscribeResult,
        UnsubscribeParams, JSONRPC_VERSION, MAX_FRAME_BYTES,
    },
    runtime::RuntimeMetadata,
};
use serde_json::Value;
use std::{sync::Arc, time::Duration};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{tcp::OwnedWriteHalf, TcpListener, TcpStream},
    sync::{mpsc, watch, Mutex},
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};
use uuid::Uuid;

pub async fn run(config: Config, paths: AppPaths) -> Result<()> {
    config.validate().context("invalid daemon configuration")?;
    let listener = TcpListener::bind(config.address)
        .await
        .with_context(|| format!("failed to bind IPC listener at {}", config.address))?;
    let address = listener
        .local_addr()
        .context("failed to inspect IPC listener")?;
    let mcp_listener = TcpListener::bind(config.mcp_address)
        .await
        .with_context(|| format!("failed to bind MCP listener at {}", config.mcp_address))?;
    let mcp_address = mcp_listener
        .local_addr()
        .context("failed to inspect MCP listener")?;
    let engine = MindEngine::persistent(
        config.tick_rate_hz,
        mcp_address,
        paths.game_state_file.clone(),
    )
    .with_context(|| format!("failed to load {}", paths.game_state_file.display()))?;
    let started_at = engine.status().started_at;
    let metadata = RuntimeMetadata::current(address, mcp_address, started_at);
    metadata
        .write_atomic(&paths.runtime_file)
        .with_context(|| format!("failed to write {}", paths.runtime_file.display()))?;

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let engine_task = tokio::spawn(engine.clone().run(shutdown_rx.clone()));
    let telemetry_task = tokio::spawn(sample_process_cpu(engine.clone(), shutdown_rx.clone()));
    let persistence_task = tokio::spawn(checkpoint_game_state(engine.clone(), shutdown_rx.clone()));
    let mcp_cancellation = CancellationToken::new();
    let (mcp_exit_tx, mut mcp_exit_rx) = mpsc::channel(1);
    let mcp_task = tokio::spawn({
        let engine = engine.clone();
        let cancellation = mcp_cancellation.clone();
        async move {
            let result = mind_mcp::serve_http(mcp_listener, engine, cancellation).await;
            let message = result
                .as_ref()
                .map_or_else(|error| error.to_string(), |()| "server stopped".to_owned());
            let _ = mcp_exit_tx.send(message).await;
            result
        }
    });
    let mut connections = JoinSet::new();
    let mut unexpected_mcp_exit = None;
    info!(%address, %mcp_address, pid = std::process::id(), "mind daemon ready");

    loop {
        tokio::select! {
            accepted = listener.accept() => {
                match accepted {
                    Ok((stream, peer)) => {
                        debug!(%peer, "IPC client connected");
                        connections.spawn(handle_connection(
                            stream,
                            engine.clone(),
                            shutdown_tx.clone(),
                            shutdown_rx.clone(),
                            Duration::from_millis(config.status_publish_interval_ms),
                        ));
                    }
                    Err(error) => warn!(%error, "failed to accept IPC client"),
                }
            }
            signal = tokio::signal::ctrl_c() => {
                if let Err(error) = signal {
                    warn!(%error, "failed to listen for Ctrl-C");
                }
                info!("shutdown requested by signal");
                engine.begin_shutdown();
                let _ = shutdown_tx.send(true);
                break;
            }
            changed = wait_for_shutdown(shutdown_rx.clone()) => {
                if changed {
                    info!("shutdown requested over IPC");
                    engine.begin_shutdown();
                    break;
                }
            }
            mcp_exit = mcp_exit_rx.recv() => {
                engine.begin_shutdown();
                let _ = shutdown_tx.send(true);
                let detail = mcp_exit.unwrap_or_else(|| "MCP task ended without a result".to_owned());
                unexpected_mcp_exit = Some(detail);
                break;
            }
        }
    }

    let _ = shutdown_tx.send(true);
    mcp_cancellation.cancel();
    while connections.join_next().await.is_some() {}
    let engine_result = engine_task.await.context("engine task failed");
    let telemetry_result = telemetry_task.await.context("telemetry task failed");
    let persistence_result = persistence_task.await.context("persistence task failed");
    let mcp_result = match mcp_task.await {
        Ok(result) => result.context("MCP HTTP server failed during shutdown"),
        Err(error) => Err(anyhow::Error::new(error).context("MCP HTTP task failed")),
    };
    remove_own_metadata(&paths, metadata.pid);
    engine_result?;
    telemetry_result?;
    persistence_result??;
    mcp_result?;
    if let Some(detail) = unexpected_mcp_exit {
        anyhow::bail!("MCP HTTP server exited unexpectedly: {detail}");
    }
    info!("mind daemon stopped");
    Ok(())
}

async fn checkpoint_game_state(
    engine: Arc<MindEngine>,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), mind_core::game::GameError> {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    interval.tick().await;
    loop {
        tokio::select! {
            _ = interval.tick() => {
                if let Err(error) = engine.checkpoint_game_state() {
                    warn!(%error, "game-state checkpoint failed; will retry");
                }
            }
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    return engine.checkpoint_game_state();
                }
            }
        }
    }
}

async fn sample_process_cpu(engine: Arc<MindEngine>, mut shutdown: watch::Receiver<bool>) {
    let pid = Pid::from_u32(std::process::id());
    let refresh = ProcessRefreshKind::nothing().with_cpu();
    let mut system = System::new();
    system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, refresh);
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    interval.tick().await;

    loop {
        tokio::select! {
            _ = interval.tick() => {
                system.refresh_processes_specifics(
                    ProcessesToUpdate::Some(&[pid]),
                    true,
                    refresh,
                );
                engine.set_cpu_usage_percent(system.process(pid).map(|process| process.cpu_usage()));
            }
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    break;
                }
            }
        }
    }
}

async fn wait_for_shutdown(mut shutdown: watch::Receiver<bool>) -> bool {
    if *shutdown.borrow() {
        return true;
    }
    shutdown.changed().await.is_ok() && *shutdown.borrow()
}

fn remove_own_metadata(paths: &AppPaths, pid: u32) {
    match RuntimeMetadata::read(&paths.runtime_file) {
        Ok(Some(current)) if current.pid == pid => {
            if let Err(error) = RuntimeMetadata::remove(&paths.runtime_file) {
                warn!(%error, "failed to remove runtime metadata");
            }
        }
        Ok(_) => {}
        Err(error) => warn!(%error, "failed to inspect runtime metadata during shutdown"),
    }
}

struct ClientGuard(Arc<MindEngine>);

impl Drop for ClientGuard {
    fn drop(&mut self) {
        self.0.client_disconnected();
    }
}

#[derive(Default)]
struct ClientSubscriptions {
    status: Option<Uuid>,
    planet: Option<Uuid>,
    planet_revision: Option<u64>,
}

async fn handle_connection(
    stream: TcpStream,
    engine: Arc<MindEngine>,
    shutdown_tx: watch::Sender<bool>,
    mut shutdown_rx: watch::Receiver<bool>,
    publish_interval: Duration,
) {
    engine.client_connected();
    let _guard = ClientGuard(engine.clone());
    let (reader, writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let writer = Arc::new(Mutex::new(writer));
    let mut interval = tokio::time::interval(publish_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    interval.tick().await;
    let mut subscriptions = ClientSubscriptions::default();

    loop {
        let mut frame = Vec::new();
        tokio::select! {
            read = reader.read_until(b'\n', &mut frame) => {
                match read {
                    Ok(0) => break,
                    Ok(_) if frame.len() > MAX_FRAME_BYTES + 1 => {
                        let response = failure(Value::Null, -32600, "request exceeds 1 MiB frame limit");
                        let _ = write_value(&writer, &response).await;
                        break;
                    }
                    Ok(_) => {
                        let should_close = dispatch_frame(
                            &frame,
                            &writer,
                            &engine,
                            &shutdown_tx,
                            &mut subscriptions,
                        ).await;
                        if should_close {
                            break;
                        }
                    }
                    Err(error) => {
                        debug!(%error, "IPC client read failed");
                        break;
                    }
                }
            }
            _ = interval.tick(), if subscriptions.status.is_some() || subscriptions.planet.is_some() => {
                if let Some(subscription_id) = subscriptions.status {
                    let params = StatusNotification {
                        subscription_id,
                        status: engine.status(),
                    };
                    if write_value(&writer, &notification(method::STATUS_CHANGED, params)).await.is_err() {
                        break;
                    }
                }
                if let (Some(subscription_id), Some(planet)) = (subscriptions.planet, engine.planet()) {
                    if subscriptions.planet_revision != Some(planet.revision) {
                        let revision = planet.revision;
                        let params = PlanetNotification {
                            subscription_id,
                            planet,
                        };
                        if write_value(&writer, &notification(method::PLANET_CHANGED, params)).await.is_err() {
                            break;
                        }
                        subscriptions.planet_revision = Some(revision);
                    }
                }
            }
            changed = shutdown_rx.changed() => {
                if changed.is_err() || *shutdown_rx.borrow() {
                    break;
                }
            }
        }
    }
}

async fn dispatch_frame(
    frame: &[u8],
    writer: &Arc<Mutex<OwnedWriteHalf>>,
    engine: &Arc<MindEngine>,
    shutdown_tx: &watch::Sender<bool>,
    subscriptions: &mut ClientSubscriptions,
) -> bool {
    let request: RpcRequest = match serde_json::from_slice(frame) {
        Ok(request) => request,
        Err(_) => {
            let _ = write_value(writer, &failure(Value::Null, -32700, "parse error")).await;
            return false;
        }
    };
    if request.jsonrpc != JSONRPC_VERSION {
        let _ = write_value(
            writer,
            &failure(request.id, -32600, "jsonrpc must be \"2.0\""),
        )
        .await;
        return false;
    }

    let (response, shutdown) = match request.method.as_str() {
        method::PING => (success(request.id, protocol::PingResult::current()), false),
        method::GET_STATUS => (success(request.id, engine.status()), false),
        method::START_NEW_PLAYTHROUGH => {
            (success(request.id, engine.start_new_playthrough()), false)
        }
        method::TRIGGER_DIAGNOSTIC_PULSE => (
            success(request.id, engine.trigger_diagnostic_pulse()),
            false,
        ),
        method::COMPLETE_ONBOARDING => {
            let params = request
                .params
                .and_then(|value| serde_json::from_value::<CompleteOnboardingParams>(value).ok());
            match params {
                Some(params) => (
                    success(
                        request.id,
                        engine.complete_onboarding(params.band_positions),
                    ),
                    false,
                ),
                None => (
                    failure(request.id, -32602, "invalid onboarding parameters"),
                    false,
                ),
            }
        }
        method::GET_PLANET => match engine.planet() {
            Some(planet) => (success(request.id, planet), false),
            None => (failure(request.id, -32010, "onboarding required"), false),
        },
        method::DEPLOY_SCOUT => {
            let params = request
                .params
                .and_then(|value| serde_json::from_value::<DeployScoutParams>(value).ok());
            match params {
                Some(params) => (
                    success(request.id, engine.deploy_scout(&params.region_id)),
                    false,
                ),
                None => (
                    failure(request.id, -32602, "invalid scout deployment parameters"),
                    false,
                ),
            }
        }
        method::RECALL_SCOUT => (success(request.id, engine.recall_scout()), false),
        method::DEPLOY_EXTRACTOR => {
            let params = request
                .params
                .and_then(|value| serde_json::from_value::<DeployExtractorParams>(value).ok());
            match params {
                Some(params) => (
                    success(
                        request.id,
                        engine
                            .deploy_extractor_at(&params.region_id, params.subregion_id.as_deref()),
                    ),
                    false,
                ),
                None => (
                    failure(
                        request.id,
                        -32602,
                        "invalid extractor deployment parameters",
                    ),
                    false,
                ),
            }
        }
        method::CONFIGURE_EXTRACTOR => {
            let params = request
                .params
                .and_then(|value| serde_json::from_value::<ConfigureExtractorParams>(value).ok());
            match params {
                Some(params) => (
                    success(request.id, engine.configure_extractor(params.loadout)),
                    false,
                ),
                None => (
                    failure(
                        request.id,
                        -32602,
                        "invalid extractor configuration parameters",
                    ),
                    false,
                ),
            }
        }
        method::CONSTRUCT_RELAY => {
            let params = request
                .params
                .and_then(|value| serde_json::from_value::<ConstructRelayParams>(value).ok());
            match params {
                Some(params) => (
                    success(request.id, engine.construct_relay(&params.region_id)),
                    false,
                ),
                None => (
                    failure(request.id, -32602, "invalid relay construction parameters"),
                    false,
                ),
            }
        }
        method::SURVEY_SUBREGION | method::ESTABLISH_BASE | method::MAINTAIN_BASE => {
            let method_name = request.method.clone();
            let params = request
                .params
                .and_then(|value| serde_json::from_value::<SubregionParams>(value).ok());
            match params {
                Some(params) => {
                    let result = match method_name.as_str() {
                        method::SURVEY_SUBREGION => engine.survey_subregion(&params.subregion_id),
                        method::ESTABLISH_BASE => engine.establish_base(&params.subregion_id),
                        _ => engine.maintain_base(&params.subregion_id),
                    };
                    (success(request.id, result), false)
                }
                None => (
                    failure(request.id, -32602, "invalid subregion parameters"),
                    false,
                ),
            }
        }
        method::CONSTRUCT_BASE_MODULE => {
            let params = request
                .params
                .and_then(|value| serde_json::from_value::<ConstructBaseModuleParams>(value).ok());
            match params {
                Some(params) => (
                    success(
                        request.id,
                        engine.construct_base_module(&params.subregion_id, params.module),
                    ),
                    false,
                ),
                None => (
                    failure(request.id, -32602, "invalid base module parameters"),
                    false,
                ),
            }
        }
        method::SET_BASE_MODULE_ENABLED => {
            let params = request
                .params
                .and_then(|value| serde_json::from_value::<SetBaseModuleEnabledParams>(value).ok());
            match params {
                Some(params) => (
                    success(
                        request.id,
                        engine.set_base_module_enabled(
                            &params.subregion_id,
                            params.module,
                            params.enabled,
                        ),
                    ),
                    false,
                ),
                None => (
                    failure(request.id, -32602, "invalid base module mode parameters"),
                    false,
                ),
            }
        }
        method::START_RESEARCH => {
            let params = request
                .params
                .and_then(|value| serde_json::from_value::<StartResearchParams>(value).ok());
            match params {
                Some(params) => (
                    success(request.id, engine.start_research(params.project)),
                    false,
                ),
                None => (
                    failure(request.id, -32602, "invalid research parameters"),
                    false,
                ),
            }
        }
        method::START_ARCHIVE_EXPERIMENT => (
            success(request.id, engine.start_archive_experiment()),
            false,
        ),
        method::ACTIVATE_PROFILE => {
            let params = request
                .params
                .and_then(|value| serde_json::from_value::<ActivateProfileParams>(value).ok());
            match params {
                Some(params) => (
                    success(request.id, engine.activate_profile(params.profile)),
                    false,
                ),
                None => (
                    failure(request.id, -32602, "invalid profile parameters"),
                    false,
                ),
            }
        }
        method::QUEUE_SURVEY | method::REMOVE_QUEUED_SURVEY | method::SET_EXTRACTION_ROUTE => {
            let method_name = request.method.clone();
            let params = request
                .params
                .and_then(|value| serde_json::from_value::<RegionAutomationParams>(value).ok());
            match params {
                Some(params) => {
                    let result = match method_name.as_str() {
                        method::QUEUE_SURVEY => engine.queue_survey(&params.region_id),
                        method::REMOVE_QUEUED_SURVEY => {
                            engine.remove_queued_survey(&params.region_id)
                        }
                        _ => engine.set_extraction_route(&params.region_id),
                    };
                    (success(request.id, result), false)
                }
                None => (
                    failure(request.id, -32602, "invalid automation parameters"),
                    false,
                ),
            }
        }
        method::CLEAR_EXTRACTION_ROUTE => {
            (success(request.id, engine.clear_extraction_route()), false)
        }
        method::ACKNOWLEDGE_ACTIVITY => {
            let params = request
                .params
                .and_then(|value| serde_json::from_value::<AcknowledgeActivityParams>(value).ok());
            match params {
                Some(params) => (
                    success(
                        request.id,
                        engine.acknowledge_activity(params.through_activity_id),
                    ),
                    false,
                ),
                None => (
                    failure(request.id, -32602, "invalid activity acknowledgement"),
                    false,
                ),
            }
        }
        method::SUBSCRIBE_STATUS => {
            let id = subscriptions.status.get_or_insert_with(Uuid::new_v4);
            (
                success(
                    request.id,
                    SubscribeResult {
                        subscription_id: *id,
                    },
                ),
                false,
            )
        }
        method::UNSUBSCRIBE_STATUS => {
            let params = request
                .params
                .and_then(|value| serde_json::from_value::<UnsubscribeParams>(value).ok());
            match params {
                Some(params) => {
                    let removed = subscriptions.status.as_ref() == Some(&params.subscription_id);
                    if removed {
                        subscriptions.status = None;
                    }
                    (success(request.id, removed), false)
                }
                None => (
                    failure(request.id, -32602, "invalid unsubscribe parameters"),
                    false,
                ),
            }
        }
        method::SUBSCRIBE_PLANET => match engine.planet() {
            None => (failure(request.id, -32010, "onboarding required"), false),
            Some(planet) => {
                if let Some(id) = subscriptions.planet {
                    (
                        success(
                            request.id,
                            SubscribeResult {
                                subscription_id: id,
                            },
                        ),
                        false,
                    )
                } else {
                    let id = Uuid::new_v4();
                    subscriptions.planet = Some(id);
                    subscriptions.planet_revision = Some(planet.revision);
                    (
                        success(
                            request.id,
                            SubscribeResult {
                                subscription_id: id,
                            },
                        ),
                        false,
                    )
                }
            }
        },
        method::UNSUBSCRIBE_PLANET => {
            let params = request
                .params
                .and_then(|value| serde_json::from_value::<UnsubscribeParams>(value).ok());
            match params {
                Some(params) => {
                    let removed = subscriptions.planet.as_ref() == Some(&params.subscription_id);
                    if removed {
                        subscriptions.planet = None;
                        subscriptions.planet_revision = None;
                    }
                    (success(request.id, removed), false)
                }
                None => (
                    failure(request.id, -32602, "invalid unsubscribe parameters"),
                    false,
                ),
            }
        }
        method::SHUTDOWN => (success(request.id, true), true),
        _ => (failure(request.id, -32601, "method not found"), false),
    };

    if write_value(writer, &response).await.is_err() {
        return true;
    }
    if shutdown {
        engine.begin_shutdown();
        let _ = shutdown_tx.send(true);
    }
    shutdown
}

async fn write_value(writer: &Arc<Mutex<OwnedWriteHalf>>, value: &Value) -> std::io::Result<()> {
    let mut encoded = serde_json::to_vec(value).map_err(std::io::Error::other)?;
    encoded.push(b'\n');
    let mut writer = writer.lock().await;
    writer.write_all(&encoded).await?;
    writer.flush().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use mind_core::config::Config;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    #[tokio::test]
    async fn malformed_json_returns_parse_error() {
        let engine = MindEngine::new(20);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            handle_connection(
                stream,
                engine,
                shutdown_tx,
                shutdown_rx,
                Duration::from_secs(1),
            )
            .await;
        });
        let mut stream = TcpStream::connect(address).await.unwrap();
        stream.write_all(b"not-json\n").await.unwrap();
        let mut response = String::new();
        BufReader::new(stream)
            .read_line(&mut response)
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&response).unwrap()["error"]["code"],
            -32700
        );
        server.abort();
        let _ = Config::default();
    }

    #[tokio::test]
    async fn complete_onboarding_rejects_missing_parameters() {
        let engine = MindEngine::new(20);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            handle_connection(
                stream,
                engine,
                shutdown_tx,
                shutdown_rx,
                Duration::from_secs(1),
            )
            .await;
        });
        let mut stream = TcpStream::connect(address).await.unwrap();
        stream
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"mind.complete_onboarding\"}\n")
            .await
            .unwrap();
        let mut response = String::new();
        BufReader::new(stream)
            .read_line(&mut response)
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&response).unwrap()["error"]["code"],
            -32602
        );
        server.abort();
    }

    #[tokio::test]
    async fn retired_prototype_method_is_not_found() {
        let engine = MindEngine::new(20);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            handle_connection(
                stream,
                engine,
                shutdown_tx,
                shutdown_rx,
                Duration::from_secs(1),
            )
            .await;
        });
        let mut stream = TcpStream::connect(address).await.unwrap();
        stream
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"mind.execute_dsl\"}\n")
            .await
            .unwrap();
        let mut response = String::new();
        BufReader::new(stream)
            .read_line(&mut response)
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&response).unwrap()["error"]["code"],
            -32601
        );
        server.abort();
    }
}
