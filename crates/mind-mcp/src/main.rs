use anyhow::{Context, Result};
use clap::Parser;
use mind_core::{
    config::{load_or_create, ConfigOverrides},
    ipc::IpcClient,
};
use rmcp::{transport::stdio, ServiceExt};
use std::{net::SocketAddr, path::PathBuf};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(version, about = "Substrate MCP stdio bridge")]
struct Args {
    #[arg(long, env = "SUBSTRATE_CONFIG")]
    config: Option<PathBuf>,
    #[arg(long, env = "SUBSTRATE_ADDR")]
    addr: Option<SocketAddr>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let args = Args::parse();
    let (config, _) = load_or_create(ConfigOverrides {
        config_path: args.config,
        address: args.addr,
        mcp_address: None,
    })?;
    IpcClient::connect(config.address)
        .await
        .with_context(|| format!("mind daemon is not reachable at {}", config.address))?;

    let service = mind_mcp::MindMcpServer::ipc(config.address)
        .serve(stdio())
        .await
        .context("failed to start MCP stdio transport")?;
    service
        .waiting()
        .await
        .context("MCP stdio transport failed")?;
    Ok(())
}
