//! MCP adapters shared by the daemon's HTTP endpoint and the stdio bridge.

use axum::Router;
use mind_core::{
    engine::MindEngine,
    ipc::IpcClient,
    protocol::{DiagnosticPulseResult, StatusSnapshot},
};
use rmcp::{
    handler::server::{router::tool::ToolRouter, tool::schema_for_output},
    model::{
        CallToolResult, ContentBlock, Implementation, ProtocolVersion, ServerCapabilities,
        ServerInfo,
    },
    tool, tool_handler, tool_router,
    transport::streamable_http_server::{
        session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
    },
    ServerHandler,
};
use serde::Serialize;
use std::{borrow::Cow, net::SocketAddr, sync::Arc};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

pub const MCP_PATH: &str = "/mcp";
const CURRENT_PROTOCOLS: &[ProtocolVersion] = &[ProtocolVersion::V_2026_07_28];

#[derive(Clone)]
enum MindBackend {
    Direct(Arc<MindEngine>),
    Ipc(SocketAddr),
}

impl MindBackend {
    async fn status(&self) -> Result<StatusSnapshot, String> {
        match self {
            Self::Direct(engine) => Ok(engine.status()),
            Self::Ipc(address) => {
                let mut client = IpcClient::connect(*address)
                    .await
                    .map_err(|error| format!("mind daemon is unavailable: {error}"))?;
                client
                    .status()
                    .await
                    .map_err(|error| format!("could not read mind status: {error}"))
            }
        }
    }

    async fn trigger_diagnostic_pulse(&self) -> Result<DiagnosticPulseResult, String> {
        match self {
            Self::Direct(engine) => Ok(engine.trigger_diagnostic_pulse()),
            Self::Ipc(address) => {
                let mut client = IpcClient::connect(*address)
                    .await
                    .map_err(|error| format!("mind daemon is unavailable: {error}"))?;
                client
                    .trigger_diagnostic_pulse()
                    .await
                    .map_err(|error| format!("diagnostic pulse failed: {error}"))
            }
        }
    }
}

#[derive(Clone)]
pub struct MindMcpServer {
    backend: MindBackend,
    tool_router: ToolRouter<Self>,
}

impl MindMcpServer {
    pub fn direct(engine: Arc<MindEngine>) -> Self {
        Self::new(MindBackend::Direct(engine))
    }

    pub fn ipc(address: SocketAddr) -> Self {
        Self::new(MindBackend::Ipc(address))
    }

    fn new(backend: MindBackend) -> Self {
        Self {
            backend,
            tool_router: Self::tool_router(),
        }
    }

    pub fn tool_count(&self) -> usize {
        self.tool_router.list_all().len()
    }
}

#[tool_router(router = tool_router)]
impl MindMcpServer {
    #[tool(
        name = "get_mind_status",
        description = "Read the Substrate mind daemon's operational state, latest diagnostic, and recent engine events",
        output_schema = schema_for_output::<StatusSnapshot>()
    )]
    async fn get_mind_status(&self) -> CallToolResult {
        match self.backend.status().await {
            Ok(status) => structured_result(
                format!(
                    "Mind is {:?} at tick {} ({} Hz); latest diagnostic: {}",
                    status.health,
                    status.tick,
                    status.tick_rate_hz,
                    status
                        .last_diagnostic
                        .as_ref()
                        .map_or_else(|| "none".to_owned(), |pulse| pulse.pulse_id.to_string())
                ),
                &status,
            ),
            Err(error) => CallToolResult::error(vec![ContentBlock::text(error)]),
        }
    }

    #[tool(
        name = "trigger_diagnostic_pulse",
        description = "Run and record a baseline self-test pulse against the awakening mind",
        output_schema = schema_for_output::<DiagnosticPulseResult>()
    )]
    async fn trigger_diagnostic_pulse(&self) -> CallToolResult {
        match self.backend.trigger_diagnostic_pulse().await {
            Ok(pulse) => structured_result(
                format!(
                    "Diagnostic pulse {} completed at tick {} with health {:?}",
                    pulse.pulse_id, pulse.tick, pulse.health
                ),
                &pulse,
            ),
            Err(error) => CallToolResult::error(vec![ContentBlock::text(error)]),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for MindMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("substrate", env!("CARGO_PKG_VERSION")))
            .with_instructions("Inspect the mind daemon and run diagnostic pulses.")
    }

    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        Cow::Borrowed(CURRENT_PROTOCOLS)
    }
}

fn structured_result<T: Serialize>(summary: String, value: &T) -> CallToolResult {
    match serde_json::to_value(value) {
        Ok(value) => {
            let mut result = CallToolResult::structured(value);
            result.content = vec![ContentBlock::text(summary)];
            result
        }
        Err(error) => CallToolResult::error(vec![ContentBlock::text(format!(
            "could not serialize tool result: {error}"
        ))]),
    }
}

pub fn http_router(
    engine: Arc<MindEngine>,
    address: SocketAddr,
    cancellation_token: CancellationToken,
) -> Router {
    let port = address.port();
    let allowed_hosts = vec![address.to_string(), format!("localhost:{port}")];
    let allowed_origins = vec![
        format!("http://{address}"),
        format!("http://localhost:{port}"),
    ];
    let config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(false)
        .with_json_response(true)
        .with_stateless_protocol_metadata_required(true)
        .with_allowed_hosts(allowed_hosts)
        .with_allowed_origins(allowed_origins)
        .with_cancellation_token(cancellation_token);
    let handler = MindMcpServer::direct(engine);
    let service: StreamableHttpService<MindMcpServer, LocalSessionManager> =
        StreamableHttpService::new(move || Ok(handler.clone()), Default::default(), config);
    Router::new().nest_service(MCP_PATH, service)
}

pub async fn serve_http(
    listener: TcpListener,
    engine: Arc<MindEngine>,
    cancellation_token: CancellationToken,
) -> std::io::Result<()> {
    let address = listener.local_addr()?;
    let router = http_router(engine, address, cancellation_token.clone());
    axum::serve(listener, router)
        .with_graceful_shutdown(cancellation_token.cancelled_owned())
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use mind_core::protocol::{method, success, PingResult, RpcRequest};
    use rmcp::{
        model::{CallToolRequestParams, ClientInfo},
        transport::StreamableHttpClientTransport,
        ClientLifecycleMode, ClientServiceExt, ServiceExt,
    };
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    #[test]
    fn exposes_only_the_phase_zero_tools() {
        let server = MindMcpServer::direct(MindEngine::new(20));
        let mut names: Vec<_> = server
            .tool_router
            .list_all()
            .into_iter()
            .map(|tool| tool.name.into_owned())
            .collect();
        names.sort_unstable();
        assert_eq!(names, ["get_mind_status", "trigger_diagnostic_pulse"]);
        assert!(server
            .tool_router
            .list_all()
            .iter()
            .all(|tool| tool.output_schema.is_some()));
    }

    #[tokio::test]
    async fn ipc_backend_adapts_status_and_diagnostics() -> anyhow::Result<()> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let engine = MindEngine::new(20);
        let server_engine = engine.clone();
        let ipc_server = tokio::spawn(async move {
            for _ in 0..2 {
                let (stream, _) = listener.accept().await?;
                let (reader, mut writer) = stream.into_split();
                let mut reader = BufReader::new(reader);
                let mut frame = String::new();
                reader.read_line(&mut frame).await?;
                let ping: RpcRequest = serde_json::from_str(&frame)?;
                let mut response = serde_json::to_vec(&success(ping.id, PingResult::current()))?;
                response.push(b'\n');
                writer.write_all(&response).await?;

                frame.clear();
                reader.read_line(&mut frame).await?;
                let request: RpcRequest = serde_json::from_str(&frame)?;
                let value = match request.method.as_str() {
                    method::GET_STATUS => success(request.id, server_engine.status()),
                    method::TRIGGER_DIAGNOSTIC_PULSE => {
                        success(request.id, server_engine.trigger_diagnostic_pulse())
                    }
                    _ => unreachable!("unexpected IPC method"),
                };
                let mut response = serde_json::to_vec(&value)?;
                response.push(b'\n');
                writer.write_all(&response).await?;
            }
            anyhow::Ok(())
        });

        let server = MindMcpServer::ipc(address);
        assert_eq!(
            server
                .backend
                .status()
                .await
                .map_err(anyhow::Error::msg)?
                .protocol_version,
            5
        );
        assert_eq!(
            server
                .backend
                .trigger_diagnostic_pulse()
                .await
                .map_err(anyhow::Error::msg)?
                .pulse_id,
            1
        );
        ipc_server.await??;
        Ok(())
    }

    #[tokio::test]
    async fn current_client_discovers_and_calls_shared_tools() -> anyhow::Result<()> {
        let engine = MindEngine::new(20);
        let (server_transport, client_transport) = tokio::io::duplex(16 * 1024);
        let server = MindMcpServer::direct(engine);
        let server_task = tokio::spawn(async move {
            server.serve(server_transport).await?.waiting().await?;
            anyhow::Ok(())
        });
        let client = ClientInfo::default()
            .serve_with_lifecycle(
                client_transport,
                ClientLifecycleMode::Discover {
                    preferred_versions: vec![ProtocolVersion::V_2026_07_28],
                },
            )
            .await?;

        let mut tools = client.list_all_tools().await?;
        tools.sort_by(|left, right| left.name.cmp(&right.name));
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name, "get_mind_status");
        assert_eq!(tools[1].name, "trigger_diagnostic_pulse");
        assert!(tools.iter().all(|tool| tool.output_schema.is_some()));

        let pulse = client
            .call_tool(CallToolRequestParams::new("trigger_diagnostic_pulse"))
            .await?;
        assert_eq!(pulse.structured_content.as_ref().unwrap()["pulse_id"], 1);
        assert!(pulse.content[0]
            .as_text()
            .unwrap()
            .text
            .contains("Diagnostic pulse 1"));
        let status = client
            .call_tool(CallToolRequestParams::new("get_mind_status"))
            .await?;
        assert_eq!(
            status.structured_content.as_ref().unwrap()["last_diagnostic"]["pulse_id"],
            1
        );

        client.cancel().await?;
        server_task.await??;
        Ok(())
    }

    #[tokio::test]
    async fn rejects_legacy_protocol_discovery() -> anyhow::Result<()> {
        let (server_transport, client_transport) = tokio::io::duplex(8 * 1024);
        let server_task = tokio::spawn(async move {
            let _ = MindMcpServer::direct(MindEngine::new(20))
                .serve(server_transport)
                .await;
        });
        let result = ClientInfo::default()
            .serve_with_lifecycle(
                client_transport,
                ClientLifecycleMode::Discover {
                    preferred_versions: vec![ProtocolVersion::V_2025_11_25],
                },
            )
            .await;
        assert!(result.is_err());
        server_task.await?;
        Ok(())
    }

    #[tokio::test]
    async fn streamable_http_accepts_current_clients_and_rejects_foreign_origins(
    ) -> anyhow::Result<()> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let cancellation = CancellationToken::new();
        let server_task = tokio::spawn(serve_http(
            listener,
            MindEngine::with_mcp_address(20, address),
            cancellation.clone(),
        ));
        let url = format!("http://{address}{MCP_PATH}");

        let client = ClientInfo::default()
            .serve_with_lifecycle(
                StreamableHttpClientTransport::from_uri(url.clone()),
                ClientLifecycleMode::Discover {
                    preferred_versions: vec![ProtocolVersion::V_2026_07_28],
                },
            )
            .await?;
        assert_eq!(client.list_all_tools().await?.len(), 2);
        client.cancel().await?;

        let discovery = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "server/discover",
            "params": {
                "_meta": {
                    "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                    "io.modelcontextprotocol/clientInfo": {"name": "security-test", "version": "1"},
                    "io.modelcontextprotocol/clientCapabilities": {}
                }
            }
        });
        let response = reqwest::Client::new()
            .post(&url)
            .header("Accept", "application/json, text/event-stream")
            .header("MCP-Protocol-Version", "2026-07-28")
            .header("Mcp-Method", "server/discover")
            .header("Origin", "https://malicious.example")
            .json(&discovery)
            .send()
            .await?;
        assert_eq!(response.status(), reqwest::StatusCode::FORBIDDEN);

        cancellation.cancel();
        server_task.await??;
        Ok(())
    }
}
