use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use mind_core::{
    config::{load_or_create, AppPaths, Config, ConfigOverrides},
    ipc::{IpcClient, IpcError},
    protocol::StatusSnapshot,
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
        TopLevel::Play { ascii, no_color } => play(&config, &paths, ascii, no_color).await,
        TopLevel::Daemon(args) => match args.command {
            DaemonCommand::Start { foreground } => start(&config, &paths, foreground).await,
            DaemonCommand::Stop => stop(&config, &paths).await,
            DaemonCommand::Status { watch, json } => status(&config, watch, json).await,
        },
    }
}

async fn play(config: &Config, paths: &AppPaths, ascii: bool, no_color: bool) -> Result<()> {
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

async fn status(config: &Config, watch: bool, json: bool) -> Result<()> {
    let mut client = connect_with_timeout(config.address, Duration::from_secs(1))
        .await
        .context("mind daemon is not reachable")?;
    if !watch {
        print_status(&client.status().await?, json)?;
        return Ok(());
    }

    let subscription_id = client.subscribe_status().await?;
    loop {
        tokio::select! {
            notification = client.next_status() => {
                let notification = notification?;
                if notification.subscription_id == subscription_id {
                    print_status(&notification.status, json)?;
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
        bail!(
            "an older mind daemon is running at {address} (daemon protocol {daemon}, client protocol {client}); run `./target/debug/mind-cli daemon stop`, then retry"
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
    use windows_sys::Win32::System::Threading::{
        CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW, DETACHED_PROCESS,
    };
    command
        .as_std_mut()
        .creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW | DETACHED_PROCESS);
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
        assert!(message.contains("mind-cli daemon stop"));
        assert!(message.contains(&format!("protocol {}", PROTOCOL_VERSION - 1)));
        assert!(message.contains(&format!("protocol {PROTOCOL_VERSION}")));
    }
}
