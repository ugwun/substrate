use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use mind_core::{
    config::{load_or_create, AppPaths, Config, ConfigOverrides},
    ipc::IpcClient,
    protocol::StatusSnapshot,
    runtime::RuntimeMetadata,
    DAEMON_BINARY,
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
        TopLevel::Daemon(args) => match args.command {
            DaemonCommand::Start { foreground } => start(&config, &paths, foreground).await,
            DaemonCommand::Stop => stop(&config, &paths).await,
            DaemonCommand::Status { watch, json } => status(&config, watch, json).await,
        },
    }
}

async fn start(config: &Config, paths: &AppPaths, foreground: bool) -> Result<()> {
    if connect_with_timeout(config.address, Duration::from_millis(400))
        .await
        .is_ok()
    {
        bail!("mind daemon is already running at {}", config.address);
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
    let mut client = connect_with_timeout(config.address, Duration::from_secs(1))
        .await
        .context("mind daemon is not reachable")?;
    client.shutdown().await.context("shutdown request failed")?;
    drop(client);

    let deadline = time::Instant::now() + Duration::from_secs(5);
    while time::Instant::now() < deadline {
        if connect_with_timeout(config.address, Duration::from_millis(150))
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
            "health={:?} pid={} uptime={}ms tick={} tick_rate={}Hz cpu={} clients={} diagnostics={} mcp={}",
            status.health,
            status.pid,
            status.uptime_ms,
            status.tick,
            status.tick_rate_hz,
            status
                .cpu_usage_percent
                .map_or_else(|| "unavailable".to_owned(), |value| format!("{value:.1}%")),
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
        if connect_with_timeout(metadata.address, Duration::from_millis(300))
            .await
            .is_ok()
        {
            bail!(
                "another mind daemon is running at {} (pid {})",
                metadata.address,
                metadata.pid
            );
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

fn resolve_daemon_binary() -> Result<PathBuf> {
    if let Some(path) = env::var_os("SUBSTRATE_DAEMON_BIN") {
        return Ok(PathBuf::from(path));
    }
    let current = env::current_exe().context("failed to locate mind-cli executable")?;
    let sibling = current.with_file_name(executable_name(DAEMON_BINARY));
    if sibling.is_file() {
        return Ok(sibling);
    }
    Ok(PathBuf::from(executable_name(DAEMON_BINARY)))
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
