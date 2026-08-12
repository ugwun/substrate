use anyhow::{Context, Result};
use clap::Parser;
use mind_core::config::{load_or_create, ConfigOverrides};
use std::{net::SocketAddr, path::PathBuf};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(version, about = "Substrate simulation daemon")]
struct Args {
    #[arg(long, env = "SUBSTRATE_CONFIG")]
    config: Option<PathBuf>,
    #[arg(long, env = "SUBSTRATE_ADDR")]
    addr: Option<SocketAddr>,
    #[arg(long, env = "SUBSTRATE_MCP_ADDR")]
    mcp_addr: Option<SocketAddr>,
    #[arg(long, hide = true)]
    background: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let (config, paths) = load_or_create(ConfigOverrides {
        config_path: args.config,
        address: args.addr,
        mcp_address: args.mcp_addr,
    })?;
    init_logging(args.background, &paths)?;
    let result = mind_daemon::run(config, paths).await;
    if let Err(error) = &result {
        tracing::error!(error = %format!("{error:#}"), "mind daemon failed");
    }
    result
}

fn init_logging(background: bool, paths: &mind_core::config::AppPaths) -> Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    if background {
        let directory = paths
            .log_file
            .parent()
            .context("log path has no parent directory")?;
        let filename = paths
            .log_file
            .file_name()
            .context("log path has no filename")?;
        let appender = tracing_appender::rolling::never(directory, filename);
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_ansi(false)
            .with_writer(appender)
            .init();
    } else {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    }
    Ok(())
}
