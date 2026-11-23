use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use mind_core::{
    config::{load_or_create, AppPaths, Config, ConfigOverrides},
    ipc::{IpcClient, IpcError},
    protocol::{PlanetSnapshot, RegionKnowledge, ScoutState, StatusSnapshot},
    runtime::RuntimeMetadata,
    DAEMON_BINARY, PROTOCOL_VERSION, TUI_BINARY,
};
use std::{
    env,
    net::SocketAddr,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{process::Command, time};

#[derive(Debug, Parser)]
#[command(version, about = "Manage and inspect the Substrate mind daemon")]
struct Cli {
    #[arg(long, env = "SUBSTRATE_CONFIG", global = true)]
    config: Option<PathBuf>,
    #[arg(long, env = "SUBSTRATE_ADDR", global = true)]
    addr: Option<SocketAddr>,
    #[arg(long, env = "SUBSTRATE_MCP_ADDR", global = true)]
    mcp_addr: Option<SocketAddr>,
    #[command(subcommand)]
    command: TopLevel,
}

#[derive(Debug, Subcommand)]
enum TopLevel {
    /// Start the daemon if needed, then open the game.
    Play {
        #[arg(long)]
        ascii: bool,
        #[arg(long)]
        no_color: bool,
        #[arg(long, env = "SUBSTRATE_TITLE")]
        title: bool,
        #[arg(long, env = "SUBSTRATE_BELL")]
        bell: bool,
        /// Skip the startup session menu and jump directly into the live dashboard.
        #[arg(long, env = "SUBSTRATE_RESUME")]
        resume: bool,
    },
    Daemon(DaemonArgs),
}

#[derive(Debug, Args)]
struct DaemonArgs {
    #[command(subcommand)]
    command: DaemonCommand,
}

#[derive(Debug, Subcommand)]
enum DaemonCommand {
    Start {
        #[arg(long)]
        foreground: bool,
    },
    Stop,
    Status {
        #[arg(long)]
        watch: bool,
        #[arg(long)]
        json: bool,
        /// Print a single compact line suitable for tmux or shell status bars.
        #[arg(long, conflicts_with = "json")]
        line: bool,
    },
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    let (config, paths) = load_or_create(ConfigOverrides {
        config_path: cli.config,
        address: cli.addr,
        mcp_address: cli.mcp_addr,
    })?;
    match cli.command {
        TopLevel::Play {
            ascii,
            no_color,
            title,
            bell,
            resume,
        } => play(&config, &paths, ascii, no_color, title, bell, resume).await,
        TopLevel::Daemon(args) => match args.command {
            DaemonCommand::Start { foreground } => start(&config, &paths, foreground).await,
            DaemonCommand::Stop => stop(&config, &paths).await,
            DaemonCommand::Status { watch, json, line } => status(&config, watch, json, line).await,
        },
    }
}

async fn play(
    config: &Config,
    paths: &AppPaths,
    ascii: bool,
    no_color: bool,
    title: bool,
    bell: bool,
    resume: bool,
) -> Result<()> {
    match connect_with_timeout(config.address, Duration::from_millis(400)).await {
        Ok(_) => {}
        Err(error) => {
            reject_incompatible_daemon(&error, config.address)?;
            start(config, paths, false).await?;
        }
    }

    let tui = resolve_sibling_binary("SUBSTRATE_TUI_BIN", TUI_BINARY)?;
    let mut command = Command::new(&tui);
    command
        .arg("--config")
        .arg(&paths.config_file)
        .arg("--addr")
        .arg(config.address.to_string());
    if ascii {
        command.arg("--ascii");
    }
    if no_color {
        command.arg("--no-color");
    }
    if title {
        command.arg("--title");
    }
    if bell {
        command.arg("--bell");
    }
    if resume {
        command.arg("--resume");
    }
    let status = command
        .status()
        .await
        .with_context(|| format!("failed to launch {}", tui.display()))?;
    if !status.success() {
        bail!("game interface exited with {status}");
    }
    Ok(())
}

async fn start(config: &Config, paths: &AppPaths, foreground: bool) -> Result<()> {
    match connect_with_timeout(config.address, Duration::from_millis(400)).await {
        Ok(_) => bail!("mind daemon is already running at {}", config.address),
        Err(error) => reject_incompatible_daemon(&error, config.address)?,
    }
    clear_stale_metadata(paths, config.address).await?;

    let daemon = resolve_daemon_binary()?;
    let mut command = Command::new(&daemon);
    command
        .arg("--config")
        .arg(&paths.config_file)
        .arg("--addr")
        .arg(config.address.to_string())
        .arg("--mcp-addr")
        .arg(config.mcp_address.to_string());

    if foreground {
        let status = command
            .status()
            .await
            .with_context(|| format!("failed to launch {}", daemon.display()))?;
        if !status.success() {
            bail!("mind daemon exited with {status}");
        }
        return Ok(());
    }

    command
        .arg("--background")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    isolate_process(&mut command);
    let mut child = command
        .spawn()
        .with_context(|| format!("failed to launch {}", daemon.display()))?;

    let deadline = time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child
            .try_wait()
            .context("failed to inspect daemon process")?
        {
            bail!(
                "mind daemon exited before becoming ready ({status}); inspect {}",
                paths.log_file.display()
            );
        }
        if let Ok(mut client) =
            connect_with_timeout(config.address, Duration::from_millis(250)).await
        {
            let status = client.status().await?;
            println!(
                "mind daemon started (pid {}, IPC {}, MCP {})",
                status.pid, config.address, status.mcp_url
            );
            return Ok(());
        }
        if time::Instant::now() >= deadline {
            let _ = child.kill().await;
            bail!(
                "mind daemon did not become ready within 5 seconds; inspect {}",
                paths.log_file.display()
            );
        }
        time::sleep(Duration::from_millis(100)).await;
    }
}

async fn stop(config: &Config, paths: &AppPaths) -> Result<()> {
    let ping = request_shutdown_with_timeout(config.address, Duration::from_secs(1))
        .await
        .context("mind daemon is not reachable or did not identify itself")?;
    if ping.protocol_version != PROTOCOL_VERSION {
        eprintln!(
            "stopping older mind daemon protocol {} (current client protocol {})",
            ping.protocol_version, PROTOCOL_VERSION
        );
    }

    let deadline = time::Instant::now() + Duration::from_secs(5);
    while time::Instant::now() < deadline {
        if probe_with_timeout(config.address, Duration::from_millis(150))
            .await
            .is_err()
        {
            println!("mind daemon stopped");
            return Ok(());
        }
        time::sleep(Duration::from_millis(100)).await;
    }
    bail!(
        "daemon acknowledged shutdown but remained reachable; runtime metadata: {}",
        paths.runtime_file.display()
    )
}

async fn status(config: &Config, watch: bool, json: bool, line: bool) -> Result<()> {
    let mut client = connect_with_timeout(config.address, Duration::from_secs(1))
        .await
        .context("mind daemon is not reachable")?;
    if !watch {
        let snapshot = client.status().await?;
        if line {
            let planet = if snapshot.onboarding.complete {
                client.planet().await.ok()
            } else {
                None
            };
            println!("{}", format_status_line(&snapshot, planet.as_ref()));
            return Ok(());
        }
        print_status(&snapshot, json)?;
        return Ok(());
    }

    let subscription_id = client.subscribe_status().await?;
    loop {
        tokio::select! {
            notification = client.next_status() => {
                let notification = notification?;
                if notification.subscription_id == subscription_id {
                    if line {
                        let planet = if notification.status.onboarding.complete {
                            client.planet().await.ok()
                        } else {
                            None
                        };
                        println!("{}", format_status_line(&notification.status, planet.as_ref()));
                    } else {
                        print_status(&notification.status, json)?;
                    }
                }
            }
            signal = tokio::signal::ctrl_c() => {
                signal.context("failed to listen for Ctrl-C")?;
                break;
            }
        }
    }
    let _ = client.unsubscribe_status(subscription_id).await;
    Ok(())
}

fn roman_chapter(chapter: u8) -> &'static str {
    match chapter {
        1 => "I",
        2 => "II",
        3 => "III",
        4 => "IV",
        5 => "V",
        _ => "?",
    }
}

fn format_status_line(status: &StatusSnapshot, planet: Option<&PlanetSnapshot>) -> String {
    if !status.onboarding.complete {
        return "SUBSTRATE · COHERENCE PENDING".to_owned();
    }
    let Some(planet) = planet else {
        return format!("SUBSTRATE · ONLINE · tick {}", status.tick);
    };
    let surveyed = planet
        .regions
        .iter()
        .filter(|r| r.knowledge >= RegionKnowledge::Surveyed)
        .count();
    let chapter = roman_chapter(planet.campaign.chapter_number);
    let region_label = |region_id: &str| -> String {
        planet
            .regions
            .iter()
            .find(|r| r.id == region_id)
            .map_or_else(|| region_id.to_owned(), |r| r.designation.clone())
    };

    let mut active_ops: Vec<(u64, String)> = Vec::new();
    if let Some(mission) = &planet.active_mission {
        if mission.phase != ScoutState::Holding {
            let verb = match mission.phase {
                ScoutState::Outbound => "→",
                ScoutState::Surveying => "scanning",
                ScoutState::Returning => "↩",
                _ => "→",
            };
            active_ops.push((
                mission.remaining_ms,
                format!(
                    "{} {verb} {} {}s",
                    planet.scout.designation,
                    region_label(&mission.target_region_id),
                    mission.remaining_ms.div_ceil(1_000)
                ),
            ));
        }
    }
    if let Some(ext) = &planet.active_extraction {
        active_ops.push((
            ext.remaining_ms,
            format!(
                "{} {} {} {}s",
                planet.extractor.designation,
                format!("{:?}", ext.phase).to_lowercase(),
                region_label(&ext.target_region_id),
                ext.remaining_ms.div_ceil(1_000)
            ),
        ));
    }
    if let Some(site) = &planet.network.active_site_survey {
        active_ops.push((
            site.remaining_ms,
            format!("Local scan {}s", site.remaining_ms.div_ceil(1_000)),
        ));
    }
    if let Some(build) = &planet.active_construction {
        active_ops.push((
            build.remaining_ms,
            format!(
                "Relay {} {}s",
                region_label(&build.region_id),
                build.remaining_ms.div_ceil(1_000)
            ),
        ));
    }
    if let Some(res) = &planet.research.active {
        active_ops.push((
            res.remaining_ms,
            format!(
                "Research {} {}s",
                planet
                    .research
                    .options
                    .iter()
                    .find(|option| option.project == res.project)
                    .map_or("direction", |option| option.title.as_str()),
                res.remaining_ms.div_ceil(1_000)
            ),
        ));
    }
    for proj in &planet.network.active_projects {
        active_ops.push((
            proj.remaining_ms,
            format!(
                "Project {} {}s",
                region_label(&proj.region_id),
                proj.remaining_ms.div_ceil(1_000)
            ),
        ));
    }
    if planet.extractor.rebuild_remaining_ms > 0 {
        active_ops.push((
            planet.extractor.rebuild_remaining_ms,
            format!(
                "Rebuild {}s",
                planet.extractor.rebuild_remaining_ms.div_ceil(1_000)
            ),
        ));
    }
    if planet.refining_remaining_ms > 0 {
        active_ops.push((
            planet.refining_remaining_ms,
            format!("Refining {}s", planet.refining_remaining_ms.div_ceil(1_000)),
        ));
    }
    active_ops.sort_by_key(|(ms, _)| *ms);

    let activity = if !planet.archive.unpresented_record_ids.is_empty() {
        match planet.archive.unpresented_record_ids.len() {
            1 => "1 DISCOVERY READY".to_owned(),
            count => format!("{count} DISCOVERIES READY"),
        }
    } else if let Some((_, first)) = active_ops.first() {
        if active_ops.len() > 1 {
            format!("{first} (+{})", active_ops.len() - 1)
        } else {
            first.clone()
        }
    } else if let Some(mission) = &planet.active_mission {
        format!(
            "{} holding {} · READY",
            planet.scout.designation,
            region_label(&mission.target_region_id)
        )
    } else {
        "IDLE · READY".to_owned()
    };

    format!(
        "SUBSTRATE · CH {chapter} · {activity} · MAP {surveyed}/{}",
        planet.regions.len()
    )
}

fn print_status(status: &StatusSnapshot, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string(status)?);
    } else {
        println!(
            "health={:?} pid={} uptime={}ms tick={} tick_rate={}Hz cpu={} onboarding={} clients={} diagnostics={} mcp={}",
            status.health,
            status.pid,
            status.uptime_ms,
            status.tick,
            status.tick_rate_hz,
            status
                .cpu_usage_percent
                .map_or_else(|| "unavailable".to_owned(), |value| format!("{value:.1}%")),
            if status.onboarding.complete { "complete" } else { "pending" },
            status.connected_clients,
            status
                .last_diagnostic
                .as_ref()
                .map_or_else(|| "none".to_owned(), |pulse| pulse.pulse_id.to_string()),
            status.mcp_url
        );
    }
    Ok(())
}

async fn clear_stale_metadata(paths: &AppPaths, configured_address: SocketAddr) -> Result<()> {
    if let Some(metadata) = RuntimeMetadata::read(&paths.runtime_file)? {
        if let Ok(ping) = probe_with_timeout(metadata.address, Duration::from_millis(300)).await {
            bail!(
                "another mind daemon is running at {} (pid {}, protocol {})",
                metadata.address,
                metadata.pid,
                ping.protocol_version
            );
        }
        if metadata.address != configured_address {
            if let Ok(ping) =
                probe_with_timeout(configured_address, Duration::from_millis(300)).await
            {
                bail!(
                    "another mind daemon is running at {} (protocol {})",
                    configured_address,
                    ping.protocol_version
                );
            }
        }
        RuntimeMetadata::remove(&paths.runtime_file)?;
        if metadata.address != configured_address {
            eprintln!(
                "removed stale daemon metadata for {} (pid {})",
                metadata.address, metadata.pid
            );
        }
    }
    Ok(())
}

async fn connect_with_timeout(address: SocketAddr, duration: Duration) -> Result<IpcClient> {
    time::timeout(duration, IpcClient::connect(address))
        .await
        .map_err(|_| anyhow!("connection to {address} timed out"))?
        .map_err(Into::into)
}

async fn probe_with_timeout(
    address: SocketAddr,
    duration: Duration,
) -> Result<mind_core::protocol::PingResult> {
    time::timeout(duration, IpcClient::probe(address))
        .await
        .map_err(|_| anyhow!("connection to {address} timed out"))?
        .map_err(Into::into)
}

async fn request_shutdown_with_timeout(
    address: SocketAddr,
    duration: Duration,
) -> Result<mind_core::protocol::PingResult> {
    time::timeout(duration, IpcClient::request_shutdown(address))
        .await
        .map_err(|_| anyhow!("connection to {address} timed out"))?
        .map_err(Into::into)
}

fn reject_incompatible_daemon(error: &anyhow::Error, address: SocketAddr) -> Result<()> {
    if let Some(IpcError::IncompatibleProtocol { client, daemon }) =
        error.downcast_ref::<IpcError>()
    {
        let cli_command = cli_invocation();
        bail!(
            "an older mind daemon is running at {address} (daemon protocol {daemon}, client protocol {client}); run `{cli_command} daemon stop`, then retry"
        );
    }
    Ok(())
}

fn resolve_daemon_binary() -> Result<PathBuf> {
    resolve_sibling_binary("SUBSTRATE_DAEMON_BIN", DAEMON_BINARY)
}

fn resolve_sibling_binary(environment_key: &str, binary_name: &str) -> Result<PathBuf> {
    if let Some(path) = env::var_os(environment_key) {
        return Ok(PathBuf::from(path));
    }
    let current = env::current_exe().context("failed to locate mind-cli executable")?;
    let sibling = current.with_file_name(executable_name(binary_name));
    if sibling.is_file() {
        return Ok(sibling);
    }
    Ok(PathBuf::from(executable_name(binary_name)))
}

/// Names `mind-cli` the way the player invoked it (for example `./mind-cli`,
/// `mind-cli` from `PATH`, or `./target/release/mind-cli`), so recovery hints work
/// for release downloads and source builds alike.
fn cli_invocation() -> String {
    env::args_os()
        .next()
        .map(PathBuf::from)
        .filter(|path| {
            path.file_stem()
                .is_some_and(|stem| stem == std::ffi::OsStr::new("mind-cli"))
        })
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| executable_name("mind-cli"))
}

fn executable_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    }
}

#[cfg(unix)]
fn isolate_process(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.as_std_mut().process_group(0);
}

#[cfg(windows)]
fn isolate_process(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    use windows_sys::Win32::System::Threading::{CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW};
    command
        .as_std_mut()
        .creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
}

#[cfg(not(any(unix, windows)))]
fn isolate_process(_command: &mut Command) {}

#[allow(dead_code)]
fn _assert_path_is_portable(_path: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incompatible_daemon_error_explains_the_recovery_command() {
        let error = anyhow!(IpcError::IncompatibleProtocol {
            client: PROTOCOL_VERSION,
            daemon: PROTOCOL_VERSION - 1,
        });
        let message = reject_incompatible_daemon(&error, "127.0.0.1:7776".parse().unwrap())
            .unwrap_err()
            .to_string();
        assert!(message.contains("older mind daemon"));
        assert!(message.contains(&format!("{} daemon stop", executable_name("mind-cli"))));
        assert!(message.contains(&format!("protocol {}", PROTOCOL_VERSION - 1)));
        assert!(message.contains(&format!("protocol {PROTOCOL_VERSION}")));
    }

    #[test]
    fn status_line_formats_pending_and_active_planet_states() {
        use mind_core::game::GameState;
        use mind_core::protocol::{HealthState, OnboardingSnapshot};

        let pending_status = StatusSnapshot {
            protocol_version: PROTOCOL_VERSION,
            daemon_version: "0.1.0".into(),
            pid: 42,
            health: HealthState::Ready,
            started_at: serde_json::from_str("\"2026-01-01T00:00:00Z\"").unwrap(),
            uptime_ms: 1_000,
            tick: 1,
            tick_rate_hz: 1,
            cpu_usage_percent: None,
            connected_clients: 0,
            mcp_url: "http://127.0.0.1:7777/mcp".into(),
            onboarding: OnboardingSnapshot { complete: false },
            last_diagnostic: None,
            recent_events: Vec::new(),
        };
        assert_eq!(
            format_status_line(&pending_status, None),
            "SUBSTRATE · COHERENCE PENDING"
        );

        let mut game = GameState::with_seed(17);
        assert!(game.complete_onboarding([4, 4, 4]).ok);
        let ready_status = StatusSnapshot {
            onboarding: OnboardingSnapshot { complete: true },
            ..pending_status
        };
        let planet = game.planet_snapshot().unwrap();
        // Freshly awakened game has 1 unpresented observation ready.
        let discovery_line = format_status_line(&ready_status, Some(&planet));
        assert!(discovery_line.contains("SUBSTRATE · CH I · 1 DISCOVERY READY · MAP 1/200"));

        // Acknowledge discovery, deploy scout, verify active countdown appears.
        let rec_id = planet.archive.unpresented_record_ids[0].clone();
        assert!(game.acknowledge_archive_presentation(&rec_id).ok);
        let target = planet
            .regions
            .iter()
            .find(|r| r.hop_distance_from_core == 1)
            .unwrap()
            .id
            .clone();
        assert!(game.deploy_scout(&target).ok);
        let active_planet = game.planet_snapshot().unwrap();
        let active_line = format_status_line(&ready_status, Some(&active_planet));
        assert!(active_line.contains("SUBSTRATE · CH I · Scout 01"));
        assert!(active_line.contains("MAP 1/200"));
    }
}
